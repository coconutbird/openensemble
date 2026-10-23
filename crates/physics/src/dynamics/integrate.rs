//! The motion step of a body (`hkSweptTransformUtil` as the DE binary inlines it).
//!
//! Two callers exist:
//!
//! - [`step_direct`] for islands without constraints
//!   (`hkRigidMotionUtil_applyForcesAndStep_deactivation`, `0x1408974B0`): gravity, damping,
//!   the velocity clamp, then the transform step with the clamped velocity.
//! - [`step_from_solver`] for islands that ran the solver
//!   (`hkpSolveIsland_integrateFromAccumulators`, `0x1408991D0`): the transform step with the
//!   solver's sum velocity, then the velocity clamp.

use glam::{Quat, Vec3};

use super::solver::Accumulator;
use super::{StepInfo, deactivation, decode_ufloat8};
use crate::body::{Body, MotionType};

/// The velocity that Havok writes when a velocity is not finite or too large
/// (`xmmword_1414A95A0`).
const RESET_VELOCITY: Vec3 = Vec3::X;

/// The largest velocity component that the guard accepts (`xmmword_14145C460`).
const MAX_VELOCITY_COMPONENT: f32 = 1e6;

/// The largest rotation of one step in half turns (`dword_1411F3DB0`).
const MAX_ROTATION_HALF_TURNS: f32 = 0.9;

/// `4 / π²`: turns the squared half angle into squared half turns (`dword_140F19A00`).
const HALF_TURNS_PER_RADIAN_SQUARED: f32 = 0.405_284_7;

/// Runs the direct motion step of a dynamic or keyframed body and its deactivation counters.
/// Returns the larger count of passed deactivation checks.
pub(crate) fn step_direct(body: &mut Body, step: &StepInfo) -> u8 {
    let dt = step.delta_time;
    if body.motion == MotionType::BoxInertia {
        body.linear_velocity += step.gravity_per_step;
        body.linear_velocity *= damping_factor(dt, body.linear_damping);
        body.angular_velocity *= damping_factor(dt, body.angular_damping);
    }
    if !velocities_valid(body.linear_velocity, body.angular_velocity) {
        body.linear_velocity = RESET_VELOCITY;
        body.angular_velocity = RESET_VELOCITY;
    }
    body.linear_velocity = clamp_length(
        body.linear_velocity,
        decode_ufloat8(body.max_linear_velocity),
    );
    let mut linear = body.linear_velocity;
    let mut angular = body.angular_velocity;
    integrate_transform(body, &mut linear, &mut angular, dt);
    // The direct step keeps the angular velocity that the rotation cap scaled.
    body.angular_velocity = angular;
    deactivation::update(body, &step.deactivation)
}

/// Runs the motion step of a body from its solver accumulator and its deactivation counters.
/// Returns the larger count of passed deactivation checks.
pub(crate) fn step_from_solver(body: &mut Body, accumulator: &Accumulator, step: &StepInfo) -> u8 {
    let (mut sum_linear, mut sum_angular) = if body.motion == MotionType::BoxInertia {
        let rotation = body.rotation_matrix();
        body.linear_velocity = accumulator.linear_velocity;
        body.angular_velocity = rotation * accumulator.angular_velocity;
        (accumulator.sum_linear, rotation * accumulator.sum_angular)
    } else {
        (body.linear_velocity, body.angular_velocity)
    };
    if !velocities_valid(sum_linear, sum_angular) {
        sum_linear = RESET_VELOCITY;
        sum_angular = RESET_VELOCITY;
    }
    integrate_transform(body, &mut sum_linear, &mut sum_angular, step.delta_time);
    body.linear_velocity = clamp_or_reset(
        body.linear_velocity,
        decode_ufloat8(body.max_linear_velocity),
    );
    body.angular_velocity = clamp_or_reset(
        body.angular_velocity,
        decode_ufloat8(body.max_angular_velocity),
    );
    deactivation::update(body, &step.deactivation)
}

/// `max(0, 1 − dt · damping)`.
pub(crate) fn damping_factor(dt: f32, damping: f32) -> f32 {
    let factor = 1.0 - dt * damping;
    if 0.0 > factor { 0.0 } else { factor }
}

/// `true` if every component is finite and below 1e6 in size.
fn velocities_valid(linear: Vec3, angular: Vec3) -> bool {
    let valid = |value: Vec3| {
        value
            .to_array()
            .iter()
            .all(|component| component.abs() < MAX_VELOCITY_COMPONENT)
    };
    valid(linear) && valid(angular)
}

/// Scales a vector down to `max` length (`v · (1/|v|) · max`).
fn clamp_length(value: Vec3, max: f32) -> Vec3 {
    let length_squared = value.length_squared();
    if length_squared > max * max {
        value * ((1.0 / length_squared.sqrt()) * max)
    } else {
        value
    }
}

/// Like [`clamp_length`], but a vector with a NaN length becomes [`RESET_VELOCITY`].
fn clamp_or_reset(value: Vec3, max: f32) -> Vec3 {
    let length_squared = value.length_squared();
    if length_squared.is_nan() {
        return RESET_VELOCITY;
    }
    clamp_length(value, max)
}

/// Steps the centre of mass and the rotation with the given velocities. The linear velocity is
/// clamped to the maximum speed and the angular velocity to the rotation cap, in place.
fn integrate_transform(body: &mut Body, linear: &mut Vec3, angular: &mut Vec3, dt: f32) {
    *linear = clamp_length(*linear, decode_ufloat8(body.max_linear_velocity));
    body.center_of_mass += *linear * dt;

    let mut delta = *angular * (dt * 0.5);
    let mut max_angle = dt * decode_ufloat8(body.max_angular_velocity);
    if max_angle > MAX_ROTATION_HALF_TURNS {
        max_angle = MAX_ROTATION_HALF_TURNS;
    }
    let mut x = delta.length_squared() * HALF_TURNS_PER_RADIAN_SQUARED;
    let max_squared = max_angle * max_angle;
    if max_squared < x {
        let factor = (1.0 / x.sqrt()) * max_angle;
        *angular *= factor;
        delta *= factor;
        x = max_squared;
    }
    // The polynomial for cos(half angle) of `hkSweptTransformUtil`.
    let w = 1.0 - x * 0.822_948 - (x * x) * 0.130_529 - (x * 0.044_408) * (x * x);
    body.rotation = multiply_and_normalize(delta, w, body.rotation);
}

/// Returns `normalize(dq · q)` with `dq = (delta, w)`, in the operation order of the binary.
fn multiply_and_normalize(delta: Vec3, w: f32, q: Quat) -> Quat {
    let qv = Vec3::new(q.x, q.y, q.z);
    let cross = Vec3::new(
        q.z * delta.y - q.y * delta.z,
        q.x * delta.z - q.z * delta.x,
        q.y * delta.x - q.x * delta.y,
    );
    let xyz = cross + qv * w + delta * q.w;
    let dot = qv.x * delta.x + qv.y * delta.y + qv.z * delta.z;
    let qw = q.w * w - dot;
    let length_squared = (xyz.x * xyz.x + xyz.z * xyz.z) + (xyz.y * xyz.y + qw * qw);
    let inverse = 1.0 / length_squared.sqrt();
    // One Newton step on the exact inverse square root.
    let refined = 0.5 * inverse * (3.0 - length_squared * inverse * inverse);
    Quat::from_xyzw(
        xyz.x * refined,
        xyz.y * refined,
        xyz.z * refined,
        qw * refined,
    )
}
