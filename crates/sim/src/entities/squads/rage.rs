//! Squad-owned reference state for retail's persistent Rage action.

use crate::sync::SyncChecksum;

/// Active Rage sources lock ordinary squad work while the power owns control.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SquadRage {
    sources: u32,
}

impl SquadRage {
    pub(crate) fn begin(&mut self) -> bool {
        let first = self.sources == 0;
        self.sources = self.sources.saturating_add(1);
        first
    }

    pub(crate) fn end(&mut self) -> bool {
        self.sources = self.sources.saturating_sub(1);
        self.sources == 0
    }

    pub(crate) const fn is_active(self) -> bool {
        self.sources > 0
    }

    pub(crate) const fn source_count(self) -> u32 {
        self.sources
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.sources);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_sources_only_release_on_the_last_end() {
        let mut rage = SquadRage::default();
        assert!(rage.begin());
        assert!(!rage.begin());
        assert!(!rage.end());
        assert!(rage.is_active());
        assert!(rage.end());
        assert!(!rage.is_active());
    }
}
