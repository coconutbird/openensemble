//! The limited hinge constraint (`hkpLimitedHingeConstraintData`) as retail builds it for the
//! clamshell ragdolls (`BClamshellPhysicsObject_setupHingeConstraints`, `0x1407B7AB0`).
//!
//! The atoms run in this order (`hkpConstraintAtoms_buildJacobian`, `0x140DB6450`):
//!
//! 1. Angular friction about the hinge axis (`hk1dAngularFriction_buildJacobian`,
//!    `0x140DBC3D0`, schema 0x0D).
//! 2. The angle limit about the hinge axis (atom case 14, schema 0x0C).
//! 3. Two angular rows that keep the axes aligned (`hk1dAngularBilateral_buildJacobian`,
//!    `0x140DBC0B0`, schema 0x0B).
//! 4. Three linear rows that join the pivots (`hkBallSocket_buildJacobian`, `0x140DB8EB0`,
//!    schema 5).
//!
//! The disabled motor atom adds no row.

use glam::{Mat3, Vec3};

use super::StepInfo;
use super::solver::{
    Accumulator, AngularJacobian, InverseMass, Jacobian, MASS_EPSILON, RowResult, pair_mut,
};
use crate::body::BodyId;
use crate::math::Transform;

/// A limited hinge between two bodies.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hinge {
    pub(crate) body_a: BodyId,
    pub(crate) body_b: BodyId,
    /// The constraint space in body A: column 0 is the hinge axis, columns 1 and 2 are
    /// perpendicular, the translation is the pivot.
    pub(crate) frame_a: Transform,
    /// The constraint space in body B.
    pub(crate) frame_b: Transform,
    pub(crate) min_angle: f32,
    pub(crate) max_angle: f32,
    /// `m_angularLimitsTauFactor` (default 1).
    pub(crate) limit_tau_factor: f32,
    pub(crate) max_friction_torque: f32,
    /// The friction drift that the solver exports (`m_internalSolverData` of the friction).
    friction_drift: f32,
    /// The exported limit data; its negation is the current angle.
    limit_internal: f32,
}

impl Hinge {
    /// `setInWorldSpace`: builds both constraint spaces from a world pivot and a world axis.
    pub(crate) fn in_world_space(
        (body_a, transform_a): (BodyId, &Transform),
        (body_b, transform_b): (BodyId, &Transform),
        pivot: Vec3,
        axis: Vec3,
    ) -> Self {
        let axis = axis.normalize_or_zero();
        let perpendicular = perpendicular(axis).normalize_or_zero();
        let other = axis.cross(perpendicular);
        let local = |transform: &Transform| {
            let inverse = transform.inverse();
            Transform {
                rotation: Mat3::from_cols(
                    inverse.transform_vector(axis),
                    inverse.transform_vector(perpendicular),
                    inverse.transform_vector(other),
                ),
                translation: inverse.transform_point(pivot),
            }
        };
        Self {
            body_a,
            body_b,
            frame_a: local(transform_a),
            frame_b: local(transform_b),
            min_angle: -std::f32::consts::PI,
            max_angle: std::f32::consts::PI,
            limit_tau_factor: 1.0,
            max_friction_torque: 0.0,
            friction_drift: 0.0,
            limit_internal: 0.0,
        }
    }

    /// The current hinge angle as the solver last measured it (`Runtime::getCurrentPos`).
    pub(crate) fn angle(&self) -> f32 {
        -self.limit_internal
    }
}

/// `hkVector4Util::calculatePerpendicularVector`.
fn perpendicular(vector: Vec3) -> Vec3 {
    let (mut min, mut ok1, mut ok2) = (0, 1, 2);
    let mut a0 = vector.x.abs();
    let a1 = vector.y.abs();
    let a2 = vector.z.abs();
    if a1 < a0 {
        ok1 = 0;
        min = 1;
        a0 = a1;
    }
    if a2 < a0 {
        ok2 = min;
    }
    let mut out = Vec3::ZERO;
    out[ok1] = vector[ok2];
    out[ok2] = -vector[ok1];
    out
}

