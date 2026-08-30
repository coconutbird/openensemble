//! Persistent identity for retail static death replacements.

use super::{Unit, UnitKind};
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
enum StaticDeathReplacementPhase {
    #[default]
    Inactive = 0,
    Retained = 1,
    Healing = 2,
}

/// State that keeps an in-place death replacement in the unit pool.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UnitStaticDeathReplacement {
    phase: StaticDeathReplacementPhase,
    invulnerable_when_healed: bool,
}

impl UnitStaticDeathReplacement {
    fn retain(&mut self, healing: bool, invulnerable_when_healed: bool) {
        self.phase = if healing {
            StaticDeathReplacementPhase::Healing
        } else {
            StaticDeathReplacementPhase::Retained
        };
        self.invulnerable_when_healed = invulnerable_when_healed;
    }

    const fn is_retained(self) -> bool {
        !matches!(self.phase, StaticDeathReplacementPhase::Inactive)
    }

    const fn is_healing(self) -> bool {
        matches!(self.phase, StaticDeathReplacementPhase::Healing)
    }

    fn finish_healing(&mut self) -> Option<bool> {
        if !self.is_healing() {
            return None;
        }
        self.phase = StaticDeathReplacementPhase::Retained;
        Some(self.invulnerable_when_healed)
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(u32::from(self.invulnerable_when_healed));
    }
}

impl Unit {
    /// Return whether retail retained this dead entity as its static replacement.
    #[must_use]
    pub const fn is_static_death_replacement(&self) -> bool {
        self.static_death_replacement.is_retained()
    }

    /// Return whether this damaged replacement still requires repair.
    #[must_use]
    pub const fn is_death_replacement_healing(&self) -> bool {
        self.static_death_replacement.is_healing()
    }

    pub(crate) fn reset_for_prototype_transform(&mut self, target_kind: UnitKind, now_ms: u32) {
        let mut replacement = match target_kind {
            UnitKind::Mobile => Self::new(self.base.id, self.base.player_id),
            UnitKind::Building => Self::new_building(self.base.id, self.base.player_id),
        };
        replacement.base = self.base.clone();
        replacement.object_state = self.object_state.clone();
        replacement.copy_surviving_relationships(self);
        replacement.copy_runtime_scalars(self);
        replacement.cryo = self.cryo;
        replacement
            .object_state
            .notify_prototype_transformed(now_ms);
        *self = replacement;
    }

    fn copy_surviving_relationships(&mut self, source: &Self) {
        self.built = source.built;
        self.authored_children.clone_from(&source.authored_children);
        self.built_by = source.built_by;
        self.build_socket_id = source.build_socket_id;
        self.build_socket_index = source.build_socket_index;
        self.socket_plug_id = source.socket_plug_id;
        self.socket_parent_id = source.socket_parent_id;
        self.associated_socket_ids
            .clone_from(&source.associated_socket_ids);
        self.associated_parking_lot_id = source.associated_parking_lot_id;
        self.socket_local_offset = source.socket_local_offset;
        self.socket_local_yaw_degrees = source.socket_local_yaw_degrees;
        self.population_costs.clone_from(&source.population_costs);
        self.population_cap_additions
            .clone_from(&source.population_cap_additions);
        self.trained_by = source.trained_by;
        self.train_limit_bucket = source.train_limit_bucket;
        self.squad_id = source.squad_id;
        self.base_id = source.base_id;
        self.formation_offset = source.formation_offset;
    }

    fn copy_runtime_scalars(&mut self, source: &Self) {
        self.damage_multiplier = source.damage_multiplier;
        self.damage_taken_multiplier = source.damage_taken_multiplier;
        self.join_damage_multiplier = source.join_damage_multiplier;
        self.join_damage_taken_multiplier = source.join_damage_taken_multiplier;
        self.accuracy_scalar = source.accuracy_scalar;
        self.dodge_scalar = source.dodge_scalar;
        self.work_rate_scalar = source.work_rate_scalar;
        self.line_of_sight_scalar = source.line_of_sight_scalar;
        self.velocity_scalar = source.velocity_scalar;
        self.weapon_range_scalar = source.weapon_range_scalar;
    }

    pub(crate) fn retain_static_death_replacement(
        &mut self,
        healing: bool,
        invulnerable_when_healed: bool,
    ) {
        self.static_death_replacement
            .retain(healing, invulnerable_when_healed);
        if healing {
            self.hitpoints = 1.0_f32.min(self.max_hitpoints);
            self.set_invulnerable(true);
        } else {
            self.hitpoints = self.max_hitpoints;
        }
        self.shields.set_current(self.shields.maximum);
    }

    pub(crate) fn finish_death_replacement_healing(&mut self) -> bool {
        if self.hitpoints < self.max_hitpoints {
            return false;
        }
        let Some(invulnerable) = self.static_death_replacement.finish_healing() else {
            return false;
        };
        self.set_invulnerable(invulnerable);
        true
    }

    pub(crate) fn hash_static_death_replacement_state(&self, checksum: &mut SyncChecksum) {
        self.static_death_replacement.hash_state(checksum);
    }
}
