//! Authoritative trigger-created carrier flight and passenger release.

use super::World;
use crate::entities::squads::{SquadTransportFlyIn, SquadTransportPlan, TransportFlyInPhase};
use crate::entities::{SquadState, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use glam::Vec3;

mod power;

impl World {
    /// Attach a newly created squad to a synthetic transport and start flight.
    pub(crate) fn start_transport_fly_in(
        &mut self,
        transport_squad_id: EntityId,
        plan: SquadTransportPlan,
    ) -> bool {
        if transport_squad_id == plan.passenger_squad_id
            || !transport_plan_is_finite(plan)
            || self
                .squads
                .get(transport_squad_id)
                .is_none_or(|squad| !squad.is_alive() || squad.transport_fly_in.is_some())
        {
            return false;
        }
        let Some((passenger_player_id, passenger_units)) = self
            .squads
            .get(plan.passenger_squad_id)
            .filter(|squad| squad.is_alive() && !squad.garrison.is_garrisoned())
            .map(|squad| (squad.base.player_id, squad.unit_ids.clone()))
        else {
            return false;
        };
        let Some((transport_player_id, container_unit_id, speed)) = self
            .squads
            .get(transport_squad_id)
            .and_then(|squad| Some((squad.base.player_id, *squad.unit_ids.first()?, squad.speed)))
        else {
            return false;
        };
        if transport_player_id != passenger_player_id || passenger_units.is_empty() {
            return false;
        }

        let flight_forward = flight_forward(plan.start_position, plan.incoming_target);
        if let Some(transport) = self.squads.get_mut(transport_squad_id) {
            transport.base.position = plan.start_position;
            transport.base.forward = flight_forward;
            transport.base.velocity = Vec3::ZERO;
            transport.state = SquadState::Moving;
            transport
                .garrison
                .add_contained_squad(plan.passenger_squad_id);
            transport.transport_fly_in = Some(SquadTransportFlyIn::new(plan, speed));
        }
        for unit_id in &passenger_units {
            if let Some(unit) = self.units.get_mut(*unit_id) {
                unit.garrison.set_container(Some(container_unit_id));
                unit.stop();
                unit.state = UnitState::Idle;
            }
            if let Some(container) = self.units.get_mut(container_unit_id) {
                container.garrison.add_contained_unit(*unit_id);
            }
        }
        if let Some(passenger) = self.squads.get_mut(plan.passenger_squad_id) {
            passenger.remove_all_orders();
            passenger.base.position = plan.start_position;
            passenger
                .garrison
                .mark_garrisoned(container_unit_id, self.game_time_ms);
        }
        self.place_squad_members(
            transport_squad_id,
            plan.start_position,
            flight_forward,
            true,
        );
        self.place_squad_members(
            plan.passenger_squad_id,
            plan.start_position,
            flight_forward,
            false,
        );
        true
    }

    pub(crate) fn update_transport_fly_ins(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let active = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                let action = squad.transport_fly_in.as_ref()?;
                Some((squad_id, action.phase(), action.target(), action.speed()))
            })
            .collect::<Vec<_>>();
        for (transport_squad_id, phase, target, speed) in active {
            let arrived = self.advance_transport(transport_squad_id, target, speed, dt);
            if !arrived {
                continue;
            }
            match phase {
                TransportFlyInPhase::Incoming => {
                    self.complete_transport_dropoff(transport_squad_id);
                }
                TransportFlyInPhase::Outgoing => {
                    let _destroyed = self.kill_squad(transport_squad_id, true);
                }
            }
        }
        self.update_power_transport_flights(dt);
    }

    fn advance_transport(
        &mut self,
        transport_squad_id: EntityId,
        target: Vec3,
        speed: f32,
        dt: f32,
    ) -> bool {
        let Some((position, old_forward)) = self
            .squads
            .get(transport_squad_id)
            .map(|squad| (squad.base.position, squad.base.forward))
        else {
            return false;
        };
        let delta = target - position;
        let distance = delta.length();
        let arrived = !distance.is_finite() || distance <= speed * dt;
        let next_position = if arrived {
            target
        } else {
            position + delta / distance * (speed * dt)
        };
        let forward = if distance > f32::EPSILON {
            flight_forward(position, target)
        } else {
            old_forward
        };
        if let Some(transport) = self.squads.get_mut(transport_squad_id) {
            transport.base.position = next_position;
            transport.base.forward = forward;
            transport.base.velocity = if arrived {
                Vec3::ZERO
            } else {
                (next_position - position) / dt
            };
        }
        self.place_squad_members(transport_squad_id, next_position, forward, true);
        arrived
    }

    fn complete_transport_dropoff(&mut self, transport_squad_id: EntityId) {
        let Some(action) = self
            .squads
            .get(transport_squad_id)
            .and_then(|squad| squad.transport_fly_in.clone())
        else {
            return;
        };
        let passenger_squad_id = action.passenger_squad_id();
        let facing = action.facing().map_or_else(
            || {
                self.squads
                    .get(transport_squad_id)
                    .map_or(Vec3::Z, |squad| normalized_forward(squad.base.forward))
            },
            normalized_forward,
        );
        self.detach_passenger_refs(passenger_squad_id);
        let player_id = self
            .squads
            .get(passenger_squad_id)
            .map(|squad| squad.base.player_id);
        if let Some(passenger) = self.squads.get_mut(passenger_squad_id) {
            passenger.garrison.finish_action();
            passenger.base.position = action.dropoff_position();
            passenger.base.forward = facing;
            passenger.base.velocity = Vec3::ZERO;
            passenger.state = SquadState::Idle;
        }
        self.place_squad_members(passenger_squad_id, action.dropoff_position(), facing, true);
        if let (Some(player_id), Some(rally_point)) = (player_id, action.rally_point()) {
            let _issued = self.issue_squad_move_order_to_position(
                player_id,
                passenger_squad_id,
                rally_point,
                action.attack_move(),
                false,
            );
        }
        if let Some(transport) = self.squads.get_mut(transport_squad_id) {
            if let Some(action) = &mut transport.transport_fly_in {
                action.begin_outgoing();
            }
            transport.state = SquadState::Moving;
        }
    }
}

fn transport_plan_is_finite(plan: SquadTransportPlan) -> bool {
    plan.start_position.is_finite()
        && plan.dropoff_position.is_finite()
        && plan.incoming_target.is_finite()
        && plan.outgoing_target.is_finite()
        && plan.rally_point.is_none_or(Vec3::is_finite)
        && plan.facing.is_none_or(Vec3::is_finite)
}

fn flight_forward(from: Vec3, to: Vec3) -> Vec3 {
    normalized_forward(to - from)
}

fn normalized_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

#[cfg(test)]
#[path = "transports/tests.rs"]
mod tests;
