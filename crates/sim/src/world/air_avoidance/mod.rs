//! Retail persistent aircraft collision avoidance and crash orchestration.

mod crash;
mod hover;
mod steering;

use super::World;
use crate::entities::{AircraftCrashPhase, Unit};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AirAvoidanceActionProfile, GameplayCatalog};
use std::collections::BTreeMap;

const BIRTH_SPEED_LIMIT: f32 = 8.0;

impl World {
    pub(in crate::world) fn configure_air_avoidance_for_damage(
        &mut self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) {
        let profile = self
            .enabled_air_avoidance_profile(unit_id, gameplay)
            .cloned();
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.air_avoidance
                .reconcile(profile.as_ref(), unit.base.position);
        }
    }

    pub(super) fn prepare_air_avoidance(
        &mut self,
        dt: f32,
        gameplay: &GameplayCatalog,
    ) -> BTreeMap<EntityId, f32> {
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        let mut speed_limits = BTreeMap::new();
        for unit_id in unit_ids {
            let profile = self
                .enabled_air_avoidance_profile(unit_id, gameplay)
                .cloned();
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            unit.air_avoidance
                .reconcile(profile.as_ref(), unit.base.position);
            if profile.is_some() && unit.flying && unit.is_alive() && !unit.is_garrisoned() {
                unit.air_avoidance.advance_birth(dt);
            }
            if unit.is_air_speed_limited()
                && let Some(squad_id) = unit.squad_id
            {
                speed_limits
                    .entry(squad_id)
                    .and_modify(|limit: &mut f32| *limit = limit.min(BIRTH_SPEED_LIMIT))
                    .or_insert(BIRTH_SPEED_LIMIT);
            }
        }
        self.prepare_aircraft_crashes(gameplay);
        speed_limits
    }

    pub(super) fn advance_air_avoidance(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.update_aircraft_crashes(dt, gameplay);
        self.update_aircraft_steering(dt, gameplay);
        self.update_aircraft_hover(dt, gameplay);
    }

    pub(super) fn finalize_air_avoidance_positions(&mut self) {
        for (_, unit) in self.units.iter_mut() {
            if unit.air_avoidance.action_name.is_some() || unit.is_crashing() {
                unit.air_avoidance.previous_position = unit.base.position;
            }
        }
    }

    pub(in crate::world) fn record_aircraft_crash_killer(
        &mut self,
        target_id: EntityId,
        killer_id: Option<EntityId>,
        killer_player: Option<crate::player::PlayerId>,
    ) {
        let killer_player = killer_player.or_else(|| {
            killer_id
                .and_then(|id| self.units.get(id))
                .map(|unit| unit.base.player_id)
        });
        let killer_team = killer_player
            .and_then(|player_id| self.get_player(player_id))
            .map(|player| player.team_id);
        if let Some(target) = self.units.get_mut(target_id) {
            target
                .air_avoidance
                .set_killer(killer_id, killer_player, killer_team);
        }
    }

    pub(crate) fn enabled_air_avoidance_profile<'a>(
        &self,
        unit_id: EntityId,
        gameplay: &'a GameplayCatalog,
    ) -> Option<&'a AirAvoidanceActionProfile> {
        let unit = self.units.get(unit_id).filter(|unit| unit.is_alive())?;
        gameplay
            .air_avoidance_actions(&unit.proto_object_name)
            .iter()
            .find(|profile| self.air_avoidance_action_enabled(unit, profile))
    }

    pub(crate) fn current_air_avoidance_profile<'a>(
        unit: &Unit,
        gameplay: &'a GameplayCatalog,
    ) -> Option<&'a AirAvoidanceActionProfile> {
        let action_name = unit.air_avoidance.action_name.as_deref()?;
        gameplay
            .air_avoidance_actions(&unit.proto_object_name)
            .iter()
            .find(|profile| profile.action_name().eq_ignore_ascii_case(action_name))
    }

    fn air_avoidance_action_enabled(
        &self,
        unit: &Unit,
        profile: &AirAvoidanceActionProfile,
    ) -> bool {
        let authored_enabled = !profile.starts_disabled();
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        unit.logical_proto_object_name(),
                        profile.action_name(),
                        authored_enabled,
                    )
                });
        unit.actions
            .is_enabled(profile.action_name(), !player_enabled)
    }
}

pub(super) fn is_active_flight(unit: &Unit) -> bool {
    unit.flying
        && unit.is_alive()
        && !unit.is_garrisoned()
        && unit.aircraft_crash_phase() == AircraftCrashPhase::Inactive
}

#[cfg(test)]
mod tests;
