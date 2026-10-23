//! The Gauss-Seidel solver (`hkSolver_solveStepsSchemas`, `0x140DAD6C0`).
//!
//! The solver works on velocity accumulators (`hkpVelocityAccumulator`): the velocity of each
//! body plus a "sum velocity" that integrates the position. Each of the `num_steps` steps runs
//! every row once, then updates the accumulators:
//!
//! - solver deactivation slows or stops slow dynamic bodies;
//! - `d = v − s`, `s += (tau / damping) · d`;
//! - on the last step `s` becomes the mean velocity (`s · invNumSteps · damping / tau`) and `v`
//!   becomes the real velocity `d`; on the other steps `v = d + s` plus the gravity of a
//!   substep for dynamic bodies.
//!
//! Angular velocities and angular jacobians of dynamic bodies are in body space, where the
//! inverse inertia is diagonal.

use glam::{Mat3, Vec3};

use super::{StepInfo, integrate::damping_factor};
use crate::body::{Body, MotionType};

/// The constant that keeps effective masses finite (`dword_1411F3B3C`, `FLT_EPSILON`).
pub(crate) const MASS_EPSILON: f32 = 1.192_092_9e-7;

/// The kind of an accumulator (`hkpVelocityAccumulator::m_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccumulatorKind {
    /// Type 0: a dynamic body. Gravity and solver deactivation apply.
    Dynamic,
    /// Type 1: a fixed or keyframed body with zero inverse mass.
    Static,
}

/// The solver state of one body (`hkpVelocityAccumulator`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Accumulator {
    pub(crate) kind: AccumulatorKind,
    /// The rotation from body space to world space. The identity for static accumulators,
    /// whose angular values are in world space.
    pub(crate) rotation: Mat3,
    /// The world centre of mass at the start of the step.
    pub(crate) center_of_mass: Vec3,
    pub(crate) linear_velocity: Vec3,
    /// The angular velocity in accumulator space.
    pub(crate) angular_velocity: Vec3,
    /// The diagonal inverse inertia in accumulator space.
    pub(crate) inverse_inertia: Vec3,
    pub(crate) inverse_mass: f32,
    pub(crate) sum_linear: Vec3,
    pub(crate) sum_angular: Vec3,
}

impl Accumulator {
    /// The accumulator of a fixed body or of a body outside the island: all zero.
    pub(crate) const FIXED: Self = Self {
        kind: AccumulatorKind::Static,
        rotation: Mat3::IDENTITY,
        center_of_mass: Vec3::ZERO,
        linear_velocity: Vec3::ZERO,
        angular_velocity: Vec3::ZERO,
        inverse_inertia: Vec3::ZERO,
        inverse_mass: 0.0,
        sum_linear: Vec3::ZERO,
        sum_angular: Vec3::ZERO,
    };

    /// Builds the accumulator of a body (`hkpSolveIsland_buildAccumulators`, `0x1408983C0`).
    ///
    /// For a dynamic body this first applies the damping of the step to the body velocity.
    pub(crate) fn build(body: &mut Body, step: &StepInfo) -> Self {
        match body.motion {
            MotionType::BoxInertia => {
                body.linear_velocity *= damping_factor(step.delta_time, body.linear_damping);
                body.angular_velocity *= damping_factor(step.delta_time, body.angular_damping);
                let rotation = body.rotation_matrix();
                Self {
                    kind: AccumulatorKind::Dynamic,
                    rotation,
                    center_of_mass: body.center_of_mass,
                    linear_velocity: body.linear_velocity,
                    angular_velocity: rotation.transpose() * body.angular_velocity,
                    inverse_inertia: body.inverse_inertia_local(),
                    inverse_mass: body.inverse_mass(),
                    sum_linear: Vec3::ZERO,
                    sum_angular: Vec3::ZERO,
                }
            }
            MotionType::Keyframed => Self {
                center_of_mass: body.center_of_mass,
                linear_velocity: body.linear_velocity,
                angular_velocity: body.angular_velocity,
                ..Self::FIXED
            },
            MotionType::Fixed => Self {
                center_of_mass: body.center_of_mass,
                ..Self::FIXED
            },
        }
    }

