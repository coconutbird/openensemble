//! Trigger-created squad transport flight state.

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

mod power;

pub(crate) use power::SquadPowerTransportPlan;
pub use power::{PowerTransportPhase, SquadPowerTransport};

/// Authoritative phase of a trigger-created transport flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFlyInPhase {
    /// Carrier is approaching the authored drop-off.
    Incoming,
    /// Carrier has unloaded passengers and is leaving the map.
    Outgoing,
}

/// Sim-owned transport action attached to the synthetic carrier squad.
#[derive(Debug, Clone, PartialEq)]
pub struct SquadTransportFlyIn {
    passenger_squad_ids: Vec<EntityId>,
    dropoff_position: Vec3,
    incoming_target: Vec3,
    outgoing_target: Vec3,
    rally_point: Option<Vec3>,
    attack_move: bool,
    facing: Option<Vec3>,
    speed: f32,
    phase: TransportFlyInPhase,
}

/// Validated setup supplied by the trigger effect after creating a carrier.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SquadTransportPlan {
    pub(crate) passenger_squad_ids: Vec<EntityId>,
    pub(crate) start_position: Vec3,
    pub(crate) dropoff_position: Vec3,
    pub(crate) incoming_target: Vec3,
    pub(crate) outgoing_target: Vec3,
    pub(crate) rally_point: Option<Vec3>,
    pub(crate) attack_move: bool,
    pub(crate) facing: Option<Vec3>,
}

impl SquadTransportFlyIn {
    pub(crate) fn new(plan: SquadTransportPlan, speed: f32) -> Self {
        Self {
            passenger_squad_ids: plan.passenger_squad_ids,
            dropoff_position: plan.dropoff_position,
            incoming_target: plan.incoming_target,
            outgoing_target: plan.outgoing_target,
            rally_point: plan.rally_point,
            attack_move: plan.attack_move,
            facing: plan.facing,
            speed: valid_speed(speed),
            phase: TransportFlyInPhase::Incoming,
        }
    }

    /// Logical squad currently carried by this transport.
    #[must_use]
    pub fn passenger_squad_id(&self) -> EntityId {
        self.passenger_squad_ids[0]
    }

    /// Logical squads currently carried by this transport in assignment order.
    #[must_use]
    pub fn passenger_squad_ids(&self) -> &[EntityId] {
        &self.passenger_squad_ids
    }

    /// Current carrier action phase.
    #[must_use]
    pub const fn phase(&self) -> TransportFlyInPhase {
        self.phase
    }

    /// Authored ground position where passengers are released.
    #[must_use]
    pub const fn dropoff_position(&self) -> Vec3 {
        self.dropoff_position
    }

    pub(crate) const fn target(&self) -> Vec3 {
        match self.phase {
            TransportFlyInPhase::Incoming => self.incoming_target,
            TransportFlyInPhase::Outgoing => self.outgoing_target,
        }
    }

    pub(crate) const fn speed(&self) -> f32 {
        self.speed
    }

    pub(crate) const fn rally_point(&self) -> Option<Vec3> {
        self.rally_point
    }

    pub(crate) const fn attack_move(&self) -> bool {
        self.attack_move
    }

    pub(crate) const fn facing(&self) -> Option<Vec3> {
        self.facing
    }

    pub(crate) fn begin_outgoing(&mut self) {
        self.phase = TransportFlyInPhase::Outgoing;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.passenger_squad_ids.len()).unwrap_or(u32::MAX));
        for passenger_id in &self.passenger_squad_ids {
            checksum.hash_u32(passenger_id.as_u32());
        }
        checksum.hash_vec3(
            self.dropoff_position.x,
            self.dropoff_position.y,
            self.dropoff_position.z,
        );
        checksum.hash_vec3(
            self.incoming_target.x,
            self.incoming_target.y,
            self.incoming_target.z,
        );
        checksum.hash_vec3(
            self.outgoing_target.x,
            self.outgoing_target.y,
            self.outgoing_target.z,
        );
        hash_optional_vec3(checksum, self.rally_point);
        checksum.hash_u32(u32::from(self.attack_move));
        hash_optional_vec3(checksum, self.facing);
        checksum.hash_f32(self.speed);
        checksum.hash_u32(match self.phase {
            TransportFlyInPhase::Incoming => 0,
            TransportFlyInPhase::Outgoing => 1,
        });
    }
}

fn valid_speed(speed: f32) -> f32 {
    if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        30.0
    }
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    checksum.hash_u32(u32::from(value.is_some()));
    if let Some(value) = value {
        checksum.hash_vec3(value.x, value.y, value.z);
    }
}
