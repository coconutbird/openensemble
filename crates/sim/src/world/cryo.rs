//! Authoritative application and ticking of retail squad cryo actions.

use super::World;
use super::powers::physics_helper::{AirImpulse, impulse_physics_air_unit};
use crate::entities::squads::{SquadCryoConfig, SquadCryoEffect, SquadCryoState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use num_traits::ToPrimitive;
use pipeline::database::hw1::{Database, GameData, Squad as ProtoSquad};

/// The chance in percent per second of the cryo aircraft impulse while freezing or thawing.
const AIR_IMPULSE_PERCENT: f32 = 100.0;
/// The chance in percent per second of the cryo aircraft impulse while frozen.
const FROZEN_AIR_IMPULSE_PERCENT: f32 = 150.0;
/// `dword_1411F3D10`: the impulse scale of the cryo aircraft impulse.
const CRYO_AIR_IMPULSE_SCALE: f32 = 0.5;

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

    /// Runs the `SquadCryo` action of each squad: the aircraft impulses, then the cryo state.
    pub(super) fn update_cryo(&mut self, dt: f32, gameplay: Option<&GameplayCatalog>) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                (squad.cryo_state() != SquadCryoState::None).then_some(squad_id)
            })
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.impulse_cryo_air_units(squad_id, dt, gameplay);
        }
        let changed = self
            .squads
            .iter_mut()
            .filter_map(|(squad_id, squad)| squad.cryo.advance(dt).then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in changed {
            self.sync_squad_cryo_effect(squad_id);
        }
    }

    /// `BSquadActionCryo_impulseAirUnits` (`0x1403B9900`, 2008 `squadactioncryo.cpp:574-591`):
    /// with a chance of `pct · elapsed` percent (`pct` 150 when frozen, else 100, truncated),
    /// each unit gets `impulsePhysicsAirUnit(unit, 0.5)` toward a random point below it.
    fn impulse_cryo_air_units(
        &mut self,
        squad_id: EntityId,
        dt: f32,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(squad) = self.squads.get(squad_id) else {
            return;
        };
        let percent = if squad.cryo_state() == SquadCryoState::Frozen {
            FROZEN_AIR_IMPULSE_PERCENT
        } else {
            AIR_IMPULSE_PERCENT
        };
        let unit_ids = squad.unit_ids.clone();
        // `cvttss2si`: the chance truncates toward zero.
        let chance = (percent * dt).to_i32().unwrap_or(i32::MAX);
        let roll = i32::try_from(self.trigger_random_index(100)).unwrap_or(i32::MAX);
        if roll > chance {
            return;
        }
        for unit_id in unit_ids {
            let _impulsed = impulse_physics_air_unit(
                self,
                gameplay,
                unit_id,
                AirImpulse {
                    scale: CRYO_AIR_IMPULSE_SCALE,
                    target: None,
                    radius: 0.0,
                },
            );
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
