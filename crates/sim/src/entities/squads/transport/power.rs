//! Native-power carrier flight state owned by a synthetic transport squad.

use crate::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Authoritative phase of a retail transport-power carrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerTransportPhase {
    /// Carrier is flying in to collect its assigned squads.
    Incoming,
    /// Assigned squads are contained while the carrier flies to drop-off.
    Transporting,
    /// Carrier has unloaded its passengers and is leaving the map.
    Outgoing,
}

/// Sim-owned transport action attached to one synthetic carrier squad.
#[derive(Debug, Clone, PartialEq)]
pub struct SquadPowerTransport {
    passenger_squad_ids: Vec<EntityId>,
    pickup_position: Vec3,
    pickup_target: Vec3,
    dropoff_position: Vec3,
    dropoff_target: Vec3,
    outgoing_target: Vec3,
    speed: f32,
    phase: PowerTransportPhase,
}

/// Validated setup supplied by the native Transport power.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SquadPowerTransportPlan {
    pub(crate) passenger_squad_ids: Vec<EntityId>,
    pub(crate) start_position: Vec3,
    pub(crate) pickup_position: Vec3,
    pub(crate) pickup_target: Vec3,
    pub(crate) dropoff_position: Vec3,
    pub(crate) dropoff_target: Vec3,
    pub(crate) outgoing_target: Vec3,
}

impl SquadPowerTransport {
    pub(crate) fn new(plan: SquadPowerTransportPlan, speed: f32) -> Self {
        Self {
            passenger_squad_ids: plan.passenger_squad_ids,
            pickup_position: plan.pickup_position,
            pickup_target: plan.pickup_target,
            dropoff_position: plan.dropoff_position,
            dropoff_target: plan.dropoff_target,
            outgoing_target: plan.outgoing_target,
            speed: valid_speed(speed),
            phase: PowerTransportPhase::Incoming,
        }
    }

    /// Logical squads assigned to this carrier in deterministic order.
    #[must_use]
    pub fn passenger_squad_ids(&self) -> &[EntityId] {
        &self.passenger_squad_ids
    }

    /// Current carrier action phase.
    #[must_use]
    pub const fn phase(&self) -> PowerTransportPhase {
        self.phase
    }

    /// Ground pickup position calculated from the selected squad formation.
    #[must_use]
    pub const fn pickup_position(&self) -> Vec3 {
        self.pickup_position
    }

    /// Ground position where this carrier releases its assigned squads.
    #[must_use]
    pub const fn dropoff_position(&self) -> Vec3 {
        self.dropoff_position
    }

    pub(crate) const fn target(&self) -> Vec3 {
        match self.phase {
            PowerTransportPhase::Incoming => self.pickup_target,
            PowerTransportPhase::Transporting => self.dropoff_target,
            PowerTransportPhase::Outgoing => self.outgoing_target,
        }
    }

    pub(crate) const fn speed(&self) -> f32 {
        self.speed
    }

    pub(crate) fn begin_transporting(&mut self, passenger_squad_ids: Vec<EntityId>) {
        self.passenger_squad_ids = passenger_squad_ids;
        self.phase = PowerTransportPhase::Transporting;
    }

    pub(crate) fn begin_outgoing(&mut self) {
        self.phase = PowerTransportPhase::Outgoing;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.passenger_squad_ids.len()).unwrap_or(u32::MAX));
        for passenger_id in &self.passenger_squad_ids {
            checksum.hash_u32(passenger_id.as_u32());
        }
        hash_vec3(checksum, self.pickup_position);
        hash_vec3(checksum, self.pickup_target);
        hash_vec3(checksum, self.dropoff_position);
        hash_vec3(checksum, self.dropoff_target);
        hash_vec3(checksum, self.outgoing_target);
        checksum.hash_f32(self.speed);
        checksum.hash_u32(match self.phase {
            PowerTransportPhase::Incoming => 0,
            PowerTransportPhase::Transporting => 1,
            PowerTransportPhase::Outgoing => 2,
        });
    }
}

impl SquadPowerTransportPlan {
    pub(crate) fn is_finite(&self) -> bool {
        self.start_position.is_finite()
            && self.pickup_position.is_finite()
            && self.pickup_target.is_finite()
            && self.dropoff_position.is_finite()
            && self.dropoff_target.is_finite()
            && self.outgoing_target.is_finite()
    }
}

fn valid_speed(speed: f32) -> f32 {
    if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        30.0
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}
