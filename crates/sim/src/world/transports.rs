//! Authoritative trigger-created carrier flight and passenger release.

use super::World;
use crate::entities::squads::{
    SquadContainmentState, SquadTransportFlyIn, SquadTransportPlan, TransportFlyInPhase,
};
use crate::entities::{SquadState, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use glam::Vec3;
use num_traits::ToPrimitive;
use std::collections::BTreeSet;

mod planning;
mod power;

pub(crate) use planning::{
    TransportGroupPlan, TransportGroupRequest, average_transport_position, plan_transport_groups,
    transport_carrier_spacing,
};

impl World {
    /// Attach a newly created squad to a synthetic transport and start flight.
    pub(crate) fn start_transport_fly_in(
        &mut self,
        transport_squad_id: EntityId,
        plan: SquadTransportPlan,
    ) -> bool {
        self.start_transport_fly_in_batch(vec![(transport_squad_id, plan)])
    }

    /// Atomically preload and launch one or more trigger-created carriers.
    pub(crate) fn start_transport_fly_in_batch(
        &mut self,
        flights: Vec<(EntityId, SquadTransportPlan)>,
    ) -> bool {
        if !self.transport_fly_in_batch_is_valid(&flights) {
            return false;
        }
        for (transport_squad_id, plan) in flights {
            self.start_validated_transport_fly_in(transport_squad_id, plan);
        }
        true
    }

    fn start_validated_transport_fly_in(
        &mut self,
        transport_squad_id: EntityId,
        plan: SquadTransportPlan,
    ) {
        let passenger_squad_ids = plan.passenger_squad_ids.clone();
        let (container_unit_id, speed) = self
            .squads
            .get(transport_squad_id)
            .and_then(|squad| Some((*squad.unit_ids.first()?, squad.speed)))
            .expect("validated trigger transport carrier");
        let start_position = plan.start_position;
        let flight_forward = flight_forward(start_position, plan.incoming_target);
        let action = SquadTransportFlyIn::new(plan, speed);
        if let Some(transport) = self.squads.get_mut(transport_squad_id) {
            transport.base.position = start_position;
            transport.base.forward = flight_forward;
            transport.base.velocity = Vec3::ZERO;
            transport.state = SquadState::Moving;
            for passenger_squad_id in &passenger_squad_ids {
                transport.garrison.add_contained_squad(*passenger_squad_id);
            }
            transport.transport_fly_in = Some(action);
        }
        for passenger_squad_id in passenger_squad_ids {
            self.attach_trigger_transport_passenger(
                passenger_squad_id,
                container_unit_id,
                start_position,
                flight_forward,
            );
        }
        self.place_squad_members(transport_squad_id, start_position, flight_forward, true);
    }

    fn attach_trigger_transport_passenger(
        &mut self,
        passenger_squad_id: EntityId,
        container_unit_id: EntityId,
        position: Vec3,
        forward: Vec3,
    ) {
        let passenger_units = self
            .squads
            .get(passenger_squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        for unit_id in passenger_units {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.garrison.set_container(Some(container_unit_id));
                unit.stop();
                unit.state = UnitState::Idle;
            }
            if let Some(container) = self.units.get_mut(container_unit_id) {
                container.garrison.add_contained_unit(unit_id);
            }
        }
        if let Some(passenger) = self.squads.get_mut(passenger_squad_id) {
            passenger.remove_all_orders();
            passenger.base.position = position;
            passenger.base.forward = forward;
            passenger.base.velocity = Vec3::ZERO;
            passenger.state = SquadState::Idle;
            passenger
                .garrison
                .mark_garrisoned(container_unit_id, self.game_time_ms);
        }
        self.place_squad_members(passenger_squad_id, position, forward, false);
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
        let facing = action.facing().map_or_else(
            || {
                self.squads
                    .get(transport_squad_id)
                    .map_or(Vec3::Z, |squad| normalized_forward(squad.base.forward))
            },
            normalized_forward,
        );
        let passengers = action.passenger_squad_ids().to_vec();
        let right = Vec3::Y.cross(facing).normalize_or(Vec3::X);
        let center = passengers
            .len()
            .saturating_sub(1)
            .to_f32()
            .unwrap_or(f32::MAX)
            * 0.5;
        for (index, passenger_squad_id) in passengers.into_iter().enumerate() {
            let lateral = index.to_f32().unwrap_or(f32::MAX) - center;
            let mut position = action.dropoff_position() + right * (lateral * 4.0);
            if let Some(height) = self.terrain_height(position, true) {
                position.y = height;
            }
            self.release_trigger_transport_passenger(
                passenger_squad_id,
                position,
                facing,
                action.rally_point(),
                action.attack_move(),
            );
        }
        if let Some(transport) = self.squads.get_mut(transport_squad_id) {
            if let Some(action) = &mut transport.transport_fly_in {
                action.begin_outgoing();
            }
            transport.state = SquadState::Moving;
        }
    }

    fn release_trigger_transport_passenger(
        &mut self,
        passenger_squad_id: EntityId,
        position: Vec3,
        facing: Vec3,
        rally_point: Option<Vec3>,
        attack_move: bool,
    ) {
        self.detach_passenger_refs(passenger_squad_id);
        let player_id = self
            .squads
            .get(passenger_squad_id)
            .map(|squad| squad.base.player_id);
        if let Some(passenger) = self.squads.get_mut(passenger_squad_id) {
            passenger.garrison.finish_action();
            passenger.base.position = position;
            passenger.base.forward = facing;
            passenger.base.velocity = Vec3::ZERO;
            passenger.state = SquadState::Idle;
        }
        self.place_squad_members(passenger_squad_id, position, facing, true);
        if let (Some(player_id), Some(rally_point)) = (player_id, rally_point) {
            let _issued = self.issue_squad_move_order_to_position(
                player_id,
                passenger_squad_id,
                rally_point,
                attack_move,
                false,
            );
        }
    }

    fn transport_fly_in_batch_is_valid(&self, flights: &[(EntityId, SquadTransportPlan)]) -> bool {
        if flights.is_empty() {
            return false;
        }
        let carriers = flights
            .iter()
            .map(|(carrier_id, _)| *carrier_id)
            .collect::<BTreeSet<_>>();
        if carriers.len() != flights.len() {
            return false;
        }
        let mut passengers = BTreeSet::new();
        for (carrier_id, plan) in flights {
            let Some(player_id) = self.trigger_transport_carrier_player(*carrier_id) else {
                return false;
            };
            if plan.passenger_squad_ids.is_empty() || !transport_plan_is_finite(plan) {
                return false;
            }
            for passenger_id in &plan.passenger_squad_ids {
                if carriers.contains(passenger_id)
                    || !passengers.insert(*passenger_id)
                    || !self.trigger_transport_passenger_is_ready(*passenger_id, player_id)
                {
                    return false;
                }
            }
        }
        true
    }

    fn trigger_transport_carrier_player(&self, carrier_id: EntityId) -> Option<u8> {
        self.squads.get(carrier_id).and_then(|carrier| {
            (carrier.is_alive()
                && carrier.transport_fly_in.is_none()
                && carrier.power_transport.is_none()
                && carrier
                    .unit_ids
                    .first()
                    .is_some_and(|unit_id| self.units.contains(*unit_id)))
            .then_some(carrier.base.player_id)
        })
    }

    fn trigger_transport_passenger_is_ready(&self, passenger_id: EntityId, player_id: u8) -> bool {
        self.squads.get(passenger_id).is_some_and(|passenger| {
            passenger.is_alive()
                && passenger.base.player_id == player_id
                && !passenger.unit_ids.is_empty()
                && matches!(passenger.garrison.state(), SquadContainmentState::Free)
        })
    }
}

fn transport_plan_is_finite(plan: &SquadTransportPlan) -> bool {
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
