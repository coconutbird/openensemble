//! Deterministic state for retail's shared ranged/hand attack executor.

mod hardpoints;

use crate::entities::UnitAmmunition;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AttackAnimation, AttackAnimationAnchor, AttackAnimationEvent, AttackAnimationEventKind,
    AttackProfile,
};
use crate::random::Random;
use glam::{Mat4, Vec3};

pub(crate) use hardpoints::HardpointAim;

/// Runtime state that turns authored visual Attack tags into simulation hits.
#[derive(Debug, Clone)]
pub struct UnitCombat {
    action_name: String,
    engaged_target: Option<EntityId>,
    engaged_position: Option<Vec3>,
    animation_index: Option<usize>,
    charged_cycle_requested: bool,
    cycle_uses_charged_animation: bool,
    cycle_elapsed: f32,
    pre_attack_duration: f32,
    cycle_duration: f32,
    next_attack_tag: usize,
    reload_remaining: f32,
    visual_ammo_remaining: u32,
    hardpoint_force_body_facing: bool,
    hardpoints: hardpoints::UnitHardpointState,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AttackAdvance {
    pub hit_count: u32,
    pub completed_cycles: u32,
    pub events: Vec<AttackEventOccurrence>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AttackEventOccurrence {
    pub animation_index: usize,
    pub charged: bool,
    pub event: AttackAnimationEvent,
}

impl Default for UnitCombat {
    fn default() -> Self {
        Self {
            action_name: String::new(),
            engaged_target: None,
            engaged_position: None,
            animation_index: None,
            charged_cycle_requested: false,
            cycle_uses_charged_animation: false,
            cycle_elapsed: 0.0,
            pre_attack_duration: 0.0,
            cycle_duration: 0.0,
            next_attack_tag: 0,
            reload_remaining: 0.0,
            visual_ammo_remaining: 0,
            hardpoint_force_body_facing: false,
            hardpoints: hardpoints::UnitHardpointState::default(),
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

    /// Return the ground position selected for a position-target attack.
    #[must_use]
    pub const fn engaged_position(&self) -> Option<Vec3> {
        self.engaged_position
    }

    /// Return whether an attack animation cycle is active.
    #[must_use]
    pub const fn is_animating(&self) -> bool {
        self.animation_index.is_some()
    }

    /// Return the selected visual-animation variant while an attack cycle exists.
    #[must_use]
    pub const fn animation_index(&self) -> Option<usize> {
        self.animation_index
    }

    /// Return whether the active cycle uses a persistent `Charge` animation.
    #[must_use]
    pub const fn uses_charged_animation(&self) -> bool {
        self.animation_index.is_some() && self.cycle_uses_charged_animation
    }

    /// Project the active attack clip's normalized position.
    ///
    /// Retail starts the action animation only after the pre-attack cooldown and
    /// returns to non-action presentation while the post-attack cooldown runs.
    #[must_use]
    pub fn animation_position(&self, duration_seconds: f32) -> Option<f32> {
        self.animation_index?;
        if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
            return None;
        }
        let elapsed = self.cycle_elapsed - self.pre_attack_duration;
        if !elapsed.is_finite() || elapsed < 0.0 || elapsed >= duration_seconds {
            return None;
        }
        Some((elapsed / duration_seconds).clamp(0.0, 1.0))
    }

    /// Return whether the current action is waiting on its visual reload.
    #[must_use]
    pub fn is_reloading(&self) -> bool {
        self.reload_remaining > 0.0
    }

    /// Return the live simulation transform for an attached hardpoint component.
    #[must_use]
    pub fn hardpoint_attachment_transform(&self, name: &str) -> Option<Mat4> {
        self.hardpoints.attachment_transform(name)
    }

    /// Iterate live single-bone hardpoint transforms by authored bone name.
    pub fn hardpoint_bone_transforms(&self) -> impl Iterator<Item = (&str, Mat4)> {
        self.hardpoints.bone_transforms()
    }

    /// Return the live simulation transform for one single-bone hardpoint node.
    #[must_use]
    pub fn hardpoint_bone_transform(&self, name: &str) -> Option<Mat4> {
        self.hardpoints.bone_transform(name)
    }

    /// Clear action, cooldown, reload, and visual-ammo state.
    pub fn reset(&mut self) {
        let mut hardpoints = std::mem::take(&mut self.hardpoints);
        hardpoints.release_active();
        *self = Self {
            hardpoints,
            ..Self::default()
        };
    }

    pub(crate) fn stop_firing(&mut self) {
        self.engaged_target = None;
        self.engaged_position = None;
        self.animation_index = None;
        self.charged_cycle_requested = false;
        self.cycle_uses_charged_animation = false;
        self.cycle_elapsed = 0.0;
        self.pre_attack_duration = 0.0;
        self.cycle_duration = 0.0;
        self.next_attack_tag = 0;
        self.hardpoint_force_body_facing = false;
        self.hardpoints.release_active();
    }

    pub(crate) fn orient_hardpoint(
        &mut self,
        profile: &AttackProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
        elapsed: f32,
    ) -> Option<HardpointAim> {
        self.ensure_action(profile);
        profile.hardpoint.as_ref().map(|hardpoint| {
            self.hardpoints
                .orient(hardpoint, anchor, unit_world, target_world, elapsed)
        })
    }

    pub(crate) fn is_hardpoint_oriented(
        &self,
        profile: &AttackProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
        tolerance: f32,
    ) -> bool {
        profile.hardpoint.as_ref().is_none_or(|hardpoint| {
            self.hardpoints
                .is_oriented(hardpoint, anchor, unit_world, target_world, tolerance)
        })
    }

    pub(crate) fn hardpoint_yaw_origin(
        &self,
        profile: &AttackProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
    ) -> Option<Vec3> {
        self.hardpoints
            .yaw_origin(profile.hardpoint.as_ref()?, anchor, unit_world)
    }

    pub(crate) fn advance_hardpoint_auto_center(&mut self, elapsed: f32) {
        self.hardpoints.advance_auto_center(elapsed);
    }

    pub(crate) fn hardpoint_yaw_target_world(
        &self,
        profile: &AttackProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
    ) -> Option<Vec3> {
        self.hardpoints
            .yaw_target_world(profile.hardpoint.as_ref()?, anchor, unit_world)
    }

    pub(crate) fn preserve_hardpoint_yaw_target(
        &mut self,
        profile: &AttackProfile,
        anchor: Option<&AttackAnimationAnchor>,
        unit_world: Mat4,
        target_world: Vec3,
    ) {
        if let Some(hardpoint) = &profile.hardpoint {
            self.hardpoints
                .preserve_yaw_target(hardpoint, anchor, unit_world, target_world);
        }
    }

    pub(crate) fn hardpoint_yaw_before_owner_motion(&self, unit_world: Mat4) -> Option<Vec3> {
        self.engaged_target?;
        self.hardpoints.active_yaw_target_world(unit_world)
    }

    pub(crate) fn restore_hardpoint_yaw_after_owner_motion(
        &mut self,
        unit_world: Mat4,
        target_world: Vec3,
    ) {
        if self.engaged_target.is_some() {
            self.hardpoints
                .restore_active_yaw_target(unit_world, target_world);
        }
    }

    pub(crate) const fn hardpoint_forces_body_facing(&self) -> bool {
        self.hardpoint_force_body_facing
    }

    pub(crate) fn set_hardpoint_force_body_facing(&mut self, force: bool) {
        self.hardpoint_force_body_facing = force;
    }

    pub(crate) fn hardpoint_anchor(
        &self,
        profile: &AttackProfile,
    ) -> Option<AttackAnimationAnchor> {
        let charged = self.animation_index.map_or_else(
            || self.charged_cycle_requested && profile.charged_animation.is_some(),
            |_| self.cycle_uses_charged_animation,
        );
        let animations = profile.cycle_animations(charged);
        let animation = self
            .animation_index
            .and_then(|index| animations.get(index))
            .or_else(|| animations.first())?;
        self.animation_position(animation.duration)
            .and_then(|position| animation.hardpoint_anchor_at(position))
            .or_else(|| {
                animation
                    .events
                    .iter()
                    .find_map(|event| event.anchor.clone())
            })
    }

    pub(crate) fn request_charged_cycle(&mut self, requested: bool) {
        self.charged_cycle_requested = requested;
    }

    pub(crate) fn prepare_target(&mut self, profile: &AttackProfile, target: EntityId) {
        self.select_action(profile, target);
    }

    pub(crate) fn prepare_position(&mut self, profile: &AttackProfile, target: Vec3) {
        self.select_action(profile, EntityId::INVALID);
        self.engaged_target = None;
        self.engaged_position = Some(target);
    }

    pub(crate) fn advance(
        &mut self,
        dt: f32,
        target: EntityId,
        profile: &AttackProfile,
        ammunition: &mut UnitAmmunition,
        ammunition_per_attack: f32,
        rng: &mut Random,
    ) -> AttackAdvance {
        if !dt.is_finite() || dt <= 0.0 || profile.animations.is_empty() {
            return AttackAdvance::default();
        }
        self.select_action(profile, target);

        let mut remaining = dt;
        let mut hits = 0_u32;
        let mut completed_cycles = 0_u32;
        let mut events = Vec::new();
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
                if profile.ammunition.is_used()
                    && !ammunition.has_full_attack(
                        profile.maximum_attacks_per_animation(),
                        ammunition_per_attack,
                    )
                {
                    break;
                }
                self.begin_cycle(profile, rng);
            }

            let animation_index = self
                .animation_index
                .expect("begin_cycle selects an animation");
            let animation =
                &profile.cycle_animations(self.cycle_uses_charged_animation)[animation_index];
            let next_tag_time = timeline_event(animation, self.next_attack_tag)
                .map(|event| event.position)
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
                let event = timeline_event(animation, self.next_attack_tag)
                    .expect("finite event boundary resolves its timeline event");
                self.next_attack_tag += 1;
                if matches!(event.kind, AttackAnimationEventKind::Attack { .. }) {
                    hits += 1;
                    if profile.ammunition.is_used() {
                        ammunition.spend_attack(ammunition_per_attack);
                    }
                    if profile.visual_ammo > 0 {
                        self.visual_ammo_remaining = self.visual_ammo_remaining.saturating_sub(1);
                    }
                }
                events.push(AttackEventOccurrence {
                    animation_index,
                    charged: self.cycle_uses_charged_animation,
                    event,
                });
                continue;
            }

            self.finish_cycle(profile);
            completed_cycles += 1;
        }
        AttackAdvance {
            hit_count: hits,
            completed_cycles,
            events,
        }
    }