    /// Rotates a world vector into accumulator space.
    pub(crate) fn local_vector(&self, world: Vec3) -> Vec3 {
        match self.kind {
            AccumulatorKind::Dynamic => self.rotation.transpose() * world,
            AccumulatorKind::Static => world,
        }
    }
}

/// The inverse masses of one side of a constraint, scaled by the response modifier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InverseMass {
    pub(crate) inertia: Vec3,
    pub(crate) mass: f32,
}

impl InverseMass {
    pub(crate) fn of(accumulator: &Accumulator, scale: f32) -> Self {
        Self {
            inertia: accumulator.inverse_inertia * scale,
            mass: accumulator.inverse_mass * scale,
        }
    }
}

/// A row with a linear part along `direction` and angular parts for both bodies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Jacobian {
    /// The linear direction. It acts on A positively and on B negatively.
    pub(crate) direction: Vec3,
    pub(crate) angular_a: Vec3,
    pub(crate) angular_b: Vec3,
}

impl Jacobian {
    /// `J · v` with Havok's sum order: `z + (y + x)` of
    /// `angA·ωA + angB·ωB + n·(vA − vB)`.
    pub(crate) fn velocity(&self, a: &Accumulator, b: &Accumulator) -> f32 {
        let value = self.angular_a * a.angular_velocity
            + self.angular_b * b.angular_velocity
            + self.direction * (a.linear_velocity - b.linear_velocity);
        value.z + (value.y + value.x)
    }

    /// `J · s`, the same product with the sum velocities.
    pub(crate) fn sum_velocity(&self, a: &Accumulator, b: &Accumulator) -> f32 {
        let value = self.angular_a * a.sum_angular
            + self.angular_b * b.sum_angular
            + self.direction * (a.sum_linear - b.sum_linear);
        value.z + (value.y + value.x)
    }

    /// The inverse effective mass `J M⁻¹ Jᵀ + ε`, summed like `shufps 4E/B1`.
    pub(crate) fn inverse_effective_mass(&self, a: InverseMass, b: InverseMass) -> f32 {
        let lanes = self.angular_a * self.angular_a * a.inertia
            + self.angular_b * self.angular_b * b.inertia;
        let linear = a.mass + b.mass;
        (lanes.x + lanes.z) + (lanes.y + linear) + MASS_EPSILON
    }

    /// The coupling `J₁ M⁻¹ J₂ᵀ` of two rows (without ε).
    pub(crate) fn coupling(&self, other: &Self, a: InverseMass, b: InverseMass) -> f32 {
        let products = self.direction * other.direction;
        let value = other.angular_b * self.angular_b * b.inertia
            + other.angular_a * self.angular_a * a.inertia
            + (products * b.mass + products * a.mass);
        value.z + (value.y + value.x)
    }

    /// Applies an impulse `Δ` along the row.
    pub(crate) fn apply(
        &self,
        impulse: f32,
        a: &mut Accumulator,
        b: &mut Accumulator,
        mass_a: InverseMass,
        mass_b: InverseMass,
    ) {
        a.linear_velocity += self.direction * (impulse * mass_a.mass);
        b.linear_velocity -= self.direction * (impulse * mass_b.mass);
        a.angular_velocity += mass_a.inertia * impulse * self.angular_a;
        b.angular_velocity += mass_b.inertia * impulse * self.angular_b;
    }
}

/// An angular-only row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AngularJacobian {
    pub(crate) angular_a: Vec3,
    pub(crate) angular_b: Vec3,
}

impl AngularJacobian {
    pub(crate) fn velocity(&self, a: &Accumulator, b: &Accumulator) -> f32 {
        let value = self.angular_a * a.angular_velocity + self.angular_b * b.angular_velocity;
        value.z + (value.y + value.x)
    }

    pub(crate) fn sum_velocity(&self, a: &Accumulator, b: &Accumulator) -> f32 {
        let value = self.angular_a * a.sum_angular + self.angular_b * b.sum_angular;
        value.z + (value.y + value.x)
    }

    /// `J M⁻¹ Jᵀ` without ε and without the linear masses.
    pub(crate) fn inverse_effective_mass(&self, a: InverseMass, b: InverseMass) -> f32 {
        let value = self.angular_a * self.angular_a * a.inertia
            + self.angular_b * self.angular_b * b.inertia;
        value.z + (value.y + value.x)
    }

