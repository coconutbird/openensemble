//! Retail hardpoint ownership, aiming, attack gating, and body-facing fallback.

use crate::entities::Unit;
use crate::entity_id::EntityId;
use crate::gameplay::AttackProfile;
use glam::{Quat, Vec3};

use super::helpers::unit_world_transform;
use super::{ConcreteTargetSnapshot, World};

const BODY_FACING_EPSILON: f32 = std::f32::consts::PI / 180.0;

impl World {
    pub(in crate::world) fn capture_hardpoint_yaw_targets(&self) -> Vec<(EntityId, Vec3)> {
        self.units
            .iter()
            .filter_map(|(unit_id, unit)| {
                let unit_world = unit_world_transform(unit)?;
                unit.combat
                    .hardpoint_yaw_before_owner_motion(unit_world)
                    .map(|target| (unit_id, target))
            })
            .collect()
    }

    pub(in crate::world) fn restore_hardpoint_yaw_targets(&mut self, targets: &[(EntityId, Vec3)]) {
        for &(unit_id, target) in targets {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            let Some(unit_world) = unit_world_transform(unit) else {
                continue;
            };
            unit.combat
                .restore_hardpoint_yaw_after_owner_motion(unit_world, target);
        }
    }
}

pub(super) fn advance_auto_center(unit: &mut Unit, elapsed: f32) {
    unit.combat.advance_hardpoint_auto_center(elapsed);
}

pub(super) fn prepare_for_attack(
    unit: &mut Unit,
    profile: &AttackProfile,
    target: &ConcreteTargetSnapshot,
    target_world: Vec3,
    charged_cycle: bool,
    orientation_tolerance: f32,
    elapsed: f32,
) -> bool {
    unit.combat.prepare_target(profile, target.id);
    unit.combat.request_charged_cycle(charged_cycle);
    orient_for_attack(
        unit,
        profile,
        target_world,
        orientation_tolerance,
        true,
        elapsed,
    )
}

pub(super) fn orient_for_attack(
    unit: &mut Unit,
    profile: &AttackProfile,
    target_world: Vec3,
    orientation_tolerance: f32,
    check_hit_orientation: bool,
    elapsed: f32,
) -> bool {
    if profile.orientation.skips_orientation_check() {
        return true;
    }
    let anchor = unit.combat.hardpoint_anchor(profile);
    let working = unit.combat.is_animating();
    if !(working && profile.orientation.is_stationary()) {
        update_orientation(unit, profile, anchor.as_ref(), target_world, elapsed);
    }
    if !check_hit_orientation {
        return true;
    }
    let Some(unit_world) = unit_world_transform(unit) else {
        return true;
    };
    unit.combat.is_hardpoint_oriented(
        profile,
        anchor.as_ref(),
        unit_world,
        target_world,
        orientation_tolerance,
    )
}

fn update_orientation(
    unit: &mut Unit,
    profile: &AttackProfile,
    anchor: Option<&crate::gameplay::AttackAnimationAnchor>,
    target_world: Vec3,
    elapsed: f32,
) {
    let Some(unit_world) = unit_world_transform(unit) else {
        return;
    };
    let Some(aim) =
        unit.combat
            .orient_hardpoint(profile, anchor, unit_world, target_world, elapsed)
    else {
        if profile.orientation.owner_can_rotate() && profile.orientation.can_orient_owner() {
            let _aligned = turn_body_toward(unit, target_world, elapsed);
        }
        return;
    };
    if aim.can_face && !unit.combat.hardpoint_forces_body_facing() {
        return;
    }
    if !profile.orientation.owner_can_rotate() || !profile.orientation.can_orient_owner() {
        return;
    }

    let preserved_yaw = unit
        .combat
        .hardpoint_yaw_target_world(profile, anchor, unit_world);
    let already_aligned = turn_body_toward(unit, target_world, elapsed);
    unit.combat
        .set_hardpoint_force_body_facing(!already_aligned);
    let Some(unit_world) = unit_world_transform(unit) else {
        return;
    };
    if let Some(preserved_yaw) = preserved_yaw {
        unit.combat
            .preserve_hardpoint_yaw_target(profile, anchor, unit_world, preserved_yaw);
    }
}

fn turn_body_toward(unit: &mut Unit, target_world: Vec3, elapsed: f32) -> bool {
    let desired = Vec3::new(
        target_world.x - unit.base.position.x,
        0.0,
        target_world.z - unit.base.position.z,
    )
    .normalize_or_zero();
    if desired == Vec3::ZERO {
        return true;
    }
    let current = Vec3::new(unit.base.forward.x, 0.0, unit.base.forward.z).normalize_or_zero();
    if current == Vec3::ZERO {
        unit.base.set_forward(desired);
        return true;
    }
    let angle = current.dot(desired).clamp(-1.0, 1.0).acos();
    if angle <= BODY_FACING_EPSILON {
        unit.base.set_forward(desired);
        return true;
    }
    let maximum = unit.turn_rate_degrees.max(0.0).to_radians() * elapsed.max(0.0);
    if maximum <= 0.0 {
        return false;
    }
    let signed = maximum.min(angle).copysign(current.cross(desired).y);
    unit.base
        .set_forward(Quat::from_rotation_y(signed) * current);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_facing_fallback_uses_authored_turn_rate() {
        let mut unit = Unit::default();
        unit.base.set_forward(Vec3::Z);
        unit.turn_rate_degrees = 90.0;

        assert!(!turn_body_toward(&mut unit, Vec3::X * 10.0, 0.5));
        assert!(unit.base.forward.abs_diff_eq(
            Vec3::new(
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
            ),
            1.0e-5,
        ));
        assert!(!turn_body_toward(&mut unit, Vec3::X * 10.0, 0.5));
        assert!(unit.base.forward.abs_diff_eq(Vec3::X, 1.0e-5));
        assert!(turn_body_toward(&mut unit, Vec3::X * 10.0, 0.5));
    }
}
