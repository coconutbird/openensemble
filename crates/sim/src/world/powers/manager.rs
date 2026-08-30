//! Deterministic storage and checksumming for running native powers.

use super::{
    CarpetBombingPowerExecution, CleansingPowerExecution, CryoPowerExecution,
    DisruptionPowerExecution, OdstPowerExecution, OrbitalPowerExecution, PowerExecutionId,
    RagePowerExecution, RepairPowerExecution, TransportPowerExecution, WavePowerExecution, rage,
};
use crate::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub(super) struct PowerVisualLifetime {
    pub object_id: EntityId,
    pub expires_at_ms: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PowerDebris {
    pub object_id: EntityId,
    pub velocity: Vec3,
    pub expires_at_ms: u32,
}

#[derive(Debug)]
pub(crate) struct PowerManagerState {
    next_execution_id: u32,
    pub(super) carpet_bombing_executions: Vec<CarpetBombingPowerExecution>,
    pub(super) cleansing_executions: Vec<CleansingPowerExecution>,
    pub(super) cryo_executions: Vec<CryoPowerExecution>,
    pub(super) disruption_executions: Vec<DisruptionPowerExecution>,
    pub(super) odst_executions: Vec<OdstPowerExecution>,
    pub(super) orbital_executions: Vec<OrbitalPowerExecution>,
    pub(super) rage_executions: Vec<RagePowerExecution>,
    pub(super) repair_executions: Vec<RepairPowerExecution>,
    pub(super) transport_executions: Vec<TransportPowerExecution>,
    pub(super) wave_executions: Vec<WavePowerExecution>,
    pub(super) pending_rage_kills: Vec<rage::PendingRageKill>,
    pub(super) transient_visuals: Vec<PowerVisualLifetime>,
    pub(super) debris: Vec<PowerDebris>,
}

impl Default for PowerManagerState {
    fn default() -> Self {
        Self {
            next_execution_id: 1,
            carpet_bombing_executions: Vec::new(),
            cleansing_executions: Vec::new(),
            cryo_executions: Vec::new(),
            disruption_executions: Vec::new(),
            odst_executions: Vec::new(),
            orbital_executions: Vec::new(),
            rage_executions: Vec::new(),
            repair_executions: Vec::new(),
            transport_executions: Vec::new(),
            wave_executions: Vec::new(),
            pending_rage_kills: Vec::new(),
            transient_visuals: Vec::new(),
            debris: Vec::new(),
        }
    }
}

impl PowerManagerState {
    pub(super) fn allocate_id(&mut self) -> PowerExecutionId {
        let id = PowerExecutionId(self.next_execution_id.max(1));
        self.next_execution_id = id.0.wrapping_add(1).max(1);
        id
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn track_visual(&mut self, object_id: EntityId, expires_at_ms: u32) {
        self.forget_visual(object_id);
        self.transient_visuals.push(PowerVisualLifetime {
            object_id,
            expires_at_ms,
        });
    }

    pub(crate) fn forget_visual(&mut self, object_id: EntityId) {
        self.transient_visuals
            .retain(|visual| visual.object_id != object_id);
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.next_execution_id);
        hash_executions(
            checksum,
            &self.carpet_bombing_executions,
            |entry, checksum| {
                entry.hash_state(checksum);
            },
        );
        hash_executions(checksum, &self.cleansing_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.cryo_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.disruption_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.odst_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.orbital_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.rage_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.repair_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.transport_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.wave_executions, |entry, checksum| {
            entry.hash_state(checksum);
        });
        hash_executions(checksum, &self.pending_rage_kills, |entry, checksum| {
            entry.hash_state(checksum);
        });
        checksum.hash_u32(u32::try_from(self.transient_visuals.len()).unwrap_or(u32::MAX));
        for visual in &self.transient_visuals {
            checksum.hash_u32(visual.object_id.as_u32());
            checksum.hash_u32(visual.expires_at_ms);
        }
        checksum.hash_u32(u32::try_from(self.debris.len()).unwrap_or(u32::MAX));
        for debris in &self.debris {
            checksum.hash_u32(debris.object_id.as_u32());
            checksum.hash_vec3(debris.velocity.x, debris.velocity.y, debris.velocity.z);
            checksum.hash_u32(debris.expires_at_ms);
        }
    }
}

fn hash_executions<T>(
    checksum: &mut SyncChecksum,
    entries: &[T],
    mut hash: impl FnMut(&T, &mut SyncChecksum),
) {
    checksum.hash_u32(u32::try_from(entries.len()).unwrap_or(u32::MAX));
    for entry in entries {
        hash(entry, checksum);
    }
}
