//! Immutable fire-event data separated from combat orchestration.

use super::deviation::LaunchAccuracy;
use super::{AttackerSnapshot, ConcreteTargetSnapshot};
use crate::EntityId;
use crate::gameplay::{
    AreaDamageProfile, AttackProfile, ImpactEffectProfile, ProjectileReactionFlags,
    PullAttackProfile,
};
use crate::player::PlayerId;
use glam::Vec3;

#[derive(Debug, Clone)]
pub(super) struct FireEvent {
    pub(super) source_id: EntityId,
    pub(super) source_player_id: PlayerId,
    pub(super) source_position: Vec3,
    pub(super) launch_position: Vec3,
    pub(super) target: ConcreteTargetSnapshot,
    pub(super) damage: f32,
    pub(super) weapon_type: Option<String>,
    pub(super) projectile_name: Option<String>,
    pub(super) impact_effect: Option<ImpactEffectProfile>,
    pub(super) area_damage: Option<AreaDamageProfile>,
    pub(super) action_name: String,
    pub(super) pull: Option<PullAttackProfile>,
    pub(super) normal_range: f32,
    pub(super) max_range: f32,
    pub(super) max_velocity_lead: f32,
    pub(super) accuracy: LaunchAccuracy,
    pub(super) friendly_fire: bool,
    pub(super) collides_with_all_units: bool,
    pub(super) targets_foot_of_unit: bool,
    pub(super) projectile_reactions: ProjectileReactionFlags,
}

impl FireEvent {
    pub(super) fn from_attack(
        source_id: EntityId,
        attacker: &AttackerSnapshot,
        target: ConcreteTargetSnapshot,
        damage: f32,
        profile: &AttackProfile,
        area_damage: Option<AreaDamageProfile>,
        normal_range: f32,
    ) -> Self {
        Self {
            source_id,
            source_player_id: attacker.player_id,
            source_position: attacker.position,
            launch_position: attacker.launch_position,
            target,
            damage,
            weapon_type: profile.weapon_type.clone(),
            projectile_name: profile.projectile.clone(),
            impact_effect: profile.impact_effect.clone(),
            area_damage,
            action_name: profile.action_name.clone(),
            pull: profile.pull.clone(),
            normal_range,
            max_range: profile.max_range,
            max_velocity_lead: profile.max_velocity_lead,
            accuracy: LaunchAccuracy::new(profile.accuracy, false, 1.0, 1.0),
            friendly_fire: profile.friendly_fire,
            collides_with_all_units: !profile.targets_foot_of_unit,
            targets_foot_of_unit: profile.targets_foot_of_unit,
            projectile_reactions: profile.projectile_reactions,
        }
    }

    pub(super) fn with_launch_tuning(mut self, tuning: LaunchTuning) -> Self {
        self.max_range = tuning.max_range;
        self.max_velocity_lead = tuning.max_velocity_lead;
        self.accuracy = tuning.accuracy;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LaunchTuning {
    pub(super) max_range: f32,
    pub(super) max_velocity_lead: f32,
    pub(super) accuracy: LaunchAccuracy,
}
