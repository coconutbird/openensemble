//! Deterministic per-unit ranged-attack animation state.

use crate::entity_id::EntityId;
use crate::gameplay::AttackProfile;
use crate::random::Random;

/// Runtime state that turns authored visual Attack tags into simulation hits.
#[derive(Debug, Clone)]
pub struct UnitCombat {
    action_name: String,
    engaged_target: Option<EntityId>,
    animation_index: Option<usize>,
    cycle_elapsed: f32,
    pre_attack_duration: f32,
    cycle_duration: f32,
    next_attack_tag: usize,
    reload_remaining: f32,
    visual_ammo_remaining: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AttackAdvance {
    pub hit_count: u32,
    pub completed_cycles: u32,
}

impl Default for UnitCombat {
    fn default() -> Self {
        Self {
            action_name: String::new(),
            engaged_target: None,
            animation_index: None,
            cycle_elapsed: 0.0,
            pre_attack_duration: 0.0,
            cycle_duration: 0.0,
            next_attack_tag: 0,
            reload_remaining: 0.0,
            visual_ammo_remaining: 0,
        }
    }
}

impl UnitCombat {
    /// Return the currently selected tactic action.
    #[must_use]
    pub fn action_name(&self) -> Option<&str> {
        (!self.action_name.is_empty()).then_some(self.action_name.as_str())
    }

    /// Return the concrete target selected for this unit's current engagement.
    #[must_use]
    pub const fn engaged_target(&self) -> Option<EntityId> {
        self.engaged_target
    }

    /// Return whether an attack animation cycle is active.
    #[must_use]
    pub const fn is_animating(&self) -> bool {
        self.animation_index.is_some()
    }

    /// Clear action, cooldown, reload, and visual-ammo state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn stop_firing(&mut self) {
        self.engaged_target = None;
        self.animation_index = None;
        self.cycle_elapsed = 0.0;
        self.pre_attack_duration = 0.0;
        self.cycle_duration = 0.0;
        self.next_attack_tag = 0;
    }

    pub(crate) fn advance(
        &mut self,
        dt: f32,
        target: EntityId,
        profile: &AttackProfile,
        rng: &mut Random,
    ) -> AttackAdvance {
        if !dt.is_finite() || dt <= 0.0 || profile.animations.is_empty() {
            return AttackAdvance::default();
        }
        self.select_action(profile, target);

        let mut remaining = dt;
        let mut hits = 0_u32;
        let mut completed_cycles = 0_u32;
        let mut transitions = 0_u32;
        while remaining > 0.0 && transitions < 128 {
            transitions += 1;
            if self.reload_remaining > 0.0 {
                let elapsed = remaining.min(self.reload_remaining);
                self.reload_remaining -= elapsed;
                remaining -= elapsed;
                if self.reload_remaining <= f32::EPSILON {
                    self.reload_remaining = 0.0;
                    self.visual_ammo_remaining = profile.visual_ammo;
                }
                continue;
            }
            if self.animation_index.is_none() {
                self.begin_cycle(profile, rng);
            }

            let animation_index = self
                .animation_index
                .expect("begin_cycle selects an animation");
            let animation = &profile.animations[animation_index];
            let next_tag_time = animation
                .attack_positions
                .get(self.next_attack_tag)
                .map_or(f32::INFINITY, |position| {
                    self.pre_attack_duration + position * animation.duration
                });
            let boundary = next_tag_time.min(self.cycle_duration);
            let until_boundary = (boundary - self.cycle_elapsed).max(0.0);
            if until_boundary > remaining {
                self.cycle_elapsed += remaining;
                remaining = 0.0;
                continue;
            }

            self.cycle_elapsed += until_boundary;
            remaining -= until_boundary;
            if next_tag_time <= self.cycle_duration
                && next_tag_time <= self.cycle_elapsed + f32::EPSILON
            {
                self.next_attack_tag += 1;
                hits += 1;
                if profile.visual_ammo > 0 {
                    self.visual_ammo_remaining = self.visual_ammo_remaining.saturating_sub(1);
                }
                continue;
            }

            self.finish_cycle(profile);
            completed_cycles += 1;
        }
        AttackAdvance {
            hit_count: hits,
            completed_cycles,
        }
    }

    fn select_action(&mut self, profile: &AttackProfile, target: EntityId) {
        if !self.action_name.eq_ignore_ascii_case(&profile.action_name) {
            self.reset();
            self.action_name.clone_from(&profile.action_name);
            self.visual_ammo_remaining = profile.visual_ammo;
        }
        self.engaged_target = Some(target);
    }

