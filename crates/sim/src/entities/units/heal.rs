//! Persistent retail per-unit `Heal` action state.

use super::Unit;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// Observable phase of a unit's persistent `Heal` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HealPhase {
    /// The prototype has no connected `Heal` action.
    #[default]
    None,
    /// The action is waiting for an eligible, sufficiently idle squad.
    Waiting,
    /// The retail high-priority heal opportunity is active.
    Working,
}

/// Persistent state owned by one retail `BUnitActionHeal`.
#[derive(Debug, Clone, Default)]
pub(crate) struct UnitHeal {
    action_name: String,
    target_squad_id: Option<EntityId>,
    phase: HealPhase,
}

impl UnitHeal {
    pub(crate) fn connect(&mut self, action_name: &str) {
        if self.action_name.eq_ignore_ascii_case(action_name) {
            return;
        }
        self.action_name.clear();
        self.action_name.push_str(action_name);
        self.target_squad_id = None;
        self.phase = HealPhase::Waiting;
    }

    pub(crate) fn wait_for(&mut self, target_squad_id: Option<EntityId>) {
        self.target_squad_id = target_squad_id;
        self.phase = HealPhase::Waiting;
    }

    pub(crate) fn set_target(&mut self, target_squad_id: Option<EntityId>) {
        self.target_squad_id = target_squad_id;
    }

    pub(crate) fn begin_work(&mut self, target_squad_id: EntityId) {
        self.target_squad_id = Some(target_squad_id);
        self.phase = HealPhase::Working;
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) const fn target_squad_id(&self) -> Option<EntityId> {
        self.target_squad_id
    }

    pub(crate) const fn phase(&self) -> HealPhase {
        self.phase
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
        checksum.hash_u32(self.target_squad_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.phase as u32);
    }
}

impl Unit {
    /// Return the current persistent healing phase.
    #[must_use]
    pub fn heal_phase(&self) -> HealPhase {
        self.heal.phase()
    }

    /// Return the squad selected by this unit's persistent heal action.
    #[must_use]
    pub fn heal_target(&self) -> Option<EntityId> {
        self.heal.target_squad_id()
    }

    /// Return whether the unit currently owns the retail heal opportunity.
    #[must_use]
    pub fn is_healing(&self) -> bool {
        self.heal.phase() == HealPhase::Working
    }

    pub(crate) fn hash_heal_state(&self, checksum: &mut SyncChecksum) {
        self.heal.hash_state(checksum);
    }
}
