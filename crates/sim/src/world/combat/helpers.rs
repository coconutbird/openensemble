//! Small deterministic combat calculations shared by targeting and launch code.

use super::fire::LaunchTuning;
use super::{AttackerSnapshot, CombatMotion, ConcreteTargetSnapshot, TargetSnapshot};
use crate::entities::Unit;
use crate::entities::projectiles::{ballistic_aim_direction, launch_target_position};
use crate::gameplay::{
    AttackAccuracyProfile, AttackAnimationAnchor, AttackProfile, GameplayCatalog,
};
use glam::{Mat4, Vec3};

pub(super) fn animation_anchor_world_transform(
    unit: &Unit,
    anchor: &AttackAnimationAnchor,
) -> Option<Mat4> {
    Some(
        unit_world_transform(unit)?
            * anchor.unit_transform_with_hardpoints(
                |name| unit.combat.hardpoint_attachment_transform(name),
                |name| unit.combat.hardpoint_bone_transform(name),
            ),
    )
}

pub(super) fn unit_world_transform(unit: &Unit) -> Option<Mat4> {
    let position = unit.base.position;
    let forward = Vec3::new(unit.base.forward.x, 0.0, unit.base.forward.z).try_normalize()?;
    if !position.is_finite() {
        return None;
    }
    let right = Vec3::Y.cross(forward).try_normalize()?;
    Some(Mat4::from_cols(
        right.extend(0.0),
        Vec3::Y.extend(0.0),
        forward.extend(0.0),
        position.extend(1.0),
    ))
}

pub(super) fn attack_aim_position(
    attacker: &AttackerSnapshot,
    target: &ConcreteTargetSnapshot,
    profile: &AttackProfile,
    gameplay: &GameplayCatalog,
    tuning: &LaunchTuning,
) -> Vec3 {
    let projectile = profile
        .projectile
        .as_deref()
        .and_then(|name| gameplay.projectile(name));
    let projectile_speed = projectile.map_or(0.0, |projectile| projectile.speed);
    let led_position = launch_target_position(
        attacker.position,
        target.position,
        target.velocity,
        projectile_speed,
        tuning.max_velocity_lead,
    );
    let led_aim_position = target.aim_position + led_position - target.position;
    let Some((hardpoint_position, projectile)) = attacker.hardpoint_position.zip(projectile) else {
        return led_aim_position;
    };
    ballistic_aim_direction(
        hardpoint_position,
        attacker.position,
        led_aim_position,
        target.position,
        target.collision_radius,
        tuning.max_range,
        projectile,
    )
    .map_or(led_aim_position, |direction| {
        hardpoint_position + direction * 100.0
    })
}

pub(super) fn effective_attack_accuracy(
    technologies: &crate::player::PlayerTechState,
    proto_object: &str,
    weapon: &str,
    base: AttackAccuracyProfile,
) -> AttackAccuracyProfile {
    AttackAccuracyProfile {
        accuracy: technologies.weapon_accuracy(proto_object, weapon, base.accuracy),
        moving_accuracy: technologies.weapon_moving_accuracy(
            proto_object,
            weapon,
            base.moving_accuracy,
        ),
        max_deviation: technologies.weapon_max_deviation(proto_object, weapon, base.max_deviation),
        moving_max_deviation: technologies.weapon_moving_max_deviation(
            proto_object,
            weapon,
            base.moving_max_deviation,
        ),
        distance_factor: technologies.weapon_accuracy_distance_factor(
            proto_object,
            weapon,
            base.distance_factor,
        ),
        deviation_factor: technologies.weapon_accuracy_deviation_factor(
            proto_object,
            weapon,
            base.deviation_factor,
        ),
    }
}

pub(super) fn selected_range(range_override: f32, authored_range: f32, scalar: f32) -> f32 {
    if range_override.is_finite() && range_override > 0.0 {
        range_override
    } else {
        authored_range * scalar
    }
}

pub(super) fn xz_distance_squared(left: Vec3, right: Vec3) -> f32 {
    Vec3::new(left.x - right.x, 0.0, left.z - right.z).length_squared()
}

pub(super) fn combat_motion(
    position: Vec3,
    target: TargetSnapshot,
    range: Option<f32>,
) -> CombatMotion {
    let Some(range) = range else {
        return CombatMotion::Hold(target);
    };
    let offset = Vec3::new(
        target.position.x - position.x,
        0.0,
        target.position.z - position.z,
    );
    if offset.length_squared() <= range * range {
        CombatMotion::Hold(target)
    } else {
        CombatMotion::Chase(target)
    }
}

pub(super) fn scaled_launch_damage(
    authored_damage: f32,
    damage_multiplier: f32,
    source_position: Vec3,
    target_position: Vec3,
    uses_height_bonus_damage: bool,
    height_bonus_factor: f32,
) -> f32 {
    let mut damage = authored_damage * damage_multiplier;
    if uses_height_bonus_damage {
        let height_difference = (source_position.y - target_position.y).max(0.0);
        damage *= 1.0 + height_difference * height_bonus_factor;
    }
    if damage.is_finite() && damage > 0.0 {
        damage
    } else {
        0.0
    }
}
