//! Per-unit effects projected from the owning squad's cryo action.

use super::Unit;
use crate::entities::squads::SquadCryoEffect;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UnitCryo {
    frozen: bool,
    movement_modifier: f32,
    damage_taken_modifier: f32,
}

impl Default for UnitCryo {
    fn default() -> Self {
        Self {
            frozen: false,
            movement_modifier: 1.0,
            damage_taken_modifier: 1.0,
        }
    }
}

impl UnitCryo {
    fn apply(&mut self, effect: SquadCryoEffect) {
        self.frozen = effect.frozen;
        self.movement_modifier = effect.movement_modifier;
        self.damage_taken_modifier = effect.damage_taken_modifier;
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.frozen));
        checksum.hash_f32(self.movement_modifier);
        checksum.hash_f32(self.damage_taken_modifier);
    }
}

impl Unit {
    /// Return whether this unit is currently frozen by retail's cryo action.
    #[must_use]
    pub const fn is_cryo_frozen(&self) -> bool {
        self.cryo.frozen
    }

    /// Return the transient flag that selects a shatter-only death replacement.
    #[must_use]
    pub const fn is_shatter_on_death(&self) -> bool {
        self.cryo.frozen
    }

    pub(crate) fn apply_cryo_effect(&mut self, effect: SquadCryoEffect) {
        self.cryo.apply(effect);
    }

    pub(crate) fn clear_cryo_effect(&mut self) {
        self.cryo = UnitCryo::default();
    }

    pub(crate) const fn cryo_movement_modifier(&self) -> f32 {
        self.cryo.movement_modifier
    }

    pub(crate) const fn cryo_damage_taken_modifier(&self) -> f32 {
        self.cryo.damage_taken_modifier
    }

    pub(crate) fn hash_cryo_state(&self, checksum: &mut SyncChecksum) {
        self.cryo.hash_state(checksum);
    }
}