/// The rows of one hinge for one step.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HingeRows {
    pub(crate) hinge: usize,
    a: usize,
    b: usize,
    mass_a: InverseMass,
    mass_b: InverseMass,
    friction: Option<FrictionRow>,
    limit: LimitRow,
    angular: [AngularRow; 2],
    linear: [LinearRow; 3],
    results: [RowResult; 7],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FrictionRow {
    jacobian: AngularJacobian,
    rhs: f32,
    /// `1 / (J M⁻¹ Jᵀ + ε)`, without the damping.
    virtual_mass: f32,
    max_impulse: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LimitRow {
    jacobian: AngularJacobian,
    /// `−angle / substep`.
    rhs: f32,
    min_rhs: f32,
    max_rhs: f32,
    /// `tau · tau factor`.
    tau: f32,
    virtual_mass: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AngularRow {
    jacobian: AngularJacobian,
    rhs: f32,
    virtual_mass: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LinearRow {
    jacobian: Jacobian,
    rhs: f32,
    virtual_mass: f32,
}

/// Builds the rows of a hinge from the body transforms at the start of the step.
pub(crate) fn build(
    hinge: &Hinge,
    hinge_index: usize,
    (transform_a, transform_b): (&Transform, &Transform),
    accumulators: &[Accumulator],
    (a, b): (usize, usize),
    step: &StepInfo,
) -> HingeRows {
    let (acc_a, acc_b) = (&accumulators[a], &accumulators[b]);
    let mass_a = InverseMass::of(acc_a, 1.0);
    let mass_b = InverseMass::of(acc_b, 1.0);
    let frame_a = *transform_a * hinge.frame_a;
    let frame_b = *transform_b * hinge.frame_b;
    let axis_a = frame_a.rotation.x_axis;
    let angular = |world: Vec3| AngularJacobian {
        angular_a: acc_a.local_vector(world),
        angular_b: acc_b.local_vector(-world),
    };

    let friction = (hinge.max_friction_torque != 0.0).then(|| {
        let jacobian = angular(axis_a);
        FrictionRow {
            jacobian,
            rhs: hinge.friction_drift * step.substep_inv_delta_time,
            virtual_mass: 1.0 / (jacobian.inverse_effective_mass(mass_a, mass_b) + MASS_EPSILON),
            max_impulse: hinge.max_friction_torque * step.substep_delta_time,
        }
    });

    let limit = {
        let measured = atan2_approximation(
            frame_a.rotation.y_axis.dot(frame_b.rotation.z_axis),
            frame_a.rotation.y_axis.dot(frame_b.rotation.y_axis),
        );
        let angle = unwrap_angle(measured, hinge.angle());
        let tau = step.tau * hinge.limit_tau_factor;
        let jacobian = angular(axis_a);
        let scale = -step.substep_inv_delta_time;
        LimitRow {
            jacobian,
            rhs: angle * scale,
            min_rhs: hinge.min_angle * scale * tau,
            max_rhs: hinge.max_angle * scale * tau,
            tau,
            virtual_mass: tau / (jacobian.inverse_effective_mass(mass_a, mass_b) + MASS_EPSILON),
        }
    };

    let aligned = |row_axis: Vec3, error_axis: Vec3| {
        let jacobian = angular(row_axis);
        AngularRow {
            jacobian,
            rhs: -error_axis.dot(axis_a) * step.rhs_factor,
            virtual_mass: step.damping
                / (jacobian.inverse_effective_mass(mass_a, mass_b) + MASS_EPSILON),
        }
    };
    let angular_rows = [
        aligned(frame_b.rotation.z_axis, frame_b.rotation.y_axis),
        aligned(-frame_b.rotation.y_axis, frame_b.rotation.z_axis),
    ];

    let pivot_a = frame_a.translation;
    let pivot_b = frame_b.translation;
    let linear = [Vec3::X, Vec3::Y, Vec3::Z].map(|direction| {
        let jacobian = Jacobian {
            direction,
            angular_a: acc_a.local_vector((pivot_a - acc_a.center_of_mass).cross(direction)),
            angular_b: acc_b.local_vector(-(pivot_b - acc_b.center_of_mass).cross(direction)),
        };
        LinearRow {
            jacobian,
            rhs: (pivot_b - pivot_a).dot(direction) * step.rhs_factor,
            virtual_mass: step.damping / jacobian.inverse_effective_mass(mass_a, mass_b),
        }
    });

    HingeRows {
        hinge: hinge_index,
        a,
        b,
        mass_a,
        mass_b,
        friction,
        limit,
        angular: angular_rows,
        linear,
        results: [RowResult::default(); 7],
    }
}

/// Moves an angle by whole turns until it is within π of the previous angle.
fn unwrap_angle(mut angle: f32, previous: f32) -> f32 {
    let turn = std::f32::consts::TAU;
    let half = std::f32::consts::PI;
    while previous - angle > half {
        angle += turn;
    }
    while angle - previous > half {
        angle -= turn;
    }
    angle
}

/// `hkMath_atan2Approximation` (`0x14086B450`): `atan2(y, x)` with a cubic polynomial.
fn atan2_approximation(y: f32, x: f32) -> f32 {
    const EPSILON: f32 = 1.192_092_9e-7;
    let polynomial = |t: f32| (t - (t * 0.121_079) * t) - ((t * 0.093_522_82) * t) * t;
    let (abs_y, abs_x) = (y.abs(), x.abs());
    let mut angle = if abs_x < abs_y {
        std::f32::consts::FRAC_PI_2 - polynomial(abs_x / (abs_y + EPSILON))
    } else {
        polynomial(abs_y / (abs_x + EPSILON))
    };
    if x < 0.0 {
        angle = std::f32::consts::PI - angle;
    }
    if y < 0.0 {
        angle = -angle;
    }
    angle
}

impl HingeRows {
    /// Runs one Gauss-Seidel pass over the rows of the hinge.
    pub(crate) fn solve(&mut self, accumulators: &mut [Accumulator], step: &StepInfo) {
        let (a, b) = pair_mut(accumulators, self.a, self.b);
        let (mass_a, mass_b) = (self.mass_a, self.mass_b);
        if let Some(friction) = &self.friction {
            let mut impulse =
                (friction.rhs - friction.jacobian.velocity(a, b)) * friction.virtual_mass;
            if impulse.abs() <= friction.max_impulse {
                self.results[0].ratio = 1.0;
            } else {
                let ratio = friction.max_impulse / impulse.abs();
                self.results[0].ratio = ratio;
                impulse *= ratio;
            }
            friction.jacobian.apply(impulse, a, b, mass_a, mass_b);
            self.results[0].impulse += impulse;
        }

        let limit = &self.limit;
        let jacobian = limit.jacobian;
        let real = (jacobian.angular_a * (a.angular_velocity - a.sum_angular)
            + jacobian.angular_b * (b.angular_velocity - b.sum_angular))
            * step.damping
            + (jacobian.angular_a * a.sum_angular + jacobian.angular_b * b.sum_angular)
                * (limit.tau * step.damp_div_tau);
        let target = limit.tau * limit.rhs - (real.z + (real.y + real.x));
        let below = target - limit.min_rhs;
        let impulse = if below > 0.0 {
            Some(below * limit.virtual_mass)
        } else {
            let above = target - limit.max_rhs;
            (above < 0.0).then_some(above * limit.virtual_mass)
        };
        if let Some(impulse) = impulse {
            jacobian.apply(impulse, a, b, mass_a, mass_b);
            self.results[1].impulse += impulse;
        }

        for (index, row) in self.angular.iter().enumerate() {
            let impulse = (row.rhs - row.jacobian.velocity(a, b)) * row.virtual_mass;
            row.jacobian.apply(impulse, a, b, mass_a, mass_b);
            self.results[2 + index].impulse += impulse;
        }
        for (index, row) in self.linear.iter().enumerate() {
            let impulse = (row.rhs - row.jacobian.velocity(a, b)) * row.virtual_mass;
            row.jacobian.apply(impulse, a, b, mass_a, mass_b);
            self.results[4 + index].impulse += impulse;
        }
    }

    /// Writes the friction drift and the limit angle back into the hinge.
    pub(crate) fn export(&self, hinge: &mut Hinge, accumulators: &[Accumulator], step: &StepInfo) {
        let (a, b) = (&accumulators[self.a], &accumulators[self.b]);
        if let Some(friction) = &self.friction {
            hinge.friction_drift = (friction.rhs * step.substep_delta_time
                - friction.jacobian.sum_velocity(a, b) * step.delta_time)
                * self.results[0].ratio;
        }
        hinge.limit_internal = self.limit.rhs * step.substep_delta_time
            - self.limit.jacobian.sum_velocity(a, b) * step.delta_time;
    }
}