    fn begin_cycle(&mut self, profile: &AttackProfile, rng: &mut Random) {
        let animation_index = choose_animation(profile, rng);
        let animation = &profile.animations[animation_index];
        let pre_attack_duration = roll_cooldown(profile.pre_attack_cooldown, rng);
        let post_attack_duration = roll_cooldown(profile.post_attack_cooldown, rng);
        self.animation_index = Some(animation_index);
        self.cycle_elapsed = 0.0;
        self.pre_attack_duration = pre_attack_duration;
        self.cycle_duration = pre_attack_duration + animation.duration + post_attack_duration;
        self.next_attack_tag = 0;
    }

    fn finish_cycle(&mut self, profile: &AttackProfile) {
        self.animation_index = None;
        self.cycle_elapsed = 0.0;
        self.pre_attack_duration = 0.0;
        self.cycle_duration = 0.0;
        self.next_attack_tag = 0;
        if profile.visual_ammo > 0
            && self.visual_ammo_remaining == 0
            && profile.reload_duration > 0.0
        {
            self.reload_remaining = profile.reload_duration;
        }
    }

    pub(crate) fn hash_state(&self, checksum: &mut crate::sync::SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
        checksum.hash_u32(
            self.engaged_target
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(
            self.animation_index
                .and_then(|index| u32::try_from(index).ok())
                .unwrap_or(u32::MAX),
        );
        checksum.hash_f32(self.cycle_elapsed);
        checksum.hash_f32(self.pre_attack_duration);
        checksum.hash_f32(self.cycle_duration);
        checksum.hash_u32(u32::try_from(self.next_attack_tag).unwrap_or(u32::MAX));
        checksum.hash_f32(self.reload_remaining);
        checksum.hash_u32(self.visual_ammo_remaining);
    }
}

fn choose_animation(profile: &AttackProfile, rng: &mut Random) -> usize {
    if profile.animations.len() <= 1 {
        return 0;
    }
    let total = profile
        .animations
        .iter()
        .map(|animation| animation.weight.max(0))
        .sum::<i32>();
    if total <= 0 {
        return 0;
    }
    let roll = rng.i_rand(0, total);
    let mut accumulated = 0_i32;
    for (index, animation) in profile.animations.iter().enumerate() {
        accumulated += animation.weight.max(0);
        if accumulated > roll {
            return index;
        }
    }
    0
}

fn roll_cooldown(range: [f32; 2], rng: &mut Random) -> f32 {
    if range[1] <= 0.0 {
        0.0
    } else {
        rng.f_rand(range[0], range[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::{AttackAccuracyProfile, AttackAnimation};

    fn profile() -> AttackProfile {
        AttackProfile {
            action_name: "RifleAttack".to_owned(),
            weapon_name: "Rifle".to_owned(),
            weapon_type: None,
            projectile: None,
            area_damage: None,
            friendly_fire: false,
            targets_foot_of_unit: false,
            max_range: 25.0,
            max_velocity_lead: 0.0,
            accuracy: AttackAccuracyProfile::default(),
            damage_per_attack: 5.0,
            animations: vec![AttackAnimation {
                asset_path: "attack.uax".to_owned(),
                weight: 1,
                duration: 1.0,
                attack_positions: vec![0.25, 0.75],
            }],
            pre_attack_cooldown: [0.5, 0.5],
            post_attack_cooldown: [0.5, 0.5],
            reload_duration: 1.0,
            visual_ammo: 2,
            uses_height_bonus_damage: false,
        }
    }

    #[test]
    fn attack_tags_fire_at_authored_positions_and_then_reload() {
        let mut combat = UnitCombat::default();
        let mut rng = Random::new();
        let target = EntityId::new(crate::entity_id::EntityClass::Unit, 2);
        let profile = profile();

        assert_eq!(
            combat.advance(0.74, target, &profile, &mut rng).hit_count,
            0
        );
        assert_eq!(
            combat.advance(0.02, target, &profile, &mut rng).hit_count,
            1
        );
        assert_eq!(
            combat.advance(0.48, target, &profile, &mut rng).hit_count,
            0
        );
        assert_eq!(
            combat.advance(0.02, target, &profile, &mut rng).hit_count,
            1
        );
        assert_eq!(
            combat.advance(0.74, target, &profile, &mut rng).hit_count,
            0
        );
        assert_eq!(
            combat.advance(0.99, target, &profile, &mut rng).hit_count,
            0
        );
        assert_eq!(
            combat.advance(0.02, target, &profile, &mut rng).hit_count,
            0
        );
        assert_eq!(
            combat.advance(0.74, target, &profile, &mut rng).hit_count,
            1
        );
    }
}
