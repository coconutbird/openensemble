//! Native Transport-power carrier pickup, containment, drop-off, and departure.

use super::{World, flight_forward, normalized_forward};
use crate::EntityId;
use crate::entities::squads::{
    PowerTransportPhase, SquadContainmentState, SquadPowerTransport,
    SquadPowerTransportPlan,
};
use crate::entities::{SquadState, UnitState};
use crate::entity::Entity;
use glam::Vec3;
use num_traits::ToPrimitive;

impl World {
    /// Start one synthetic carrier assigned to one or more existing squads.
    pub(crate) fn start_power_transport_flight(
        &mut self,
        transport_squad_id: EntityId,
        mut plan: SquadPowerTransportPlan,
    ) -> bool {
        plan.passenger_squad_ids.sort_unstable();
        plan.passenger_squad_ids.dedup();
        if plan.passenger_squad_ids.is_empty()
            || plan.passenger_squad_ids.contains(&transport_squad_id)
            || !plan.is_finite()
        {
            return false;
        }
        let Some((player_id, speed)) = self
            .get_squad(transport_squad_id)
            .filter(|squad| {
                squad.is_alive()
                    && squad.transport_fly_in.is_none()
                    && squad.power_transport.is_none()
                    && !squad.unit_ids.is_empty()
            })
            .map(|squad| (squad.base.player_id, squad.speed))
        else {
            return false;
        };
        if !plan.passenger_squad_ids.iter().all(|passenger_id| {
            self.power_transport_passenger_is_ready(*passenger_id, player_id)
        }) {
            return false;
        }

        let forward = flight_forward(plan.start_position, plan.pickup_target);
        if let Some(transport) = self.get_squad_mut(transport_squad_id) {
            transport.base.position = plan.start_position;
            transport.base.forward = forward;
            transport.base.velocity = Vec3::ZERO;
            transport.state = SquadState::Moving;
            transport.power_transport = Some(SquadPowerTransport::new(plan.clone(), speed));
        }
        for passenger_id in &plan.passenger_squad_ids {
            if let Some(passenger) = self.get_squad_mut(*passenger_id) {
                passenger.remove_all_orders();
                passenger.state = SquadState::Idle;
                passenger.base.velocity = Vec3::ZERO;
            }
        }
        self.place_squad_members(
            transport_squad_id,
            plan.start_position,
            forward,
            true,
        );
        true
    }

    pub(crate) fn squad_has_power_transport_reservation(&self, squad_id: EntityId) -> bool {
        self.squads.iter().any(|(_, carrier)| {
            carrier.power_transport.as_ref().is_some_and(|action| {
                action.passenger_squad_ids().binary_search(&squad_id).is_ok()
            })
        })
    }

