//! Shared retail repair-reference state for a squad.

use crate::sync::SyncChecksum;

/// Reference count used by overlapping repair actions.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SquadRepair {
    regen_sources: u32,
}

impl SquadRepair {
    /// Add one repair source and report whether it is the first source.
    pub(crate) fn begin(&mut self) -> bool {
        let first = self.regen_sources == 0;
        self.regen_sources = self.regen_sources.saturating_add(1);
        first
    }

    /// Remove one repair source and report whether visuals should be removed.
    pub(crate) fn end(&mut self) -> bool {
        if self.regen_sources == 0 {
            return true;
        }
        self.regen_sources -= 1;
        self.regen_sources == 0
    }

    pub(crate) const fn source_count(self) -> u32 {
        self.regen_sources
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.regen_sources);
    }
}
