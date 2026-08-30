//! Base protection contributed by completed child buildings.

use super::Unit;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct UnitChildDamageState {
    base_scalar: f32,
    contributes: bool,
    applied_multiplier: f32,
}

impl Default for UnitChildDamageState {
    fn default() -> Self {
        Self {
            base_scalar: 0.0,
            contributes: false,
            applied_multiplier: 1.0,
        }
    }
}

impl Unit {
    pub(crate) fn configure_child_damage_state(
        &mut self,
        base_scalar: Option<f32>,
        contributes: bool,
    ) {
        self.child_damage.base_scalar = base_scalar
            .filter(|scalar| scalar.is_finite() && *scalar > 0.0)
            .unwrap_or_default();
        self.child_damage.contributes = contributes;
    }

    pub(crate) const fn child_damage_base_scalar(&self) -> f32 {
        self.child_damage.base_scalar
    }

    pub(crate) const fn contributes_child_damage_protection(&self) -> bool {
        self.child_damage.contributes
    }

    pub(crate) fn set_child_object_damage_taken_multiplier(&mut self, multiplier: f32) {
        self.child_damage.applied_multiplier = if multiplier.is_finite() && multiplier > 0.0 {
            multiplier
        } else {
            1.0
        };
    }

    /// Return the base-protection multiplier contributed by completed children.
    #[must_use]
    pub const fn child_object_damage_taken_multiplier(&self) -> f32 {
        self.child_damage.applied_multiplier
    }

    pub(crate) fn hash_child_damage_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_f32(self.child_damage.base_scalar);
        checksum.hash_u32(u32::from(self.child_damage.contributes));
        checksum.hash_f32(self.child_damage.applied_multiplier);
    }
}