    pub(super) fn update_power_transport_flights(&mut self, dt: f32) {
        let active = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                let action = squad.power_transport.as_ref()?;
                Some((squad_id, action.phase(), action.target(), action.speed()))
            })
            .collect::<Vec<_>>();
        for (transport_squad_id, phase, target, speed) in active {
            if !self.advance_transport(transport_squad_id, target, speed, dt) {
                continue;
            }
            match phase {
                PowerTransportPhase::Incoming => {
                    self.load_power_transport(transport_squad_id);
                }
                PowerTransportPhase::Transporting => {
                    self.unload_power_transport(transport_squad_id);
                }
                PowerTransportPhase::Outgoing => {
                    let _destroyed = self.kill_squad(transport_squad_id, true);
                }
            }
        }
    }

    fn power_transport_passenger_is_ready(
        &self,
        passenger_id: EntityId,
        player_id: u8,
    ) -> bool {
        self.get_squad(passenger_id).is_some_and(|passenger| {
            passenger.is_alive()
                && passenger.base.player_id == player_id
                && matches!(passenger.garrison.state(), SquadContainmentState::Free)
                && !self.squad_has_power_transport_reservation(passenger_id)
        })
    }

    fn load_power_transport(&mut self, transport_squad_id: EntityId) {
        let Some((passenger_ids, container_unit_id, position, forward, player_id)) = self
            .get_squad(transport_squad_id)
            .and_then(|transport| {
                Some((
                    transport
                        .power_transport
                        .as_ref()?
                        .passenger_squad_ids()
                        .to_vec(),
                    *transport.unit_ids.first()?,
                    transport.base.position,
                    transport.base.forward,
                    transport.base.player_id,
                ))
            })
        else {
            return;
        };
        let mut loaded = Vec::with_capacity(passenger_ids.len());
        for passenger_id in passenger_ids {
            if !self.power_transport_passenger_can_load(passenger_id, player_id) {
                continue;
            }
            self.attach_power_transport_passenger(
                transport_squad_id,
                container_unit_id,
                passenger_id,
                position,
                forward,
            );
            loaded.push(passenger_id);
        }
        if let Some(transport) = self.get_squad_mut(transport_squad_id)
            && let Some(action) = &mut transport.power_transport
        {
            if loaded.is_empty() {
                action.begin_outgoing();
            } else {
                action.begin_transporting(loaded);
            }
        }
    }

    fn power_transport_passenger_can_load(&self, passenger_id: EntityId, player_id: u8) -> bool {
        self.get_squad(passenger_id).is_some_and(|passenger| {
            passenger.is_alive()
                && passenger.base.player_id == player_id
                && matches!(passenger.garrison.state(), SquadContainmentState::Free)
                && !passenger.unit_ids.is_empty()
        })
    }

    fn attach_power_transport_passenger(
        &mut self,
        transport_squad_id: EntityId,
        container_unit_id: EntityId,
        passenger_id: EntityId,
        position: Vec3,
        forward: Vec3,
    ) {
        let unit_ids = self
            .get_squad(passenger_id)
            .map_or_else(Vec::new, |passenger| passenger.unit_ids.clone());
        if let Some(transport) = self.get_squad_mut(transport_squad_id) {
            transport.garrison.add_contained_squad(passenger_id);
        }
        for unit_id in &unit_ids {
            if let Some(unit) = self.get_unit_mut(*unit_id) {
                unit.garrison.set_container(Some(container_unit_id));
                unit.stop();
                unit.state = UnitState::Idle;
            }
            if let Some(container) = self.get_unit_mut(container_unit_id) {
                container.garrison.add_contained_unit(*unit_id);
            }
        }
        let now_ms = self.game_time_ms;
        if let Some(passenger) = self.get_squad_mut(passenger_id) {
            passenger.remove_all_orders();
            passenger.base.position = position;
            passenger.base.forward = forward;
            passenger.base.velocity = Vec3::ZERO;
            passenger
                .garrison
                .mark_garrisoned(container_unit_id, now_ms);
        }
        self.place_squad_members(passenger_id, position, forward, false);
    }

    fn unload_power_transport(&mut self, transport_squad_id: EntityId) {
        let Some((passenger_ids, dropoff, forward)) = self
            .get_squad(transport_squad_id)
            .and_then(|transport| {
                Some((
                    transport
                        .power_transport
                        .as_ref()?
                        .passenger_squad_ids()
                        .to_vec(),
                    transport.power_transport.as_ref()?.dropoff_position(),
                    normalized_forward(transport.base.forward),
                ))
            })
        else {
            return;
        };
        let right = Vec3::Y.cross(forward).normalize_or(Vec3::X);
        let center = passenger_ids
            .len()
            .saturating_sub(1)
            .to_f32()
            .unwrap_or(f32::MAX)
            * 0.5;
        for (index, passenger_id) in passenger_ids.into_iter().enumerate() {
            let mut position = dropoff
                + right * ((index.to_f32().unwrap_or(f32::MAX) - center) * 4.0);
            if let Some(height) = self.terrain_height(position, true) {
                position.y = height;
            }
            self.detach_passenger_refs(passenger_id);
            if let Some(passenger) = self.get_squad_mut(passenger_id) {
                passenger.garrison.finish_action();
                passenger.base.position = position;
                passenger.base.forward = forward;
                passenger.base.velocity = Vec3::ZERO;
                passenger.state = SquadState::Idle;
            }
            self.place_squad_members(passenger_id, position, forward, true);
        }
        if let Some(transport) = self.get_squad_mut(transport_squad_id) {
            if let Some(action) = &mut transport.power_transport {
                action.begin_outgoing();
            }
            transport.state = SquadState::Moving;
        }
    }
}
