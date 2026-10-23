//! The spatial deactivator of a body (the end of `hkRigidMotionUtil_applyForcesAndStep`,
//! `0x1408974B0`, and of `hkpSolveIsland_integrateFromAccumulators`, `0x1408991D0`).
//!
//! Every integration step increments a frame counter. Every 4th frame (except every 16th) the
//! body runs the high-frequency check, every 16th frame the low-frequency check. A check passes
//! when the centre of mass and the rotation stayed near the reference of that check. The
//! island sleeps when every body passed more than 5 checks of one frequency in a row.

use glam::Quat;

use super::DeactivationInfo;
use crate::body::Body;

/// The number of passed checks after which an island can sleep
/// (`hkpMotion::NUM_INACTIVE_FRAMES_TO_DEACTIVATE`).
pub(crate) const INACTIVE_CHECKS_TO_SLEEP: u8 = 5;

/// Runs the deactivation counters of a body after its motion step. Returns the larger of its
/// two counts of passed checks.
pub(crate) fn update(body: &mut Body, info: &DeactivationInfo) -> u8 {
    let counter = u32::from(body.deactivation.counter) + 1;
    let wrapped = u8::try_from(counter & 0xff).unwrap_or(0);
    body.deactivation.counter = wrapped;
    if wrapped.trailing_zeros() >= 2 {
        if wrapped & 0xf != 0 {
            check(body, info, 0);
        } else if counter == 0x100 {
            // The counter was 0xFF: the checks are disabled.
            body.deactivation.counter = 0xff;
        } else {
            body.deactivation.counter = 0;
            check(body, info, 1);
        }
    }
    let [high, low] = body.deactivation.inactive_frames;
    (high & 0x7f).max(low & 0x7f)
}

/// Compares the body with the reference of check `index` and counts or resets.
fn check(body: &mut Body, info: &DeactivationInfo, index: usize) {
    let state = &mut body.deactivation;
    let distance_squared = (state.reference_position[index] - body.center_of_mass).length_squared();
    let moved = distance_squared > info.max_distance_squared[index] || {
        let reference = unpack_rotation(state.reference_rotation[index]);
        let current = body.rotation;
        let difference = Quat::from_xyzw(
            reference.x - current.x,
            reference.y - current.y,
            reference.z - current.z,
            reference.w - current.w,
        );
        let squared = difference.x * difference.x
            + difference.y * difference.y
            + difference.z * difference.z
            + difference.w * difference.w;
        squared > info.max_rotation_squared[index]
    };
    if moved {
        state.inactive_frames[index] = 0;
        state.reference_position[index] = body.center_of_mass;
        state.reference_rotation[index] = pack_rotation(body.rotation);
    } else {
        let frames = state.inactive_frames[index] & 0x7f;
        // Counts up to 64 and stays there.
        state.inactive_frames[index] = frames - (frames >> 6) + 1;
    }
}

/// Packs a quaternion to 4 × 8 bits (`hkPackQuaternionToInt8x4`, `0x140D84540`).
///
/// Each component becomes `q · 116.36 + 196736.5`; the float bits shifted right by 6 give the
/// rounded value with 128 added in the low byte.
fn pack_rotation(rotation: Quat) -> u32 {
    let pack = |value: f32| ((value * 116.363_63 + 196_736.5).to_bits() >> 6) & 0xff;
    pack(rotation.x) | pack(rotation.y) << 8 | pack(rotation.z) << 16 | pack(rotation.w) << 24
}

/// Unpacks a quaternion packed by [`pack_rotation`]: `(byte − 128) · 0.00859375`.
fn unpack_rotation(packed: u32) -> Vec4Quat {
    let unpack = |shift: u32| {
        let byte = (packed >> shift) & 0xff;
        #[expect(
            clippy::cast_precision_loss,
            reason = "the value is in -128..=127, so the conversion is exact"
        )]
        let value = (i64::from(byte) - 128) as f32;
        value * 0.008_593_75
    };
    Vec4Quat {
        x: unpack(0),
        y: unpack(8),
        z: unpack(16),
        w: unpack(24),
    }
}

/// The four components of an unpacked reference rotation. They need not form a unit
/// quaternion.
struct Vec4Quat {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

#[cfg(test)]
mod tests {
    use glam::Quat;

    use super::{pack_rotation, unpack_rotation};

    #[test]
    fn packed_rotations_round_to_the_retail_grid() {
        let packed = pack_rotation(Quat::IDENTITY);
        assert_eq!(packed & 0xff, 128);
        assert_eq!(packed >> 24, 128 + 116);
        let unpacked = unpack_rotation(packed);
        assert!((unpacked.w - 0.996_875).abs() < 1e-6, "{}", unpacked.w);
        assert!(unpacked.x.abs() < 1e-7);
    }
}
