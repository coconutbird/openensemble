//! Authoritative finite-resource and per-unit `Gather` action state.

use super::Unit;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// Runtime phase of a unit or squad gather action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum GatherPhase {
    /// No gather action is connected.
    #[default]
    None,
    /// The source is approaching its target.
    Moving,
    /// The source owns the action controllers and is gathering.
    Working,
    /// The finite target was exhausted successfully.
    Done,
    /// The action lost or rejected its target.
    Failed,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitGatherAction {
    target_id: Option<EntityId>,
    action_name: String,
    phase: GatherPhase,
}

impl UnitGatherAction {
    pub(crate) fn start(&mut self, target_id: EntityId, action_name: &str) {
        self.target_id = Some(target_id);
        action_name.clone_into(&mut self.action_name);
        self.phase = GatherPhase::Moving;
    }

    pub(crate) fn set_phase(&mut self, phase: GatherPhase) {
        self.phase = phase;
    }

    pub(crate) fn cancel(&mut self) {
        self.target_id = None;
        self.action_name.clear();
        self.phase = GatherPhase::None;
    }

    pub(crate) const fn target_id(&self) -> Option<EntityId> {
        self.target_id
    }

    pub(crate) fn action_name(&self) -> &str {
        &self.action_name
    }

    pub(crate) const fn phase(&self) -> GatherPhase {
        self.phase
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            self.target_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitResourceNode {
    resource_name: Option<String>,
    amount: f32,
    unlimited: bool,
    die_at_zero: bool,
    gatherer_limit: i32,
}

impl UnitResourceNode {
    pub(crate) fn configure(
        &mut self,
        resource_name: Option<String>,
        amount: Option<f32>,
        unlimited: bool,
        die_at_zero: bool,
        gatherer_limit: Option<i32>,
    ) {
        self.resource_name = resource_name;
        self.amount = amount
            .filter(|amount| amount.is_finite() && *amount >= 0.0)
            .unwrap_or_default();
        self.unlimited = unlimited;
        self.die_at_zero = die_at_zero;
        self.gatherer_limit = gatherer_limit.unwrap_or(-1);
    }

    pub(crate) fn gather(&mut self, requested: f32) -> f32 {
        if self.resource_name.is_none() || !requested.is_finite() || requested <= 0.0 {
            return 0.0;
        }
        if self.unlimited {
            return requested;
        }
        let gathered = requested.min(self.amount.max(0.0));
        self.amount = (self.amount - gathered).max(0.0);
        gathered
    }

    pub(crate) fn set_amount(&mut self, amount: f32) -> bool {
        if !amount.is_finite() {
            return false;
        }
        self.amount = amount.max(0.0);
        true
    }

    pub(crate) fn is_available(&self) -> bool {
        self.resource_name.is_some() && (self.unlimited || self.amount > 0.0)
    }

    pub(crate) fn resource_name(&self) -> Option<&str> {
        self.resource_name.as_deref()
    }

    pub(crate) const fn amount(&self) -> f32 {
        self.amount
    }

    pub(crate) const fn unlimited(&self) -> bool {
        self.unlimited
    }

    pub(crate) const fn die_at_zero(&self) -> bool {
        self.die_at_zero
    }

    pub(crate) const fn gatherer_limit(&self) -> i32 {
        self.gatherer_limit
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(resource_name) = &self.resource_name {
            checksum.hash_u32(u32::try_from(resource_name.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(resource_name.as_bytes());
        } else {
            checksum.hash_u32(u32::MAX);
        }
        checksum.hash_f32(self.amount);
        checksum.hash_u32(u32::from(self.unlimited));
        checksum.hash_u32(u32::from(self.die_at_zero));
        checksum.hash_i32(self.gatherer_limit);
    }
}

impl Unit {
    /// Return the configured resource kind for this gatherable unit.
    #[must_use]
    pub fn resource_name(&self) -> Option<&str> {
        self.resource_node.resource_name()
    }

    /// Return this unit's authoritative remaining resource amount.
    #[must_use]
    pub const fn resource_amount(&self) -> f32 {
        self.resource_node.amount()
    }

    /// Replace this unit's finite resource amount.
    pub fn set_resource_amount(&mut self, amount: f32) -> bool {
        self.resource_node.set_amount(amount)
    }

    /// Return whether gathering leaves this unit's amount unchanged.
    #[must_use]
    pub const fn has_unlimited_resources(&self) -> bool {
        self.resource_node.unlimited()
    }

    /// Return the current unit gather-action phase.
    #[must_use]
    pub const fn gather_phase(&self) -> GatherPhase {
        self.gather.phase()
    }

    /// Return the unit targeted by the current gather action.
    #[must_use]
    pub const fn gather_target(&self) -> Option<EntityId> {
        self.gather.target_id()
    }

    /// Return the selected authored action name.
    #[must_use]
    pub fn gather_action_name(&self) -> Option<&str> {
        (!self.gather.action_name().is_empty()).then(|| self.gather.action_name())
    }

    /// Return whether the unit is actively gathering resources.
    #[must_use]
    pub const fn is_gathering(&self) -> bool {
        matches!(self.gather.phase(), GatherPhase::Working)
    }

    pub(crate) fn cancel_gather_action(&mut self) {
        self.gather.cancel();
    }

    pub(crate) fn hash_gather_state(&self, checksum: &mut SyncChecksum) {
        self.resource_node.hash_state(checksum);
        self.gather.hash_state(checksum);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_and_unlimited_nodes_apply_retail_depletion_rules() {
        let mut node = UnitResourceNode::default();
        node.configure(Some("Supplies".to_owned()), Some(3.0), false, true, None);
        assert_eq!(node.gather(2.0).to_bits(), 2.0_f32.to_bits());
        assert_eq!(node.gather(2.0).to_bits(), 1.0_f32.to_bits());
        assert!(!node.is_available());

        node.configure(Some("Power".to_owned()), Some(0.0), true, false, None);
        assert_eq!(node.gather(4.0).to_bits(), 4.0_f32.to_bits());
        assert_eq!(node.amount().to_bits(), 0.0_f32.to_bits());
        assert!(node.is_available());
    }
}
