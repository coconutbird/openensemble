//! Elementary functions that give the same bits on each machine.
//!
//! The platform libm gives different bits for `sin`, `acos` and the other elementary functions on
//! different machines. `Quat::slerp` of glam calls them too. The simulation uses the sampled
//! poses, so the functions in this module use `floaty`, which rounds each result correctly. Do
//! not call the `f32` methods or the glam helpers that use the platform libm.

use floaty::F32;
use glam::Quat;

/// Returns the sine of `angle` (radians), correctly rounded.
fn sin(angle: f32) -> f32 {
    f32::from(F32::from(angle).sin())
}

/// Returns the arc cosine of `value` in radians, correctly rounded.
fn acos(value: f32) -> f32 {
    f32::from(F32::from(value).acos())
}

/// Returns the spherical linear interpolation from `start` to `end` at `s`.
///
/// The formula is that of the scalar `Quat::slerp` in glam 0.29. The angle between the two
/// rotations uses the correctly rounded `acos` in place of the polynomial of glam.
pub(crate) fn quat_slerp(start: Quat, end: Quat, s: f32) -> Quat {
    const DOT_THRESHOLD: f32 = 1.0 - f32::EPSILON;
    let mut end = end;
    let mut dot = start.dot(end);
    if dot < 0.0 {
        end = -end;
        dot = -dot;
    }
    if dot > DOT_THRESHOLD {
        return (start * (1.0 - s) + end * s).normalize();
    }
    let theta = acos(dot);
    let start_scale = sin(theta * (1.0 - s));
    let end_scale = sin(theta * s);
    let theta_sin = sin(theta);
    (start * start_scale + end * end_scale) * (1.0 / theta_sin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_give_correctly_rounded_bits() {
        // The expected bits come from `floaty`. The Windows MSVC libm gives the adjacent value
        // for the second input of `sin`.
        assert_eq!(sin(1.0).to_bits(), 0x3F57_6AA4);
        assert_eq!(sin(f32::from_bits(0x3EF8_3D69)).to_bits(), 0x3EEE_A0B7);
        assert_eq!(acos(0.5).to_bits(), 0x3F86_0A92);
        assert_eq!(acos(0.0).to_bits(), std::f32::consts::FRAC_PI_2.to_bits());
    }

    #[test]
    fn slerp_matches_glam() {
        let start = Quat::from_rotation_y(0.4);
        for end in [
            Quat::from_rotation_x(1.2),
            -Quat::from_rotation_x(1.2),
            start,
        ] {
            for s in [0.0_f32, 0.25, 0.5, 1.0] {
                let actual = quat_slerp(start, end, s);
                let expected = start.slerp(end, s);
                assert!(
                    actual.abs_diff_eq(expected, 0.000_01),
                    "{actual:?} != {expected:?}"
                );
            }
        }
    }
}
