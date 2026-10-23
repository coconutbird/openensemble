//! Elementary functions that give the same bits on each machine.
//!
//! The platform libm gives different bits for `sin`, `cos`, `atan2` and the other elementary
//! functions on different machines. The glam rotation builders call the platform libm too. The
//! functions in this module use `floaty`, which rounds each result correctly. Use these functions
//! in all simulation code. Do not call the `f32` methods or the glam builders directly.
//!
//! `motion::math` holds the functions that `motion` also uses. This module makes them available
//! with their names, so that each call site uses this module.

use floaty::F32;
use glam::{Quat, Vec3};
pub(crate) use motion::math::{acos, quat_slerp, sin};

/// Returns `function(value)`, calculated by `floaty`.
fn unary(value: f32, function: fn(F32) -> F32) -> f32 {
    f32::from(function(F32::from(value)))
}

/// Returns the cosine of `angle` (radians), correctly rounded.
pub(crate) fn cos(angle: f32) -> f32 {
    unary(angle, F32::cos)
}

/// Returns the sine and the cosine of `angle` (radians), each correctly rounded.
pub(crate) fn sin_cos(angle: f32) -> (f32, f32) {
    (sin(angle), cos(angle))
}

/// Returns the tangent of `angle` (radians), correctly rounded.
pub(crate) fn tan(angle: f32) -> f32 {
    unary(angle, F32::tan)
}

/// Returns the arc tangent of `value` in radians, correctly rounded.
pub(crate) fn atan(value: f32) -> f32 {
    unary(value, F32::atan)
}

/// Returns the angle of the point `(x, y)` in radians, correctly rounded, as `y.atan2(x)` does.
pub(crate) fn atan2(y: f32, x: f32) -> f32 {
    f32::from(F32::from(y).atan2(F32::from(x)))
}

/// Returns `sqrt(x * x + y * y)` without intermediate overflow, correctly rounded.
pub(crate) fn hypot(x: f32, y: f32) -> f32 {
    f32::from(F32::from(x).hypot(F32::from(y)))
}

/// Returns the rotation of `angle` (radians) about the unit vector `axis`.
///
/// The formula is that of `Quat::from_axis_angle` in glam 0.29.
pub(crate) fn quat_from_axis_angle(axis: Vec3, angle: f32) -> Quat {
    let (sin, cos) = sin_cos(angle * 0.5);
    let vector = axis * sin;
    Quat::from_xyzw(vector.x, vector.y, vector.z, cos)
}

/// Returns the rotation of `angle` (radians) about the x axis.
///
/// The formula is that of `Quat::from_rotation_x` in glam 0.29.
pub(crate) fn quat_from_rotation_x(angle: f32) -> Quat {
    let (sin, cos) = sin_cos(angle * 0.5);
    Quat::from_xyzw(sin, 0.0, 0.0, cos)
}

/// Returns the rotation of `angle` (radians) about the y axis.
///
/// The formula is that of `Quat::from_rotation_y` in glam 0.29.
pub(crate) fn quat_from_rotation_y(angle: f32) -> Quat {
    let (sin, cos) = sin_cos(angle * 0.5);
    Quat::from_xyzw(0.0, sin, 0.0, cos)
}

/// Returns the shortest rotation from the unit vector `from` to the unit vector `to`.
///
/// The formula is that of `Quat::from_rotation_arc` in glam 0.29. Only the half turn for
/// opposite vectors uses an elementary function.
pub(crate) fn quat_from_rotation_arc(from: Vec3, to: Vec3) -> Quat {
    const ONE_MINUS_EPSILON: f32 = 1.0 - 2.0 * f32::EPSILON;
    let dot = from.dot(to);
    if dot > ONE_MINUS_EPSILON {
        Quat::IDENTITY
    } else if dot < -ONE_MINUS_EPSILON {
        quat_from_axis_angle(from.any_orthonormal_vector(), std::f32::consts::PI)
    } else {
        let cross = from.cross(to);
        Quat::from_xyzw(cross.x, cross.y, cross.z, 1.0 + dot).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_give_correctly_rounded_bits() {
        // The expected bits come from `floaty`. `f64` results of the same inputs round to the
        // same bits. `motion::math` tests `sin` and `acos`.
        assert_eq!(cos(1.0).to_bits(), 0x3F0A_5140);
        assert_eq!(
            sin_cos(1.0),
            (f32::from_bits(0x3F57_6AA4), f32::from_bits(0x3F0A_5140))
        );
        assert_eq!(tan(0.5).to_bits(), 0x3F0B_DA7B);
        assert_eq!(atan(0.5).to_bits(), 0x3EED_6338);
        assert_eq!(atan2(1.0, 2.0).to_bits(), 0x3EED_6338);
        assert_eq!(atan2(1.0, -1.0).to_bits(), 0x4016_CBE4);
        assert_eq!(hypot(3.0, 4.0).to_bits(), 5.0_f32.to_bits());
        assert_eq!(hypot(1.0, 1.0).to_bits(), 0x3FB5_04F3);
        // The Windows MSVC libm gives the adjacent value for these two inputs.
        assert_eq!(cos(f32::from_bits(0x3F21_FFE6)).to_bits(), 0x3F4E_6E27);
        assert_eq!(atan(f32::from_bits(0x3ED6_D120)).to_bits(), 0x3ECB_6556);
    }

    #[test]
    fn rotation_builders_match_glam() {
        let axis = Vec3::new(1.0, 2.0, 3.0).normalize();
        for angle in [-2.5_f32, -0.75, 0.0, 0.3, 1.0, 3.0] {
            assert_close(
                quat_from_axis_angle(axis, angle),
                Quat::from_axis_angle(axis, angle),
            );
            assert_close(quat_from_rotation_x(angle), Quat::from_rotation_x(angle));
            assert_close(quat_from_rotation_y(angle), Quat::from_rotation_y(angle));
        }
        let from = Vec3::new(0.0, 0.6, 0.8);
        for to in [from, -from, Vec3::X, Vec3::new(0.0, -0.8, 0.6)] {
            assert_close(
                quat_from_rotation_arc(from, to),
                Quat::from_rotation_arc(from, to),
            );
        }
    }

    fn assert_close(actual: Quat, expected: Quat) {
        assert!(
            actual.abs_diff_eq(expected, 0.000_01),
            "{actual:?} != {expected:?}"
        );
    }
}
