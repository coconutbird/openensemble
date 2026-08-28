//! Small deterministic combat calculations shared by targeting and launch code.

use crate::entities::Unit;
use glam::Vec3;

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

pub(super) fn face_position(unit: &mut Unit, target: Vec3) {
    let direction = Vec3::new(
        target.x - unit.base.position.x,
        0.0,
        target.z - unit.base.position.z,
    )
    .normalize_or_zero();
    if direction != Vec3::ZERO {
        unit.base.set_forward(direction);
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
