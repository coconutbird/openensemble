//! Work `Transport` order: a controllable `BSquadActionTransport`.

use super::World;
use crate::entities::SquadState;
use crate::entities::squads::{SquadTransportFlyIn, SquadTransportPlan};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use glam::Vec3;

impl World {
    /// Start order 23 (`BSquad_doTransport` @ `0x1403A2C70`): the squad carries its garrisoned
    /// squads to `dropoff`, unloads them and stays there.
    ///
    /// `BSquadActionTransport::connect` fails when the order has no valid target position.
    /// The passengers are the squads garrisoned in the transport. The flight uses the trigger
    /// transport flight; the retail move action and the exit-location search
    /// (`BSimHelper::evaluateExitLocation`) are not modelled.
    pub fn issue_transport_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        dropoff: Option<Vec3>,
    ) -> bool {
        let Some(dropoff) = dropoff.filter(|position| position.is_finite()) else {
            return false;
        };
        let Some((start_position, speed, passenger_squad_ids)) =
            self.squads.get(squad_id).and_then(|squad| {
                (Self::command_reaches_squad_owner(player_id, squad.base.player_id)
                    && squad.is_alive()
                    && !squad.unit_ids.is_empty()
                    && !squad.garrison.is_garrisoned()
                    && squad.transport_fly_in.is_none()
                    && squad.power_transport.is_none())
                .then(|| {
                    (
                        squad.base.position,
                        squad.speed,
                        squad.garrison.contained_squad_ids().to_vec(),
                    )
                })
            })
        else {
            return false;
        };
        let _capture_cancelled = self.cancel_capture_order(squad_id);
        let _repair_cancelled = self.cancel_repair_other_order(squad_id);
        let flight_target = Vec3::new(dropoff.x, start_position.y, dropoff.z);
        let plan = SquadTransportPlan {
            passenger_squad_ids,
            start_position,
            dropoff_position: dropoff,
            incoming_target: flight_target,
            outgoing_target: flight_target,
            rally_point: None,
            attack_move: false,
            facing: None,
        };
        let Some(transport) = self.squads.get_mut(squad_id) else {
            return false;
        };
        transport.stop();
        transport.work_action = None;
        transport.transport_fly_in = Some(SquadTransportFlyIn::new_controllable(plan, speed));
        transport.state = SquadState::Moving;
        true
    }
}
