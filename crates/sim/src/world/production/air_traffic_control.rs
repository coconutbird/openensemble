//! Persistent air-base landing-pad initialization and reservation lifecycle.

use super::World;
use crate::entities::{AirTrafficControl, AirTrafficLandingSpot};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AirTrafficControlActionProfile, GameplayCatalog};
use crate::player::PlayerId;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::TrainLimitType;
use std::collections::BTreeSet;

struct AirTrafficControlContext {
    action_name: String,
    position: glam::Vec3,
    forward: glam::Vec3,
    unsc_layout: bool,
}

impl World {
    pub(in crate::world) fn update_air_traffic_controls(
        &mut self,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let live_aircraft = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (unit.is_alive() && unit.uses_move_air()).then_some(unit_id)
            })
            .collect::<BTreeSet<_>>();
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            let context = self.air_traffic_control_context(unit_id, database, gameplay);
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            let Some(context) = context else {
                unit.production.air_traffic_control = None;
                continue;
            };
            let replace = unit
                .production
                .air_traffic_control
                .as_ref()
                .is_none_or(|control| {
                    !control
                        .action_name()
                        .eq_ignore_ascii_case(&context.action_name)
                });
            if replace {
                unit.production.air_traffic_control = Some(AirTrafficControl::new(
                    &context.action_name,
                    context.position,
                    context.forward,
                    context.unsc_layout,
                ));
            }
            if let Some(control) = &mut unit.production.air_traffic_control {
                control.retain_live_aircraft(|aircraft_id| live_aircraft.contains(&aircraft_id));
            }
        }
        self.initialize_move_air_bases(database);
    }

    fn air_traffic_control_context(
        &self,
        unit_id: EntityId,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> Option<AirTrafficControlContext> {
        let unit = self
            .units
            .get(unit_id)
            .filter(|unit| unit.is_operational())?;
        let profile = gameplay
            .air_traffic_control_actions(&unit.proto_object_name)
            .iter()
            .find(|profile| self.air_traffic_control_action_enabled(unit_id, profile))?;
        Some(AirTrafficControlContext {
            action_name: profile.action_name().to_owned(),
            position: unit.base.position,
            forward: unit.base.forward,
            unsc_layout: player_uses_unsc_layout(self, database, unit.base.player_id),
        })
    }

    fn air_traffic_control_action_enabled(
        &self,
        unit_id: EntityId,
        profile: &AirTrafficControlActionProfile,
    ) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let authored_enabled = !profile.starts_disabled();
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    )
                });
        unit.actions
            .is_enabled(profile.action_name(), !player_enabled)
    }

    pub(super) fn request_air_traffic_landing_spot(
        &mut self,
        controller_id: EntityId,
        aircraft_id: EntityId,
    ) -> Option<AirTrafficLandingSpot> {
        self.units
            .get_mut(controller_id)?
            .production
            .air_traffic_control
            .as_mut()?
            .request_landing_spot(aircraft_id)
    }

    pub(in crate::world) fn release_air_traffic_assignment(&mut self, aircraft_id: EntityId) {
        for (_, unit) in self.units.iter_mut() {
            if let Some(control) = &mut unit.production.air_traffic_control {
                let _released = control.release_aircraft(aircraft_id);
            }
        }
    }

    fn initialize_move_air_bases(&mut self, database: &Database) {
        let aircraft = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                let state = unit.move_air_state()?;
                (unit.is_alive() && !state.lifecycle.initialized() && unit.squad_id.is_some())
                    .then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for aircraft_id in aircraft {
            self.initialize_move_air_base(aircraft_id, database);
        }
    }

    fn initialize_move_air_base(&mut self, aircraft_id: EntityId, database: &Database) {
        let Some((position, forward, air_base_id)) =
            self.move_air_base_context(aircraft_id, database)
        else {
            return;
        };
        let air_base_position = air_base_id
            .and_then(|id| self.units.get(id))
            .map(|air_base| air_base.base.position);
        self.release_air_traffic_assignment(aircraft_id);
        let landing_spot =
            air_base_id.and_then(|id| self.request_air_traffic_landing_spot(id, aircraft_id));
        let Some(unit) = self.units.get_mut(aircraft_id) else {
            return;
        };
        let Some(mut state) = unit.move_air_state() else {
            return;
        };
        state.lifecycle.set_initialized(true);
        state.air_base = air_base_id.filter(|_| air_base_position.is_some());
        state.base_position = air_base_position.unwrap_or(position);
        state.goal_position = state.base_position;
        state.goal_position_valid = true;
        state.spot_forward = glam::Vec3::X;
        if let Some(spot) = landing_spot {
            state.pad_position = spot.position();
            state.lifecycle.set_pad_position_valid(true);
            state.spot_forward = spot.forward();
        } else if state.air_base.is_none() {
            state.pad_position = position;
            state.lifecycle.set_pad_position_valid(true);
            state.spot_forward = forward;
            state.lifecycle.set_launch_requested(true);
        }
        unit.set_move_air_state(state);
    }

    fn move_air_base_context(
        &self,
        aircraft_id: EntityId,
        database: &Database,
    ) -> Option<(glam::Vec3, glam::Vec3, Option<EntityId>)> {
        let unit = self.units.get(aircraft_id)?;
        let squad = self.squads.get(unit.squad_id?)?;
        let air_base_id = squad.trained_by.filter(|&trainer_id| {
            self.units.get(trainer_id).is_some_and(|trainer| {
                trainer_has_squad_train_limit(
                    database,
                    &trainer.proto_object_name,
                    &squad.proto_squad_name,
                )
            })
        });
        Some((unit.base.position, unit.base.forward, air_base_id))
    }
}

fn trainer_has_squad_train_limit(
    database: &Database,
    trainer_name: &str,
    squad_name: &str,
) -> bool {
    let Some(trainer) = database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(trainer_name))
    else {
        return false;
    };
    trainer.train_limits.iter().any(|limit| {
        limit.target.trim().eq_ignore_ascii_case(squad_name)
            && train_limit_targets_squad(limit.limit_type)
    })
}

fn train_limit_targets_squad(limit_type: Option<TrainLimitType>) -> bool {
    match limit_type {
        Some(TrainLimitType::Squad) | None => true,
        Some(TrainLimitType::Unit) => false,
    }
}

fn player_uses_unsc_layout(world: &World, database: &Database, player_id: PlayerId) -> bool {
    world
        .get_player(player_id)
        .and_then(|player| usize::try_from(player.civ_id).ok())
        .and_then(|index| database.civs.get(index))
        .is_some_and(|civilization| civilization.name.eq_ignore_ascii_case("UNSC"))
}

#[cfg(test)]
mod tests;