    pub(crate) fn apply(
        &self,
        impulse: f32,
        a: &mut Accumulator,
        b: &mut Accumulator,
        mass_a: InverseMass,
        mass_b: InverseMass,
    ) {
        a.angular_velocity += mass_a.inertia * impulse * self.angular_a;
        b.angular_velocity += mass_b.inertia * impulse * self.angular_b;
    }
}

/// The accumulated result of a row (`hkpSolverResults` as the solver writes it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RowResult {
    /// The sum of the impulses of all iterations.
    pub(crate) impulse: f32,
    /// The last friction clamp ratio (1 when the friction did not slip).
    pub(crate) ratio: f32,
}

impl Default for RowResult {
    fn default() -> Self {
        Self {
            impulse: 0.0,
            ratio: 1.0,
        }
    }
}

/// Returns mutable references to two different accumulators. `b` may equal `a` only for
/// the shared fixed accumulator, which then gets a copy.
pub(crate) fn pair_mut(
    accumulators: &mut [Accumulator],
    a: usize,
    b: usize,
) -> (&mut Accumulator, &mut Accumulator) {
    debug_assert_ne!(a, b, "a constraint needs two accumulators");
    if a < b {
        let (low, high) = accumulators.split_at_mut(b);
        (&mut low[a], &mut high[0])
    } else {
        let (low, high) = accumulators.split_at_mut(a);
        (&mut high[0], &mut low[b])
    }
}

/// Runs the end-of-step update of every accumulator (the tail of `hkSolver_solveStepsSchemas`).
pub(crate) fn end_step(accumulators: &mut [Accumulator], step: &StepInfo, last: bool) {
    let info = &step.deactivation;
    let average = step.inv_num_steps * step.damp_div_tau;
    for accumulator in accumulators {
        let mut gravity = Vec3::ZERO;
        if accumulator.kind == AccumulatorKind::Dynamic {
            gravity = step.gravity_per_substep;
            let finite = |value: Vec3| {
                value
                    .to_array()
                    .iter()
                    .all(|component| component.abs() < 1e6)
            };
            if !(finite(accumulator.linear_velocity) && finite(accumulator.angular_velocity)) {
                accumulator.linear_velocity = Vec3::X;
                accumulator.angular_velocity = Vec3::X;
            }
            let normalized = accumulator.linear_velocity.abs() * info.linear_velocity_threshold_inv
                + accumulator.angular_velocity.abs() * info.angular_velocity_threshold_inv;
            if normalized.cmple(Vec3::ONE).all() {
                if normalized
                    .cmple(Vec3::splat(info.relative_sleep_velocity_threshold))
                    .all()
                {
                    accumulator.linear_velocity = Vec3::ZERO;
                    accumulator.angular_velocity = Vec3::ZERO;
                } else {
                    accumulator.linear_velocity *= info.slow_object_velocity_multiplier;
                    accumulator.angular_velocity *= info.slow_object_velocity_multiplier;
                }
            }
        }
        let linear = accumulator.linear_velocity - accumulator.sum_linear;
        let angular = accumulator.angular_velocity - accumulator.sum_angular;
        accumulator.sum_linear += linear * step.tau_div_damp;
        accumulator.sum_angular += angular * step.tau_div_damp;
        if last {
            accumulator.sum_linear *= average;
            accumulator.sum_angular *= average;
            accumulator.linear_velocity = linear;
            accumulator.angular_velocity = angular;
        } else {
            accumulator.linear_velocity = linear + accumulator.sum_linear + gravity;
            accumulator.angular_velocity = angular + accumulator.sum_angular;
        }
    }
}

/// Adds the gravity of one substep to the dynamic accumulators and clears the sums (the start
/// of `hkSolver_solveStepsSchemas`).
pub(crate) fn begin_solve(accumulators: &mut [Accumulator], step: &StepInfo) {
    for accumulator in accumulators {
        accumulator.sum_linear = Vec3::ZERO;
        accumulator.sum_angular = Vec3::ZERO;
        if accumulator.kind == AccumulatorKind::Dynamic {
            accumulator.linear_velocity += step.gravity_per_substep;
        }
    }
}