    pub(crate) fn advance_position(
        &mut self,
        dt: f32,
        target: Vec3,
        profile: &AttackProfile,
        ammunition: &mut UnitAmmunition,
        ammunition_per_attack: f32,
        rng: &mut Random,
    ) -> AttackAdvance {
        let advance = self.advance(
            dt,
            EntityId::INVALID,
            profile,
            ammunition,
            ammunition_per_attack,
            rng,
        );
        self.engaged_target = None;
        self.engaged_position = Some(target);
        advance
    }

    fn select_action(&mut self, profile: &AttackProfile, target: EntityId) {
        self.ensure_action(profile);
        self.engaged_target = Some(target);
        self.engaged_position = None;
    }

    fn ensure_action(&mut self, profile: &AttackProfile) {
        if !self.action_name.eq_ignore_ascii_case(&profile.action_name) {
            let charged_cycle_requested = self.charged_cycle_requested;
            self.reset();
            self.charged_cycle_requested = charged_cycle_requested;
            self.action_name.clone_from(&profile.action_name);
            self.visual_ammo_remaining = profile.visual_ammo;
        }
    }

    fn begin_cycle(&mut self, profile: &AttackProfile, rng: &mut Random) {
        self.cycle_uses_charged_animation =
            self.charged_cycle_requested && profile.charged_animation.is_some();
        let animations = profile.cycle_animations(self.cycle_uses_charged_animation);
        let animation_index = choose_animation(animations, rng);
        let animation = &animations[animation_index];
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
        self.cycle_uses_charged_animation = false;
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
        if let Some(position) = self.engaged_position {
            checksum.hash_u32(1);
            checksum.hash_vec3(position.x, position.y, position.z);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(
            self.animation_index
                .and_then(|index| u32::try_from(index).ok())
                .unwrap_or(u32::MAX),
        );
        checksum.hash_u32(u32::from(self.charged_cycle_requested));
        checksum.hash_u32(u32::from(self.cycle_uses_charged_animation));
        checksum.hash_f32(self.cycle_elapsed);
        checksum.hash_f32(self.pre_attack_duration);
        checksum.hash_f32(self.cycle_duration);
        checksum.hash_u32(u32::try_from(self.next_attack_tag).unwrap_or(u32::MAX));
        checksum.hash_f32(self.reload_remaining);
        checksum.hash_u32(self.visual_ammo_remaining);
        checksum.hash_u32(u32::from(self.hardpoint_force_body_facing));
        self.hardpoints.hash_state(checksum);
    }
}

fn timeline_event(animation: &AttackAnimation, index: usize) -> Option<AttackAnimationEvent> {
    if animation.events.is_empty() {
        return animation
            .attack_positions
            .get(index)
            .copied()
            .map(|position| AttackAnimationEvent {
                position,
                kind: AttackAnimationEventKind::Attack { to_bone: None },
                anchor: None,
            });
    }
    animation.events.get(index).cloned()
}

fn choose_animation(animations: &[AttackAnimation], rng: &mut Random) -> usize {
    if animations.len() <= 1 {
        return 0;
    }
    let total = animations
        .iter()
        .map(|animation| animation.weight.max(0))
        .sum::<i32>();
    if total <= 0 {
        return 0;
    }
    let roll = rng.i_rand(0, total);
    let mut accumulated = 0_i32;
    for (index, animation) in animations.iter().enumerate() {
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
    use crate::gameplay::{AttackAccuracyProfile, AttackAnimation, ChargedAttackAnimation};

    fn profile() -> AttackProfile {
        AttackProfile {
            action_name: "RifleAttack".to_owned(),
            animation_type: "Attack".to_owned(),
            weapon_name: "Rifle".to_owned(),
            weapon_type: None,
            projectile: None,
            impact_effect: None,
            area_damage: None,
            pull: None,
            hardpoint: None,
            orientation: crate::gameplay::AttackOrientationProfile::default(),
            charged_animation: None,
            friendly_fire: false,
            targets_foot_of_unit: false,
            projectile_reactions: crate::gameplay::ProjectileReactionFlags::default(),
            max_range: 25.0,
            max_velocity_lead: 0.0,
            accuracy: AttackAccuracyProfile::default(),
            damage_per_attack: 5.0,
            ammunition: crate::gameplay::AttackAmmunition::None,
            animations: vec![AttackAnimation {
                asset_path: "attack.uax".to_owned(),
                weight: 1,
                duration: 1.0,
                attack_positions: vec![0.25, 0.75],
                events: Vec::new(),
                hardpoint_track: None,
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
        let mut ammunition = UnitAmmunition::default();

        assert_eq!(
            combat
                .advance(0.74, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            0
        );
        assert_eq!(
            combat
                .advance(0.02, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            1
        );
        assert_eq!(
            combat
                .advance(0.48, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            0
        );
        assert_eq!(
            combat
                .advance(0.02, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            1
        );
        assert_eq!(
            combat
                .advance(0.74, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            0
        );
        assert_eq!(
            combat
                .advance(0.99, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            0
        );
        assert_eq!(
            combat
                .advance(0.02, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            0
        );
        assert_eq!(
            combat
                .advance(0.74, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            1
        );
    }

    #[test]
    fn non_attack_events_keep_their_own_authored_time_and_do_not_deal_damage() {
        let mut profile = profile();
        profile.pre_attack_cooldown = [0.0, 0.0];
        profile.post_attack_cooldown = [0.0, 0.0];
        profile.animations[0].events = vec![
            AttackAnimationEvent {
                position: 0.01,
                kind: AttackAnimationEventKind::Attack {
                    to_bone: Some("launch".to_owned()),
                },
                anchor: None,
            },
            AttackAnimationEvent {
                position: 0.08,
                kind: AttackAnimationEventKind::PhysicsImpulse(
                    crate::gameplay::PhysicsImpulseEvent {
                        to_bone: Some("launch".to_owned()),
                        impulse_type: 1,
                        force: [-4.0, -3.0, 0.0],
                        attached_to_object: false,
                    },
                ),
                anchor: None,
            },
        ];
        let target = EntityId::new(crate::entity_id::EntityClass::Unit, 2);
        let mut combat = UnitCombat::default();
        let mut ammunition = UnitAmmunition::default();
        let mut rng = Random::new();

        let attack = combat.advance(0.02, target, &profile, &mut ammunition, 0.0, &mut rng);
        assert_eq!(attack.hit_count, 1);
        assert_eq!(attack.events.len(), 1);
        assert!(matches!(
            attack.events[0].event.kind,
            AttackAnimationEventKind::Attack { .. }
        ));

        let recoil = combat.advance(0.06, target, &profile, &mut ammunition, 0.0, &mut rng);
        assert_eq!(recoil.hit_count, 0);
        assert_eq!(recoil.events.len(), 1);
        assert!(matches!(
            recoil.events[0].event.kind,
            AttackAnimationEventKind::PhysicsImpulse(_)
        ));
    }

    #[test]
    fn ammunition_requires_and_spends_a_complete_authored_volley() {
        let mut profile = profile();
        profile.ammunition = crate::gameplay::AttackAmmunition::FailWhenDepleted;
        let target = EntityId::new(crate::entity_id::EntityClass::Unit, 2);
        let mut rng = Random::new();
        let mut combat = UnitCombat::default();
        let mut ammunition = UnitAmmunition::default();
        ammunition.configure(10.0, 0.0, true);

        let advance = combat.advance(1.26, target, &profile, &mut ammunition, 5.0, &mut rng);
        assert_eq!(advance.hit_count, 2);
        assert_close(ammunition.current(), 0.0);
        assert_eq!(
            combat
                .advance(10.0, target, &profile, &mut ammunition, 5.0, &mut rng)
                .hit_count,
            0
        );

        let mut insufficient = UnitAmmunition::default();
        insufficient.configure(9.0, 0.0, true);
        let mut combat = UnitCombat::default();
        assert_eq!(
            combat
                .advance(10.0, target, &profile, &mut insufficient, 5.0, &mut rng)
                .hit_count,
            0
        );
        assert!(!combat.is_animating());
    }

    #[test]
    fn presentation_position_excludes_pre_and_post_attack_cooldowns() {
        let profile = profile();
        let target = EntityId::new(crate::entity_id::EntityClass::Unit, 2);
        let mut combat = UnitCombat::default();
        let mut ammunition = UnitAmmunition::default();
        let mut rng = Random::new();

        combat.advance(0.25, target, &profile, &mut ammunition, 0.0, &mut rng);
        assert_eq!(combat.animation_index(), Some(0));
        assert_eq!(combat.animation_position(1.0), None);

        combat.advance(0.5, target, &profile, &mut ammunition, 0.0, &mut rng);
        assert_close(combat.animation_position(1.0).unwrap(), 0.25);

        combat.advance(0.75, target, &profile, &mut ammunition, 0.0, &mut rng);
        assert_eq!(combat.animation_position(1.0), None);
        assert!(combat.is_animating());
        assert_eq!(combat.animation_position(0.0), None);
    }

    #[test]
    fn position_targets_are_explicit_renderer_facing_state() {
        let profile = profile();
        let target = Vec3::new(4.0, 5.0, 6.0);
        let entity_target = EntityId::new(crate::entity_id::EntityClass::Unit, 2);
        let mut combat = UnitCombat::default();
        let mut ammunition = UnitAmmunition::default();
        let mut rng = Random::new();

        combat.advance_position(0.1, target, &profile, &mut ammunition, 0.0, &mut rng);
        assert_eq!(combat.engaged_position(), Some(target));
        assert_eq!(combat.engaged_target(), None);

        combat.advance(0.1, entity_target, &profile, &mut ammunition, 0.0, &mut rng);
        assert_eq!(combat.engaged_position(), None);
        assert_eq!(combat.engaged_target(), Some(entity_target));
    }

    #[test]
    fn charged_cycle_uses_the_charge_clip_and_its_attack_tags() {
        let mut profile = profile();
        profile.charged_animation = Some(ChargedAttackAnimation {
            animation_type: "Pull".to_owned(),
            animations: vec![AttackAnimation {
                asset_path: "pull.uax".to_owned(),
                weight: 1,
                duration: 0.4,
                attack_positions: vec![0.5],
                events: Vec::new(),
                hardpoint_track: None,
            }],
        });
        let target = EntityId::new(crate::entity_id::EntityClass::Unit, 2);
        let mut combat = UnitCombat::default();
        let mut ammunition = UnitAmmunition::default();
        let mut rng = Random::new();

        combat.request_charged_cycle(true);
        let advance = combat.advance(0.71, target, &profile, &mut ammunition, 0.0, &mut rng);

        assert_eq!(advance.hit_count, 1);
        assert!(combat.uses_charged_animation());
        assert_eq!(profile.cycle_animation_type(true), "Pull");
        assert_eq!(profile.cycle_animations(true)[0].asset_path, "pull.uax");
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }
}
