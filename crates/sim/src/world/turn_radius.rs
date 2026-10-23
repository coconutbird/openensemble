#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the squad update of the plumbing branch calls the turn-radius update"
    )
)]
//! The turn-radius update of the physics vehicle squads (`BSquad::update` calls
//! `updateTurning` after `updateOrders`).

use super::World;
use crate::entities::UnitState;
use crate::entities::squads::{TurnRadiusLeader, TurnRadiusProgress};
use crate::entity_id::EntityId;
use crate::physics::vehicle_actions::ActionKind;

impl World {
    /// Moves the turn-radius point of a squad for one sim update of `elapsed` seconds.
    ///
    /// The point exists for a physics vehicle squad that is not an aircraft
    /// (`isSquadAPhysicsVehicle`: exactly one child with a vehicle action). Retail starts it in
    /// `BSquad::init`; the first call starts it here. When the point has caught up with a squad
    /// that completed its movement, a ground-family vehicle that may sleep ends its move
    /// (retail hack in `updateTurning`).
    pub(crate) fn update_squad_turn_radius(&mut self, squad_id: EntityId, elapsed: f32) {
        let Some(squad) = self.squads.get(squad_id) else {
            return;
        };
        let leader_id = squad.unit_ids.first().copied();
        let leader = leader_id.and_then(|id| self.units.get(id));
        let leader_action = leader
            .and_then(|unit| unit.physics_object.as_ref())
            .and_then(|state| {
                state
                    .action
                    .as_ref()
                    .map(|action| (state.body, action.kind()))
            });
        let is_physics_vehicle = squad.unit_ids.len() == 1 && leader_action.is_some();
        let is_ground_family = matches!(leader_action, Some((_, ActionKind::Ground(_))));
        let is_aircraft = matches!(leader_action, Some((_, ActionKind::Hover(_))));
        let deactivation_enabled = leader_action.is_some_and(|(body, _)| {
            self.physics.world().is_deactivation_enabled(body) == Some(true)
        });
        let leader_input = leader.map(|unit| TurnRadiusLeader {
            turn_rate_degrees: unit.turn_rate_degrees,
            desired_velocity: unit.desired_velocity * unit.velocity_scalar,
        });
        let leader_moving = leader.is_some_and(|unit| unit.state == UnitState::Moving);
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return;
        };
        if squad.turn_radius().is_none() {
            if !is_physics_vehicle || is_aircraft {
                return;
            }
            squad.start_turn_radius();
        }
        let completed = squad.move_target.is_none();
        let progress = squad.update_turning(elapsed, leader_input, completed);
        if progress == TurnRadiusProgress::Done
            && is_physics_vehicle
            && is_ground_family
            && leader_moving
            && deactivation_enabled
            && let Some(unit) = leader_id.and_then(|id| self.units.get_mut(id))
        {
            unit.stop();
        }
    }
}

#[cfg(test)]
mod tests;
