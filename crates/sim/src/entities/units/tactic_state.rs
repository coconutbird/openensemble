//! Checksummed per-unit tactic-state selection.

use super::Unit;
use crate::gameplay::TacticStateId;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Default)]
pub(super) struct UnitTacticState {
    current: Option<TacticStateId>,
    revision: u32,
}

impl UnitTacticState {
    const fn current(&self) -> Option<TacticStateId> {
        self.current
    }

    const fn revision(&self) -> u32 {
        self.revision
    }

    fn set(&mut self, state: TacticStateId) {
        if self.current != Some(state) {
            self.current = Some(state);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn clear(&mut self) {
        if self.current.take().is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            self.current
                .map_or(u32::MAX, |state| u32::from(state.as_u8())),
        );
        checksum.hash_u32(self.revision);
    }
}

impl Unit {
    /// Return the active authored tactic state, or retail's default state.
    #[must_use]
    pub const fn tactic_state(&self) -> Option<TacticStateId> {
        self.tactic_state.current()
    }

    /// Return the presentation revision incremented by state transitions.
    #[must_use]
    pub const fn tactic_state_revision(&self) -> u32 {
        self.tactic_state.revision()
    }

    pub(crate) fn set_tactic_state(&mut self, state: TacticStateId) {
        self.tactic_state.set(state);
    }

    pub(crate) fn clear_tactic_state(&mut self) {
        self.tactic_state.clear();
    }

    pub(crate) fn hash_tactic_state(&self, checksum: &mut SyncChecksum) {
        self.tactic_state.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::{EntityClass, EntityId};

    #[test]
    fn state_transitions_only_advance_revision_when_the_value_changes() {
        let mut unit = Unit::new(EntityId::new(EntityClass::Unit, 1), 1);
        let state = TacticStateId::from_index(3).unwrap();
        assert_eq!(unit.tactic_state(), None);
        assert_eq!(unit.tactic_state_revision(), 0);

        unit.set_tactic_state(state);
        unit.set_tactic_state(state);
        assert_eq!(unit.tactic_state(), Some(state));
        assert_eq!(unit.tactic_state_revision(), 1);
        unit.clear_tactic_state();
        unit.clear_tactic_state();
        assert_eq!(unit.tactic_state(), None);
        assert_eq!(unit.tactic_state_revision(), 2);
    }
}
