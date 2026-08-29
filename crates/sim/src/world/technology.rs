//! Database-backed player technology activation and live entity updates.

use super::World;
use crate::entity_id::EntityId;
use crate::player::{AppliedSquadTransform, PlayerId};
use crate::scenario::{add_squad_member_from_prototype, refresh_squad_member_settings};
use pipeline::database::hw1::{Database, Squad as ProtoSquad};

/// Failure to change one player's technology state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TechnologyError {
    /// The target player is not present in the world.
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    /// The layered database has no technology with this name.
    #[error("technology '{0}' was not found")]
    TechnologyNotFound(String),
}

#[derive(Debug, Clone, Copy)]
struct HitpointSnapshot {
    unit_id: EntityId,
    base: f32,
    previous: f32,
}

#[derive(Debug, Clone, Copy)]
struct ShieldpointSnapshot {
    unit_id: EntityId,
    base: f32,
    previous: f32,
}

impl World {
    /// Activate one database technology for a player.
    ///
    /// Returns `Ok(false)` when that technology is already active. Supported
    /// player-proto effects immediately update existing entities, while future
    /// spawns read the same player state during prototype configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown player or technology name.
    pub fn activate_technology(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        technology_name: &str,
    ) -> Result<bool, TechnologyError> {
        let technology = find_technology(database, technology_name).ok_or_else(|| {
            TechnologyError::TechnologyNotFound(technology_name.trim().to_owned())
        })?;
        let Some(player) = self.get_player(player_id) else {
            return Err(TechnologyError::PlayerNotFound(player_id));
        };
        if player.technologies.is_active(&technology.name) {
            return Ok(false);
        }

        let hitpoints = self.hitpoint_snapshots(player_id, database);
        let shieldpoints = self.shieldpoint_snapshots(player_id, database);
        let Some(player) = self.get_player_mut(player_id) else {
            return Err(TechnologyError::PlayerNotFound(player_id));
        };
        let transforms = player.technologies.activate(database, technology);
        self.apply_hitpoint_changes(player_id, &hitpoints);
        self.apply_shieldpoint_changes(player_id, &shieldpoints);
        self.refresh_shield_regen_scalars(player_id);
        for transform in transforms {
            self.apply_squad_transform(player_id, database, &transform);
        }
        Ok(true)
    }

    /// Deactivate one player technology and rebuild reversible proto effects.
    ///
    /// Retail proto-squad transformations are intentionally permanent when a
    /// technology is unapplied; action, damage, HP, and recovery modifiers are
    /// rebuilt from the remaining active technologies.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown player or technology name.
    pub fn deactivate_technology(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        technology_name: &str,
    ) -> Result<bool, TechnologyError> {
        let technology = find_technology(database, technology_name).ok_or_else(|| {
            TechnologyError::TechnologyNotFound(technology_name.trim().to_owned())
        })?;
        if self.get_player(player_id).is_none() {
            return Err(TechnologyError::PlayerNotFound(player_id));
        }
        let hitpoints = self.hitpoint_snapshots(player_id, database);
        let shieldpoints = self.shieldpoint_snapshots(player_id, database);
        let Some(player) = self.get_player_mut(player_id) else {
            return Err(TechnologyError::PlayerNotFound(player_id));
        };
        let changed = player.technologies.deactivate(database, &technology.name);
        if changed {
            self.apply_hitpoint_changes(player_id, &hitpoints);
            self.apply_shieldpoint_changes(player_id, &shieldpoints);
            self.refresh_shield_regen_scalars(player_id);
        }
        Ok(changed)
    }

    fn hitpoint_snapshots(
        &self,
        player_id: PlayerId,
        database: &Database,
    ) -> Vec<HitpointSnapshot> {
        let Some(player) = self.get_player(player_id) else {
            return Vec::new();
        };
        self.units
            .iter()
            .filter(|(_, unit)| unit.base.player_id == player_id)
            .filter_map(|(unit_id, unit)| {
                let base = database
                    .objects
                    .iter()
                    .find(|proto| proto.name.eq_ignore_ascii_case(&unit.proto_object_name))?
                    .hitpoints
                    .filter(|hitpoints| hitpoints.is_finite() && *hitpoints > 0.0)?;
                Some(HitpointSnapshot {
                    unit_id,
                    base,
                    previous: player.technologies.hitpoints(&unit.proto_object_name, base),
                })
            })
            .collect()
    }

    fn apply_hitpoint_changes(&mut self, player_id: PlayerId, snapshots: &[HitpointSnapshot]) {
        for snapshot in snapshots {
            let Some(unit) = self.units.get(snapshot.unit_id) else {
                continue;
            };
            let next = self
                .get_player(player_id)
                .expect("validated player")
                .technologies
                .hitpoints(&unit.proto_object_name, snapshot.base);
            let unchanged_tolerance =
                f32::EPSILON * next.abs().max(snapshot.previous.abs()).max(1.0);
            if snapshot.previous <= 0.0 || (next - snapshot.previous).abs() <= unchanged_tolerance {
                continue;
            }
            let ratio = next / snapshot.previous;
            if let Some(unit) = self.units.get_mut(snapshot.unit_id) {
                unit.hitpoints = (unit.hitpoints * ratio).clamp(0.0, next);
                unit.max_hitpoints = next;
            }
        }
    }

