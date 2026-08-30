//! Per-unit enablement overrides for authored tactic actions.

use super::Unit;
use crate::sync::SyncChecksum;
use std::collections::BTreeMap;

/// Live action enablement changed by techs, scripts, and abilities.
///
/// An absent override uses the tactic action's `StartDisabled` value. Keys are
/// normalized so authored name comparisons remain case-insensitive.
#[derive(Debug, Clone, Default)]
pub struct UnitActions {
    enabled_overrides: BTreeMap<String, bool>,
}

impl UnitActions {
    /// Override whether one authored action is enabled.
    pub fn set_enabled(&mut self, action_name: &str, enabled: bool) {
        self.enabled_overrides
            .insert(action_name.to_ascii_lowercase(), enabled);
    }

    /// Remove a live override and return to the authored initial state.
    pub fn clear_override(&mut self, action_name: &str) -> bool {
        self.enabled_overrides
            .remove(&action_name.to_ascii_lowercase())
            .is_some()
    }

    /// Resolve live enablement against the authored initial state.
    #[must_use]
    pub fn is_enabled(&self, action_name: &str, starts_disabled: bool) -> bool {
        self.enabled_overrides
            .get(&action_name.to_ascii_lowercase())
            .copied()
            .unwrap_or(!starts_disabled)
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.enabled_overrides.len()).unwrap_or(u32::MAX));
        for (name, &enabled) in &self.enabled_overrides {
            checksum.hash_u32(u32::try_from(name.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(name.as_bytes());
            checksum.hash_u32(u32::from(enabled));
        }
    }
}

impl Unit {
    /// Return the stable logical prototype retained across technology transforms.
    #[must_use]
    pub fn logical_proto_object_name(&self) -> &str {
        if self.logical_proto_object_name.is_empty() {
            &self.proto_object_name
        } else {
            &self.logical_proto_object_name
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_authored_initial_state_case_insensitively() {
        let mut actions = UnitActions::default();
        assert!(!actions.is_enabled("RocketAttack", true));
        actions.set_enabled("ROCKETATTACK", true);
        assert!(actions.is_enabled("rocketattack", true));
        assert!(actions.clear_override("RocketAttack"));
        assert!(!actions.is_enabled("rocketattack", true));
    }
}
