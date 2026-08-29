//! Synchronized state for one running retail Rage power.

use super::super::{PowerExecutionId, common::hash_string};
use crate::EntityId;
use crate::commands::PowerUserId;
use crate::player::{PlayerId, ProtoPowerId};
use crate::sync::SyncChecksum;
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RagePowerPhase {
    #[default]
    Active = 0,
    Moving = 1,
    Jumping = 2,
    Attacking = 3,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RageFlags(u8);

impl RageFlags {
    const IGNORE_REQUIREMENTS: Self = Self(1 << 0);
    const USE_PATHER: Self = Self(1 << 1);

    pub(super) fn new(ignore_requirements: bool, use_pather: bool) -> Self {
        let mut flags = Self::default();
        flags.set(Self::IGNORE_REQUIREMENTS, ignore_requirements);
        flags.set(Self::USE_PATHER, use_pather);
        flags
    }

    const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        } else {
            self.0 &= !flag.0;
        }
    }

    pub(super) const fn ignores_requirements(self) -> bool {
        self.contains(Self::IGNORE_REQUIREMENTS)
    }

    pub(super) const fn uses_pather(self) -> bool {
        self.contains(Self::USE_PATHER)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RageSpline {
    a0: Vec3,
    a1: Vec3,
    a2: Vec3,
}

impl RageSpline {
    pub(super) fn through(start: Vec3, middle: Vec3, end: Vec3) -> Self {
        let a0 = start;
        let a2 = ((middle - start) - 0.5 * (end - start)) / -0.25;
        let a1 = end - start - a2;
        Self { a0, a1, a2 }
    }

    pub(super) fn evaluate(self, t: f32) -> Vec3 {
        self.a2 * t * t + self.a1 * t + self.a0
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        hash_vec3(checksum, self.a0);
        hash_vec3(checksum, self.a1);
        hash_vec3(checksum, self.a2);
    }
}

#[derive(Debug, Clone)]
pub(super) struct AuraAttachmentSet {
    pub squad_id: EntityId,
    pub attachment_ids: Vec<EntityId>,
}

/// Authoritative state for one running retail `BPowerRage` execution.
#[derive(Debug, Clone)]
pub struct RagePowerExecution {
    pub(super) id: PowerExecutionId,
    pub(super) player_id: PlayerId,
    pub(super) proto_power_id: ProtoPowerId,
    pub(super) power_level: u32,
    pub(super) owner_squad_id: EntityId,
    pub(super) power_user_id: PowerUserId,
    pub(super) target_location: Vec3,
    pub(super) target_squad_id: EntityId,
    pub(super) phase: RagePowerPhase,
    pub(super) move_input: Vec3,
    pub(super) move_target: Vec3,
    pub(super) teleport_destination: Vec3,
    pub(super) spline: RageSpline,
    pub(super) teleport_remaining: f32,
    pub(super) retarget_remaining: f32,
    pub(super) elapsed_seconds: f32,
    pub(super) next_tick_time: f32,
    pub(super) tick_length: f32,
    pub(super) supplies_per_tick: f32,
    pub(super) supplies_per_tick_attacking: f32,
    pub(super) supplies_per_jump: f32,
    pub(super) supplies_resource_id: usize,
    pub(super) damage_multiplier: f32,
    pub(super) damage_taken_multiplier: f32,
    pub(super) speed_multiplier: f32,
    pub(super) nudge_multiplier: f32,
    pub(super) scan_radius: f32,
    pub(super) teleport_time: f32,
    pub(super) teleport_lateral_distance: f32,
    pub(super) teleport_jump_distance: f32,
    pub(super) time_between_retarget: f32,
    pub(super) distance_vs_angle_weight: f32,
    pub(super) projectile_prototype: String,
    pub(super) hand_attachment_prototype: String,
    pub(super) hand_attachment_prototype_id: i32,
    pub(super) teleport_attachment_prototype: String,
    pub(super) teleport_attachment_prototype_id: i32,
    pub(super) aura_attachment_prototypes: [String; 3],
    pub(super) aura_attachment_prototype_ids: [i32; 3],
    pub(super) heal_attachment_prototype: String,
    pub(super) heal_attachment_prototype_id: i32,
    pub(super) aura_filter_type: String,
    pub(super) heal_per_kill_combat_value: f32,
    pub(super) aura_radius: f32,
    pub(super) aura_damage_bonus: f32,
    pub(super) flags: RageFlags,
    pub(super) hand_attachment_ids: Vec<EntityId>,
    pub(super) aura_squad_ids: Vec<EntityId>,
    pub(super) aura_attachments: Vec<AuraAttachmentSet>,
}

impl RagePowerExecution {
    #[must_use]
    pub const fn id(&self) -> PowerExecutionId {
        self.id
    }

    #[must_use]
    pub const fn player_id(&self) -> PlayerId {
        self.player_id
    }

    #[must_use]
    pub const fn proto_power_id(&self) -> ProtoPowerId {
        self.proto_power_id
    }

    #[must_use]
    pub const fn power_level(&self) -> u32 {
        self.power_level
    }

    #[must_use]
    pub const fn owner_squad_id(&self) -> EntityId {
        self.owner_squad_id
    }

    #[must_use]
    pub const fn power_user_id(&self) -> PowerUserId {
        self.power_user_id
    }

    #[must_use]
    pub fn target_squad_id(&self) -> Option<EntityId> {
        if self.target_squad_id.is_invalid() {
            None
        } else {
            Some(self.target_squad_id)
        }
    }

    #[must_use]
    pub const fn phase(&self) -> RagePowerPhase {
        self.phase
    }

    #[must_use]
    pub const fn tick_length(&self) -> f32 {
        self.tick_length
    }

    #[must_use]
    pub const fn supplies_per_tick(&self) -> f32 {
        self.supplies_per_tick
    }

    #[must_use]
    pub const fn supplies_per_tick_attacking(&self) -> f32 {
        self.supplies_per_tick_attacking
    }

    #[must_use]
    pub const fn supplies_per_jump(&self) -> f32 {
        self.supplies_per_jump
    }

    #[must_use]
    pub const fn damage_multiplier(&self) -> f32 {
        self.damage_multiplier
    }

    #[must_use]
    pub const fn damage_taken_multiplier(&self) -> f32 {
        self.damage_taken_multiplier
    }

    #[must_use]
    pub const fn speed_multiplier(&self) -> f32 {
        self.speed_multiplier
    }

    #[must_use]
    pub const fn nudge_multiplier(&self) -> f32 {
        self.nudge_multiplier
    }

    #[must_use]
    pub const fn scan_radius(&self) -> f32 {
        self.scan_radius
    }

    #[must_use]
    pub const fn teleport_time(&self) -> f32 {
        self.teleport_time
    }

    #[must_use]
    pub const fn aura_radius(&self) -> f32 {
        self.aura_radius
    }

    #[must_use]
    pub const fn aura_damage_bonus(&self) -> f32 {
        self.aura_damage_bonus
    }

    #[must_use]
    pub const fn heal_per_kill_combat_value(&self) -> f32 {
        self.heal_per_kill_combat_value
    }

    #[must_use]
    pub fn projectile_prototype(&self) -> &str {
        &self.projectile_prototype
    }

    #[must_use]
    pub fn hand_attachment_prototype(&self) -> &str {
        &self.hand_attachment_prototype
    }

    #[must_use]
    pub fn teleport_attachment_prototype(&self) -> &str {
        &self.teleport_attachment_prototype
    }

    #[must_use]
    pub fn aura_filter_type(&self) -> &str {
        &self.aura_filter_type
    }

    #[must_use]
    pub fn aura_squad_ids(&self) -> &[EntityId] {
        &self.aura_squad_ids
    }

    #[must_use]
    pub fn hand_attachment_ids(&self) -> &[EntityId] {
        &self.hand_attachment_ids
    }

    #[must_use]
    pub const fn ignores_requirements(&self) -> bool {
        self.flags.ignores_requirements()
    }

    #[must_use]
    pub const fn uses_pather(&self) -> bool {
        self.flags.uses_pather()
    }

    pub(in crate::world::powers) fn hash_state(&self, checksum: &mut SyncChecksum) {
        self.hash_identity(checksum);
        self.hash_motion(checksum);
        self.hash_profile(checksum);
        self.hash_visuals(checksum);
    }

    fn hash_identity(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.id.get());
        checksum.hash_u32(u32::from(self.player_id));
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_u32(self.power_level);
        checksum.hash_u32(self.owner_squad_id.as_u32());
        checksum.hash_u32(self.power_user_id.raw());
        checksum.hash_u32(self.target_squad_id.as_u32());
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(u32::from(self.ignores_requirements()));
        checksum.hash_u32(u32::from(self.uses_pather()));
    }

    fn hash_motion(&self, checksum: &mut SyncChecksum) {
        hash_vec3(checksum, self.target_location);
        hash_vec3(checksum, self.move_input);
        hash_vec3(checksum, self.move_target);
        hash_vec3(checksum, self.teleport_destination);
        self.spline.hash_state(checksum);
        checksum.hash_f32(self.teleport_remaining);
        checksum.hash_f32(self.retarget_remaining);
        checksum.hash_f32(self.elapsed_seconds);
        checksum.hash_f32(self.next_tick_time);
    }

    fn hash_profile(&self, checksum: &mut SyncChecksum) {
        for value in [
            self.tick_length,
            self.supplies_per_tick,
            self.supplies_per_tick_attacking,
            self.supplies_per_jump,
            self.damage_multiplier,
            self.damage_taken_multiplier,
            self.speed_multiplier,
            self.nudge_multiplier,
            self.scan_radius,
            self.teleport_time,
            self.teleport_lateral_distance,
            self.teleport_jump_distance,
            self.time_between_retarget,
            self.distance_vs_angle_weight,
            self.heal_per_kill_combat_value,
            self.aura_radius,
            self.aura_damage_bonus,
        ] {
            checksum.hash_f32(value);
        }
        checksum.hash_u32(u32::try_from(self.supplies_resource_id).unwrap_or(u32::MAX));
        hash_string(checksum, &self.projectile_prototype);
        hash_string(checksum, &self.aura_filter_type);
    }

    fn hash_visuals(&self, checksum: &mut SyncChecksum) {
        hash_prototype(
            checksum,
            &self.hand_attachment_prototype,
            self.hand_attachment_prototype_id,
        );
        hash_prototype(
            checksum,
            &self.teleport_attachment_prototype,
            self.teleport_attachment_prototype_id,
        );
        for (name, id) in self
            .aura_attachment_prototypes
            .iter()
            .zip(self.aura_attachment_prototype_ids)
        {
            hash_prototype(checksum, name, id);
        }
        hash_prototype(
            checksum,
            &self.heal_attachment_prototype,
            self.heal_attachment_prototype_id,
        );
        hash_entity_ids(checksum, &self.hand_attachment_ids);
        hash_entity_ids(checksum, &self.aura_squad_ids);
        checksum.hash_u32(u32::try_from(self.aura_attachments.len()).unwrap_or(u32::MAX));
        for set in &self.aura_attachments {
            checksum.hash_u32(set.squad_id.as_u32());
            hash_entity_ids(checksum, &set.attachment_ids);
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::world::powers) struct PendingRageKill {
    pub attacker_id: EntityId,
    pub target_prototype: String,
}

impl PendingRageKill {
    pub(in crate::world::powers) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.attacker_id.as_u32());
        hash_string(checksum, &self.target_prototype);
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

fn hash_entity_ids(checksum: &mut SyncChecksum, ids: &[EntityId]) {
    checksum.hash_u32(u32::try_from(ids.len()).unwrap_or(u32::MAX));
    for id in ids {
        checksum.hash_u32(id.as_u32());
    }
}

fn hash_prototype(checksum: &mut SyncChecksum, name: &str, id: i32) {
    hash_string(checksum, name);
    checksum.hash_i32(id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retail_parametric_spline_passes_all_three_control_points() {
        let start = Vec3::ZERO;
        let middle = Vec3::new(4.0, 10.0, 3.0);
        let end = Vec3::new(8.0, 0.0, 6.0);
        let spline = RageSpline::through(start, middle, end);
        assert_eq!(spline.evaluate(0.0), start);
        assert_eq!(spline.evaluate(0.5), middle);
        assert_eq!(spline.evaluate(1.0), end);
    }
}