    fn shieldpoint_snapshots(
        &self,
        player_id: PlayerId,
        database: &Database,
    ) -> Vec<ShieldpointSnapshot> {
        let Some(player) = self.get_player(player_id) else {
            return Vec::new();
        };
        self.units
            .iter()
            .filter(|(_, unit)| unit.base.player_id == player_id)
            .filter_map(|(unit_id, unit)| {
                if !unit.shields.is_enabled() {
                    return None;
                }
                let proto = database
                    .objects
                    .iter()
                    .find(|proto| proto.name.eq_ignore_ascii_case(&unit.proto_object_name))?;
                let base = proto
                    .shieldpoints
                    .filter(|shieldpoints| shieldpoints.is_finite() && *shieldpoints >= 0.0)
                    .unwrap_or_default();
                Some(ShieldpointSnapshot {
                    unit_id,
                    base,
                    previous: player
                        .technologies
                        .shieldpoints(&unit.proto_object_name, base),
                })
            })
            .collect()
    }

    fn apply_shieldpoint_changes(
        &mut self,
        player_id: PlayerId,
        snapshots: &[ShieldpointSnapshot],
    ) {
        for snapshot in snapshots {
            let Some(unit) = self.units.get(snapshot.unit_id) else {
                continue;
            };
            let next = self
                .get_player(player_id)
                .expect("validated player")
                .technologies
                .shieldpoints(&unit.proto_object_name, snapshot.base);
            let tolerance = f32::EPSILON * next.abs().max(snapshot.previous.abs()).max(1.0);
            if (next - snapshot.previous).abs() <= tolerance {
                continue;
            }
            let increased = self
                .units
                .get_mut(snapshot.unit_id)
                .is_some_and(|unit| unit.shields.set_maximum(next));
            if increased {
                self.request_unit_shield_recharge(snapshot.unit_id);
            }
        }
    }

    fn refresh_shield_regen_scalars(&mut self, player_id: PlayerId) {
        let unit_ids = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| (unit.base.player_id == player_id).then_some(unit_id))
            .collect::<Vec<_>>();
        for unit_id in unit_ids {
            let Some(proto_object_name) = self
                .units
                .get(unit_id)
                .map(|unit| unit.proto_object_name.clone())
            else {
                continue;
            };
            let Some(player) = self.get_player(player_id) else {
                continue;
            };
            let rate = player
                .technologies
                .unit_shield_regen_rate(&proto_object_name);
            let delay = player
                .technologies
                .unit_shield_regen_delay(&proto_object_name);
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.shields.set_regen_scalars(rate, delay);
            }
        }
    }

    fn apply_squad_transform(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        transform: &AppliedSquadTransform,
    ) {
        if transform
            .previous_definition
            .eq_ignore_ascii_case(&transform.new_definition)
        {
            return;
        }
        let Some(new_proto) = find_squad(database, &transform.new_definition) else {
            return;
        };
        let old_proto = find_squad(database, &transform.previous_definition);
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| {
                (squad.base.player_id == player_id
                    && squad.proto_squad_name.eq_ignore_ascii_case(&transform.from))
                .then_some(id)
            })
            .collect::<Vec<_>>();

        for squad_id in squad_ids {
            add_missing_transform_members(self, database, squad_id, old_proto, new_proto);
            refresh_squad_member_settings(self, squad_id);
        }
    }
}

fn add_missing_transform_members(
    world: &mut World,
    database: &Database,
    squad_id: EntityId,
    old_proto: Option<&ProtoSquad>,
    new_proto: &ProtoSquad,
) {
    let Some(new_units) = new_proto.units.as_ref() else {
        return;
    };
    for entry in &new_units.entries {
        let old_count = old_proto.map_or(0, |proto| proto_unit_count(proto, &entry.proto_object));
        for _ in old_count..entry.count.max(0) {
            let _unit_id = add_squad_member_from_prototype(
                world,
                squad_id,
                entry.proto_object.trim(),
                database,
            );
        }
    }
}

fn proto_unit_count(proto: &ProtoSquad, proto_object: &str) -> i32 {
    proto.units.as_ref().map_or(0, |units| {
        units
            .entries
            .iter()
            .filter(|entry| entry.proto_object.eq_ignore_ascii_case(proto_object))
            .map(|entry| entry.count.max(0))
            .sum()
    })
}

fn find_technology<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database pipeline::database::hw1::Tech> {
    database
        .techs
        .iter()
        .find(|technology| technology.name.eq_ignore_ascii_case(name.trim()))
}

fn find_squad<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoSquad> {
    database
        .squads
        .iter()
        .find(|squad| squad.name.eq_ignore_ascii_case(name.trim()))
}
