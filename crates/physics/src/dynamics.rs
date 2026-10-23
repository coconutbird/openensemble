//! The dynamics of the Havok 5.1 world: integration, the contact solver, constraints and
//! deactivation.
//!
//! The code follows the discrete single-threaded Havok step as Halo Wars DE ships it:
//!
//! 1. `hkpSimulation_integrate` (`0x140884870`): every active island either runs the solver
//!    (`hkpSolveIsland_singleThreaded`, `0x140899D00`) when it has constraints, or the direct
//!    motion step (`hkRigidMotionUtil_applyForcesAndStep_deactivation`, `0x1408974B0`).
//! 2. An island whose bodies all passed more than 5 deactivation checks goes to sleep.
//!
//! The world then runs the collision detection and processes the new contact points.

mod contact;
mod deactivation;
mod hinge;
mod integrate;
mod island;
mod mass;
mod material;
mod new_points;
mod solver;

use glam::Vec3;

pub(crate) use hinge::Hinge;
pub(crate) use island::step_islands;
pub(crate) use mass::{MassProperties, shape_volume_mass_properties};
pub(crate) use material::{decode_ufloat8, encode_ufloat8};
pub(crate) use new_points::process_new_points;

/// The solver deactivation constants of one class (`hkpSolverInfo::DeactivationInfo`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeactivationInfo {
    pub(crate) linear_velocity_threshold_inv: f32,
    pub(crate) angular_velocity_threshold_inv: f32,
    pub(crate) slow_object_velocity_multiplier: f32,
    pub(crate) relative_sleep_velocity_threshold: f32,
    pub(crate) max_distance_squared: [f32; 2],
    /// Stored as `hkHalf` (the top 16 bits of the `f32`).
    pub(crate) max_rotation_squared: [f32; 2],
}

impl DeactivationInfo {
    /// The constants of `DEACTIVATION_CLASS_LOW`, the class of every Halo Wars body
    /// (`hkpWorld.cpp:2678-2751`).
    pub(crate) fn low(gravity_length: f32, inv_num_steps: f32, reference_distance: f32) -> Self {
        const EXPECTED_DELTA_TIME: f32 = 0.016;
        let gravity = if gravity_length == 0.0 {
            9.81
        } else {
            gravity_length
        };
        let average_object_size = gravity * 0.1;
        let (relative_velocity, relative_deceleration) = (0.01_f32, 0.08_f32);
        let velocity_threshold = gravity * relative_velocity;
        let deceleration = gravity * relative_deceleration / velocity_threshold;
        let q = reference_distance;
        // `m_maxRotSqrd` is an `hkHalf` array, so each store truncates.
        let rotation_0 = to_half(to_half(q * 2.0) * to_half(q * 2.0));
        Self {
            linear_velocity_threshold_inv: 1.0 / velocity_threshold,
            angular_velocity_threshold_inv: 1.0 / (average_object_size * velocity_threshold),
            slow_object_velocity_multiplier: 1.0
                - EXPECTED_DELTA_TIME * inv_num_steps * deceleration,
            relative_sleep_velocity_threshold: EXPECTED_DELTA_TIME * inv_num_steps
                / relative_deceleration,
            max_distance_squared: [q * q, q * 4.0 * (q * 4.0)],
            // Havok multiplies the stored first value by 8q, not by itself (`:2750`).
            max_rotation_squared: [rotation_0, to_half(rotation_0 * to_half(q * 8.0))],
        }
    }
}

/// Rounds a float to `hkHalf` (keeps the top 16 bits).
fn to_half(value: f32) -> f32 {
    f32::from_bits(value.to_bits() & 0xffff_0000)
}

/// The solver and step constants of one Havok step (`hkpSolverInfo` and
/// `hkpConstraintQueryIn`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StepInfo {
    /// The step length (`hkStepInfo::m_deltaTime`).
    pub(crate) delta_time: f32,
    /// `1 / delta_time`, or 0 for a zero step.
    pub(crate) inv_delta_time: f32,
    /// The solver substep: `delta_time / num_steps`.
    pub(crate) substep_delta_time: f32,
    /// `num_steps / delta_time`.
    pub(crate) substep_inv_delta_time: f32,
    pub(crate) num_steps: u32,
    pub(crate) inv_num_steps: f32,
    pub(crate) tau: f32,
    pub(crate) damping: f32,
    /// `tau / damping`: the factor that turns velocity steps into the sum velocity.
    pub(crate) tau_div_damp: f32,
    /// `damping / tau`.
    pub(crate) damp_div_tau: f32,
    /// `damping / (tau / 2)`.
    pub(crate) damp_div_friction_tau: f32,
    /// `tau / damping / substep`.
    pub(crate) rhs_factor: f32,
    /// `tau / 2 / damping / substep`.
    pub(crate) friction_rhs_factor: f32,
    /// The gravity times the step length.
    pub(crate) gravity_per_step: Vec3,
    /// The gravity times the substep length.
    pub(crate) gravity_per_substep: Vec3,
    /// The constants of the deactivation class of all bodies.
    pub(crate) deactivation: DeactivationInfo,
}

/// The world settings that the dynamics need.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DynamicsConfig {
    pub(crate) gravity: Vec3,
    pub(crate) solver_tau: f32,
    pub(crate) solver_damping: f32,
    pub(crate) solver_iterations: u32,
    pub(crate) deactivation_reference_distance: f32,
    pub(crate) enable_deactivation: bool,
}

impl DynamicsConfig {
    /// Builds the constants of a step of `delta_time` seconds (`hkpSimulation_integrate` and
    /// `hkpConstraintQueryIn::set`).
    pub(crate) fn step_info(&self, delta_time: f32) -> StepInfo {
        let num_steps = self.solver_iterations.max(1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "the iteration count is small, so the conversion is exact"
        )]
        let steps = num_steps as f32;
        let inv_num_steps = 1.0 / steps;
        let substep_delta_time = inv_num_steps * delta_time;
        let inv_delta_time = if delta_time == 0.0 {
            0.0
        } else {
            1.0 / delta_time
        };
        let substep_inv_delta_time = steps * inv_delta_time;
        let tau = self.solver_tau;
        let damping = self.solver_damping;
        let friction_tau = 0.5 * tau;
        StepInfo {
            delta_time,
            inv_delta_time,
            substep_delta_time,
            substep_inv_delta_time,
            num_steps,
            inv_num_steps,
            tau,
            damping,
            tau_div_damp: tau / damping,
            damp_div_tau: damping / tau,
            damp_div_friction_tau: damping / friction_tau,
            rhs_factor: tau / damping * substep_inv_delta_time,
            friction_rhs_factor: friction_tau / damping * substep_inv_delta_time,
            gravity_per_step: self.gravity * delta_time,
            gravity_per_substep: self.gravity * substep_delta_time,
            deactivation: DeactivationInfo::low(
                self.gravity.length(),
                inv_num_steps,
                self.deactivation_reference_distance,
            ),
        }
    }
}

#[cfg(test)]
mod tests;
