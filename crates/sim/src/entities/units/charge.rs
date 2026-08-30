//! Persistent retail charged-ranged-attack state.

use super::Unit;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

pub(crate) const FLOAT_COMPARE_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitCharge {
    action_name: String,
    elapsed_seconds: f32,
    damage_charge: f32,
    reset_pending: bool,
    attachment_entity_id: Option<EntityId>,
}

impl UnitCharge {
    pub(crate) fn configure(&mut self, action_name: &str, damage_charge: f32) -> Option<EntityId> {
        if self.action_name.eq_ignore_ascii_case(action_name) {
            self.damage_charge = finite_or_zero(damage_charge);
            return None;
        }
        let attachment = self.attachment_entity_id.take();
        self.action_name.clear();
        self.action_name.push_str(action_name);
        self.elapsed_seconds = 0.0;
        self.damage_charge = finite_or_zero(damage_charge);
        self.reset_pending = false;
        attachment
    }

    /// Advance and return readiness before retail applies a queued clear.
    pub(crate) fn advance(&mut self, dt: f32, enabled: bool) -> bool {
        if enabled && dt.is_finite() && dt > 0.0 {
            self.elapsed_seconds += dt;
            let maximum = self.damage_charge + FLOAT_COMPARE_EPSILON;
            if self.elapsed_seconds > maximum {
                self.elapsed_seconds = maximum;
            }
        }
        let ready = self.is_ready();
        if self.reset_pending {
            self.elapsed_seconds = 0.0;
            self.reset_pending = false;
        }
        ready
    }

    pub(crate) fn request_clear(&mut self) {
        self.reset_pending = true;
    }

    pub(crate) fn reset(&mut self) -> Option<EntityId> {
        let attachment = self.attachment_entity_id.take();
        *self = Self::default();
        attachment
    }

    pub(crate) fn is_ready(&self) -> bool {
        !self.action_name.is_empty()
            && self.elapsed_seconds >= self.damage_charge
            && self.damage_charge >= 0.0
    }

    pub(crate) const fn attachment_entity_id(&self) -> Option<EntityId> {
        self.attachment_entity_id
    }

    pub(crate) fn set_attachment_entity_id(&mut self, attachment: Option<EntityId>) {
        self.attachment_entity_id = attachment;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_f32(self.damage_charge);
        checksum.hash_u32(u32::from(self.reset_pending));
        checksum.hash_u32(
            self.attachment_entity_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
    }
}

impl Unit {
    /// Return the persistent Charge action currently tracked by this unit.
    #[must_use]
    pub fn charge_action_name(&self) -> Option<&str> {
        (!self.charge.action_name.is_empty()).then_some(self.charge.action_name.as_str())
    }

    /// Return authoritative elapsed Charge time in seconds.
    #[must_use]
    pub const fn charge_seconds(&self) -> f32 {
        self.charge.elapsed_seconds
    }

    /// Return whether the tracked Charge action can currently pull a squad.
    #[must_use]
    pub(crate) fn is_charge_ready(&self) -> bool {
        self.charge.is_ready()
    }

    /// Return the class-zero ready-state effect projected from this unit.
    #[must_use]
    pub const fn charge_effect_entity_id(&self) -> Option<EntityId> {
        self.charge.attachment_entity_id
    }

    pub(crate) fn clear_charge_after_pull(&mut self) {
        self.charge.request_clear();
    }

    pub(crate) fn hash_charge_state(&self, checksum: &mut SyncChecksum) {
        self.charge.hash_state(checksum);
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_clear_occurs_after_one_ready_update() {
        let mut charge = UnitCharge::default();
        assert_eq!(charge.configure("Charge", 1.0), None);
        assert!(charge.advance(1.0, true));
        charge.request_clear();

        assert!(charge.advance(0.05, true));
        assert!(charge.elapsed_seconds.abs() < f32::EPSILON);
        assert!(!charge.advance(0.05, true));
        assert!((charge.elapsed_seconds - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn disabled_actions_preserve_existing_charge() {
        let mut charge = UnitCharge::default();
        assert_eq!(charge.configure("Charge", 1.0), None);
        assert!(!charge.advance(0.5, true));
        assert!(!charge.advance(2.0, false));
        assert!((charge.elapsed_seconds - 0.5).abs() < f32::EPSILON);
    }
}
