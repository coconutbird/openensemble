//! Per-instance technology state for retail unique proto-unit research.

use super::Unit;
use crate::sync::SyncChecksum;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub(super) struct UnitUniqueTechnologies {
    active: BTreeSet<i32>,
}

impl UnitUniqueTechnologies {
    fn activate(&mut self, technology_id: i32) -> bool {
        self.active.insert(technology_id)
    }

    fn is_active(&self, technology_id: i32) -> bool {
        self.active.contains(&technology_id)
    }

    fn active(&self) -> impl Iterator<Item = i32> + '_ {
        self.active.iter().copied()
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.active.len()).unwrap_or(u32::MAX));
        for &technology_id in &self.active {
            checksum.hash_i32(technology_id);
        }
    }
}

impl Unit {
    /// Return whether this live unit has activated one per-instance technology.
    #[must_use]
    pub fn unique_technology_is_active(&self, technology_id: i32) -> bool {
        self.unique_technologies.is_active(technology_id)
    }

    /// Iterate runtime IDs for per-instance technologies active on this unit.
    pub fn active_unique_technologies(&self) -> impl Iterator<Item = i32> + '_ {
        self.unique_technologies.active()
    }

    pub(crate) fn activate_unique_technology(&mut self, technology_id: i32) -> bool {
        self.unique_technologies.activate(technology_id)
    }

    pub(crate) fn hash_unique_technology_state(&self, checksum: &mut SyncChecksum) {
        self.unique_technologies.hash_state(checksum);
    }
}
