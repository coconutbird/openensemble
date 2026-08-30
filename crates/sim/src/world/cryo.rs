//! Authoritative application and ticking of retail squad cryo actions.

use super::World;
use crate::entities::squads::{SquadCryoConfig, SquadCryoEffect};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use pipeline::database::hw1::{Database, GameData, Squad as ProtoSquad};

impl World {
    /// Apply cryo depletion to a squad using the active layered database.
    ///
    /// The first accepted application snapshots the proto squad's resistance
    /// and global thaw settings, matching creation of retail's `SquadCryo`
    /// action. Additional applications mutate that same persistent action.
    pub fn add_squad_cryo(&mut self, squad_id: EntityId, amount: f32, database: &Database) -> bool {
        self.add_squad_cryo_with_thaw_times(squad_id, amount, database, None)
    }

    pub(super) fn add_squad_cryo_with_thaw_times(
        &mut self,
        squad_id: EntityId,
        amount: f32,
        database: &Database,
        thaw_times: Option<(f32, f32)>,
    ) -> bool {
        let Some(mut config) = self.squad_cryo_config(squad_id, database) else {
            return false;
        };
        if let Some((freezing, frozen)) = thaw_times {
            if freezing.is_finite() && freezing >= 0.0 {
                config.freezing_thaw_time = config.freezing_thaw_time.max(freezing);
            }
            if frozen.is_finite() && frozen >= 0.0 {
                config.frozen_thaw_time = config.frozen_thaw_time.max(frozen);
            }
        }
        let Some((accepted, effect_changed)) = self.squads.get_mut(squad_id).map(|squad| {
            let previous = squad.cryo.effect();
            let accepted = squad.cryo.add(amount, config);
            (accepted, previous != squad.cryo.effect())
        }) else {
            return false;
        };
        if effect_changed {
            self.sync_squad_cryo_effect(squad_id);
        }
        accepted
    }

    pub(super) fn squad_cryo_maximum(
        &self,
        squad_id: EntityId,
        database: &Database,
    ) -> Option<f32> {
        self.squad_cryo_config(squad_id, database)
            .map(|config| config.maximum_points)
    }

    pub(super) fn effective_squad_prototype<'database>(
        &self,
        squad_id: EntityId,
        database: &'database Database,
    ) -> Option<&'database ProtoSquad> {
        let squad = self.squads.get(squad_id)?;
        let logical_name = squad.proto_squad_name.trim();
        let resolved_name = self
            .get_player(squad.base.player_id)
            .map_or(logical_name, |player| {
                player.technologies.resolved_squad_prototype(logical_name)
            });
        find_proto_squad(database, resolved_name)
    }

    pub(super) fn force_cryo_frozen_kill(&mut self, squad_id: EntityId) -> bool {
        let Some(member_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return false;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.kill();
        }
        for unit_id in member_ids {
            self.remove_owned_attachments(unit_id);
            let base_id = self.units.get(unit_id).and_then(|unit| unit.base_id);
            if let Some(unit) = self.units.get_mut(unit_id) {
                // Retail's cryo kill bypasses normal HeroDeath incapacitation.
                unit.kill();
            }
            if let Some(base_id) = base_id {
                self.recompute_base_child_damage(base_id);
            }
        }
        true
    }

    pub(super) fn update_cryo(&mut self, dt: f32) {
        let changed = self
            .squads
            .iter_mut()
            .filter_map(|(squad_id, squad)| squad.cryo.advance(dt).then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in changed {
            self.sync_squad_cryo_effect(squad_id);
        }
    }

    pub(super) fn apply_squad_cryo_to_unit(&mut self, squad_id: EntityId, unit_id: EntityId) {
        let effect = self
            .squads
            .get(squad_id)
            .map_or_else(SquadCryoEffect::default, |squad| squad.cryo.effect());
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.apply_cryo_effect(effect);
        }
    }

    fn sync_squad_cryo_effect(&mut self, squad_id: EntityId) {
        let Some((effect, unit_ids)) = self
            .squads
            .get(squad_id)
            .map(|squad| (squad.cryo.effect(), squad.unit_ids.clone()))
        else {
            return;
        };
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id)
                && (unit.is_alive() || unit.is_static_death_replacement())
            {
                unit.apply_cryo_effect(effect);
            }
        }
    }

    fn squad_cryo_config(
        &self,
        squad_id: EntityId,
        database: &Database,
    ) -> Option<SquadCryoConfig> {
        let proto = self.effective_squad_prototype(squad_id, database);
        cryo_config(database.game_data.as_ref()?, proto)
    }
}

fn cryo_config(game_data: &GameData, proto: Option<&ProtoSquad>) -> Option<SquadCryoConfig> {
    let default_points = finite_nonnegative(game_data.default_cryo_points).unwrap_or_default();
    let maximum_points = proto
        .and_then(|squad| finite_nonnegative(squad.cryo_points))
        .unwrap_or(default_points);
    (maximum_points > 0.0).then_some(SquadCryoConfig {
        maximum_points,
        thaw_speed: finite_nonnegative(game_data.default_thaw_speed).unwrap_or_default(),
        freezing_thaw_time: finite_nonnegative(game_data.time_freezing_to_thaw).unwrap_or_default(),
        frozen_thaw_time: finite_nonnegative(game_data.time_frozen_to_thaw).unwrap_or_default(),
        freezing_speed_modifier: finite_nonnegative(game_data.freezing_speed_modifier)
            .unwrap_or(1.0),
        freezing_damage_modifier: finite_nonnegative(game_data.freezing_damage_modifier)
            .unwrap_or(1.0),
        frozen_damage_modifier: finite_nonnegative(game_data.frozen_damage_modifier).unwrap_or(1.0),
    })
}

fn find_proto_squad<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoSquad> {
    database
        .squads
        .iter()
        .find(|squad| squad.name.eq_ignore_ascii_case(name))
}

fn finite_nonnegative(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(test)]
mod tests;
