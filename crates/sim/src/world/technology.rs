//! Database-backed player technology activation and live entity updates.

use super::World;
use crate::entity_id::EntityId;
use crate::player::{
    AppliedPrototypeTransform, AppliedSquadTransform, AppliedUnitTransform, PlayerId,
};
use crate::scenario::{
    add_squad_member_from_prototype, configure_unit_from_player_proto,
    refresh_squad_member_settings,
};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad, Tech};

/// Failure to change one player's technology state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TechnologyError {
    /// The target player is not present in the world.
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    /// The layered database has no technology with this name.
    #[error("technology '{0}' was not found")]
    TechnologyNotFound(String),
    /// The live unit selected for a per-instance technology is absent.
    #[error("unit {0:?} is not present in the world")]
    UnitNotFound(EntityId),
    /// The selected live unit does not belong to the researching player.
    #[error("unit {unit_id:?} is not owned by player {player_id}")]
    UnitNotOwned {
        unit_id: EntityId,
        player_id: PlayerId,
    },
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
        let ammunition = self.ammunition_snapshots(player_id, database);
        let Some(player) = self.get_player_mut(player_id) else {
            return Err(TechnologyError::PlayerNotFound(player_id));
        };
        let transforms = player.technologies.activate(database, technology);
        self.apply_hitpoint_changes(player_id, &hitpoints);
        self.apply_shieldpoint_changes(player_id, &shieldpoints);
        self.refresh_shield_regen_scalars(player_id);
        self.reconcile_player_ammunition(player_id, &ammunition, database);
        for transform in transforms {
            match transform {
                AppliedPrototypeTransform::Unit(transform) => {
                    self.apply_unit_transform(player_id, database, &transform);
                }
                AppliedPrototypeTransform::Squad(transform) => {
                    self.apply_squad_transform(player_id, database, &transform);
                }
            }
        }
        self.activate_dependent_shadow_technologies(player_id, database, &technology.name);
        Ok(true)
    }

    pub(crate) fn activate_unique_technology(
        &mut self,
        player_id: PlayerId,
        unit_id: EntityId,
        database: &Database,
        technology_id: i32,
        technology: &Tech,
    ) -> Result<bool, TechnologyError> {
        if self.get_player(player_id).is_none() {
            return Err(TechnologyError::PlayerNotFound(player_id));
        }
        let Some(unit) = self.get_unit(unit_id) else {
            return Err(TechnologyError::UnitNotFound(unit_id));
        };
        if unit.base.player_id != player_id {
            return Err(TechnologyError::UnitNotOwned { unit_id, player_id });
        }
        if unit.unique_technology_is_active(technology_id) {
            return Ok(false);
        }

        let transform_targets = technology
            .effects
            .iter()
            .flat_map(|effects| &effects.entries)
            .filter(|effect| {
                effect
                    .effect_type
                    .trim()
                    .eq_ignore_ascii_case("TransformUnit")
            })
            .filter_map(|effect| nonempty(effect.value.as_deref()))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.get_unit_mut(unit_id)
            .expect("validated unique-technology unit exists")
            .activate_unique_technology(technology_id);
        for target in transform_targets {
            self.transform_unit_instance(player_id, unit_id, database, &target);
        }
        Ok(true)
    }

    fn transform_unit_instance(
        &mut self,
        player_id: PlayerId,
        unit_id: EntityId,
        database: &Database,
        logical_target: &str,
    ) {
        let Some((logical_index, logical_proto)) = find_object(database, logical_target) else {
            return;
        };
        let Some(effective_target) = self.get_player(player_id).map(|player| {
            player
                .technologies
                .resolved_unit_prototype(logical_target)
                .to_owned()
        }) else {
            return;
        };
        let Some((_, effective_proto)) = find_object(database, &effective_target) else {
            return;
        };
        let Some(snapshot) = UnitTransformSnapshot::capture(self, unit_id) else {
            return;
        };
        let squad_id = snapshot.squad_id;
        if snapshot.built {
            self.deactivate_unit_built_economy(unit_id);
        }
        configure_unit_from_player_proto(
            self,
            unit_id,
            logical_target,
            database_id(logical_proto, logical_index),
            &effective_target,
            effective_proto,
        );
        snapshot.restore(self, unit_id);
        if snapshot.built {
            self.activate_unit_on_built(unit_id, database, effective_proto);
        }
        if let Some(squad_id) = squad_id {
            self.refresh_squad_ammunition(squad_id, database);
            refresh_squad_member_settings(self, squad_id);
        }
    }

    pub(crate) fn initialize_shadow_technologies(
        &mut self,
        player_id: PlayerId,
        database: &Database,
    ) {
        let candidates = database
            .techs
            .iter()
            .map(|technology| technology.name.clone())
            .collect::<Vec<_>>();
        for candidate in candidates {
            let Some(technology) = find_technology(database, &candidate) else {
                continue;
            };
            if self.shadow_technology_is_eligible(player_id, database, technology) {
                let _activation = self.activate_technology(player_id, database, &candidate);
            }
        }
    }

    fn activate_dependent_shadow_technologies(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        activated_name: &str,
    ) {
        let candidates = database
            .techs
            .iter()
            .filter(|technology| technology_depends_on(technology, activated_name))
            .map(|technology| technology.name.clone())
            .collect::<Vec<_>>();
        for candidate in candidates {
            let Some(technology) = find_technology(database, &candidate) else {
                continue;
            };
            if self.shadow_technology_is_eligible(player_id, database, technology) {
                let _activation = self.activate_technology(player_id, database, &candidate);
            }
        }
    }

    fn shadow_technology_is_eligible(
        &self,
        player_id: PlayerId,
        database: &Database,
        technology: &Tech,
    ) -> bool {
        let Some(player) = self.get_player(player_id) else {
            return false;
        };
        !technology.name.trim().is_empty()
            && super::research::has_flag(technology, "Shadow")
            && !super::research::has_flag(technology, "UniqueProtoUnitInstance")
            && !super::research::has_flag(technology, "Forbid")
            && !super::research::authored_unobtainable(technology)
            && technology_alpha_is_enabled(self, technology)
            && !player.technologies.is_active(&technology.name)
            && super::research::prerequisites_met(self, player, database, technology)
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
        let ammunition = self.ammunition_snapshots(player_id, database);
        let Some(player) = self.get_player_mut(player_id) else {
            return Err(TechnologyError::PlayerNotFound(player_id));
        };
        let changed = player.technologies.deactivate(database, &technology.name);
        if changed {
            self.apply_hitpoint_changes(player_id, &hitpoints);
            self.apply_shieldpoint_changes(player_id, &shieldpoints);
            self.refresh_shield_regen_scalars(player_id);
            self.reconcile_player_ammunition(player_id, &ammunition, database);
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
                    previous: player
                        .technologies
                        .hitpoints(unit.logical_proto_object_name(), base),
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
                .hitpoints(unit.logical_proto_object_name(), snapshot.base);
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
                        .shieldpoints(unit.logical_proto_object_name(), base),
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
                .shieldpoints(unit.logical_proto_object_name(), snapshot.base);
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
            let Some(logical_name) = self
                .units
                .get(unit_id)
                .map(|unit| unit.logical_proto_object_name().to_owned())
            else {
                continue;
            };
            let Some(player) = self.get_player(player_id) else {
                continue;
            };
            let rate = player.technologies.unit_shield_regen_rate(&logical_name);
            let delay = player.technologies.unit_shield_regen_delay(&logical_name);
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

    fn apply_unit_transform(
        &mut self,
        player_id: PlayerId,
        database: &Database,
        transform: &AppliedUnitTransform,
    ) {
        if transform
            .previous_definition
            .eq_ignore_ascii_case(&transform.new_definition)
        {
            return;
        }
        let Some((logical_index, logical_proto)) = find_object(database, &transform.from) else {
            return;
        };
        let Some((_, new_proto)) = find_object(database, &transform.new_definition) else {
            return;
        };
        let logical_id = database_id(logical_proto, logical_index);
        let unit_ids = transformed_unit_ids(self, player_id, transform);
        let mut squads = Vec::new();
        for unit_id in unit_ids {
            let Some(snapshot) = UnitTransformSnapshot::capture(self, unit_id) else {
                continue;
            };
            if snapshot.built {
                self.deactivate_unit_built_economy(unit_id);
            }
            configure_unit_from_player_proto(
                self,
                unit_id,
                &transform.from,
                logical_id,
                &transform.new_definition,
                new_proto,
            );
            snapshot.restore(self, unit_id);
            if snapshot.built {
                self.activate_unit_on_built(unit_id, database, new_proto);
            }
            if let Some(squad_id) = snapshot.squad_id
                && !squads.contains(&squad_id)
            {
                squads.push(squad_id);
            }
        }
        for squad_id in squads {
            self.refresh_squad_ammunition(squad_id, database);
            refresh_squad_member_settings(self, squad_id);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct UnitTransformSnapshot {
    hitpoint_ratio: f32,
    ammunition_ratio: f32,
    shieldpoints: f32,
    squad_id: Option<EntityId>,
    built: bool,
}

impl UnitTransformSnapshot {
    fn capture(world: &World, unit_id: EntityId) -> Option<Self> {
        let unit = world.get_unit(unit_id)?;
        Some(Self {
            hitpoint_ratio: ratio_or_one(unit.hitpoints, unit.max_hitpoints),
            ammunition_ratio: ratio_or_one(unit.ammunition.current(), unit.ammunition.maximum()),
            shieldpoints: unit.shields.current,
            squad_id: unit.squad_id,
            built: unit.built,
        })
    }

    fn restore(self, world: &mut World, unit_id: EntityId) {
        let Some(unit) = world.get_unit_mut(unit_id) else {
            return;
        };
        unit.hitpoints = (unit.max_hitpoints * self.hitpoint_ratio).clamp(0.0, unit.max_hitpoints);
        unit.ammunition
            .set_current(unit.ammunition.maximum() * self.ammunition_ratio);
        unit.shields.set_current(self.shieldpoints);
    }
}

fn transformed_unit_ids(
    world: &World,
    player_id: PlayerId,
    transform: &AppliedUnitTransform,
) -> Vec<EntityId> {
    world
        .units
        .iter()
        .filter_map(|(id, unit)| {
            (unit.base.player_id == player_id
                && unit
                    .logical_proto_object_name()
                    .eq_ignore_ascii_case(&transform.from)
                && unit
                    .proto_object_name
                    .eq_ignore_ascii_case(&transform.previous_definition))
            .then_some(id)
        })
        .collect()
}

fn ratio_or_one(current: f32, maximum: f32) -> f32 {
    if maximum.is_finite() && maximum.abs() >= f32::EPSILON {
        current / maximum
    } else {
        1.0
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

fn technology_depends_on(technology: &Tech, prerequisite: &str) -> bool {
    technology
        .prereqs
        .iter()
        .chain(technology.or_prereqs.iter())
        .flat_map(|prerequisites| &prerequisites.entries)
        .any(|entry| {
            entry
                .text
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(&entry.tech)
                .trim()
                .eq_ignore_ascii_case(prerequisite.trim())
        })
}

fn technology_alpha_is_enabled(world: &World, technology: &Tech) -> bool {
    match technology.alpha {
        Some(0) => !world.is_config_defined("Alpha"),
        Some(1) => world.is_config_defined("Alpha"),
        _ => true,
    }
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

fn find_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoObject)> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, object)| object.name.eq_ignore_ascii_case(name.trim()))
}

fn database_id(prototype: &ProtoObject, index: usize) -> i32 {
    prototype
        .dbid
        .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
