//! Persistent per-unit state for a retail collision-attack action.

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// Checksummed collision-attack lifecycle and already-processed targets.
#[derive(Debug, Clone, Default)]
pub(crate) struct UnitCollisionAttack {
    active: bool,
    impacted_unit_ids: Vec<EntityId>,
}

impl UnitCollisionAttack {
    pub(crate) const fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn begin(&mut self) -> bool {
        if self.active {
            return false;
        }
        self.active = true;
        self.impacted_unit_ids.clear();
        true
    }

    pub(crate) fn finish(&mut self) {
        self.active = false;
        self.impacted_unit_ids.clear();
    }

    pub(crate) fn has_impacted(&self, unit_id: EntityId) -> bool {
        self.impacted_unit_ids.binary_search(&unit_id).is_ok()
    }

    pub(crate) fn mark_impacted(&mut self, unit_id: EntityId) {
        if let Err(index) = self.impacted_unit_ids.binary_search(&unit_id) {
            self.impacted_unit_ids.insert(index, unit_id);
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.active));
        checksum.hash_u32(u32::try_from(self.impacted_unit_ids.len()).unwrap_or(u32::MAX));
        for unit_id in &self.impacted_unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn impact_ids_are_sorted_unique_and_reset_with_the_action() {
        let first = EntityId::new(EntityClass::Unit, 1);
        let second = EntityId::new(EntityClass::Unit, 2);
        let mut state = UnitCollisionAttack::default();
        assert!(state.begin());
        state.mark_impacted(second);
        state.mark_impacted(first);
        state.mark_impacted(second);
        assert!(state.has_impacted(first));
        assert!(state.has_impacted(second));
        assert!(!state.begin());
        state.finish();
        assert!(!state.is_active());
        assert!(!state.has_impacted(first));
    }
}
