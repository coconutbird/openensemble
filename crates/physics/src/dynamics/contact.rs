//! The contact rows of a manifold (`hkSimpleContactConstraintDataBuildJacobian`,
//! `0x140DB5800`), their solve schemas (0x0F, 0x13, 0x15, 0x16 in
//! `hkSolver_solveStepsSchemas`) and the export of their results
//! (`hkSolver_exportResultsSchemas`, `0x140DB4950`).

use glam::Vec3;

use super::solver::{
    Accumulator, AngularJacobian, InverseMass, Jacobian, MASS_EPSILON, RowResult, pair_mut,
};
use super::{StepInfo, decode_ufloat8};
use crate::contact::Manifold;

/// `HK_CONTACT_LINEAR_ERROR_RECOVERY_VELOCITY` (`0x1414A9B80`).
const LINEAR_RECOVERY_VELOCITY: f32 = 1.0;
/// `HK_CONTACT_EXPONENTIAL_ERROR_RECOVERY_VELOCITY` (`0x1414A9B84`).
const EXPONENTIAL_RECOVERY: f32 = 0.05;
/// The factor on the coupling of a pair of points (`dword_1411F3DC0`).
const PAIR_COUPLING_FACTOR: f32 = 0.99;

/// One normal row of a contact point.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PointRow {
    jacobian: Jacobian,
    rhs: f32,
    /// `damping / (J M⁻¹ Jᵀ + ε)`.
    virtual_mass: f32,
    /// `J M⁻¹ Jᵀ + ε`, kept for the pair matrix.
    inverse_effective_mass: f32,
}

/// The friction rows of a manifold.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FrictionRows {
    rows: [Jacobian; 2],
    rhs: [f32; 2],
    /// The inverse of the 2 × 2 effective mass matrix: `m00`, `m11`, `m01`.
    matrix: [f32; 3],
    max_impulse: f32,
    angular: Option<AngularFrictionRow>,
}

/// The angular friction row about the friction normal.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AngularFrictionRow {
    jacobian: AngularJacobian,
    rhs: f32,
    /// `damping / (J M⁻¹ Jᵀ + ε) / radius`.
    virtual_mass: f32,
    radius: f32,
}

/// The rows of one manifold, ready for the solver.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ManifoldRows {
    /// The index of the manifold in the manifold slice.
    pub(crate) manifold: usize,
    pub(crate) a: usize,
    pub(crate) b: usize,
    mass_a: InverseMass,
    mass_b: InverseMass,
    /// The normal rows in point order, with the pair matrix of a point that pairs with the
    /// previous point.
    points: Vec<(PointRow, Option<[f32; 3]>)>,
    friction: Option<FrictionRows>,
    /// The results: one per point, then three or four for the friction.
    results: Vec<RowResult>,
}

/// Builds the rows of a manifold with at least one point. Updates the allowed penetration and
/// the friction axis of the manifold.
pub(crate) fn build(
    manifold: &mut Manifold,
    manifold_index: usize,
    accumulators: &[Accumulator],
    (a, b): (usize, usize),
    step: &StepInfo,
) -> ManifoldRows {
    let (acc_a, acc_b) = (&accumulators[a], &accumulators[b]);
    let mass_a = InverseMass::of(acc_a, manifold.inv_mass_scale[0]);
    let mass_b = InverseMass::of(acc_b, manifold.inv_mass_scale[1]);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a manifold has few points, so the conversion is exact"
    )]
    let inv_count = 1.0 / manifold.points.len() as f32;
    let recovery_limit = LINEAR_RECOVERY_VELOCITY * step.delta_time;

    let mut points: Vec<(PointRow, Option<[f32; 3]>)> = Vec::with_capacity(manifold.points.len());
    let mut friction_sum = 0.0;
    let mut impulse_sum = 0.0;
    let mut position_sum = Vec3::ZERO;
    let mut normal_sum = Vec3::ZERO;
    for point in &mut manifold.points {
        let contact = point.contact;
        let jacobian = point_jacobian(contact.position, contact.normal, acc_a, acc_b);
        let inverse_effective_mass = jacobian.inverse_effective_mass(mass_a, mass_b);
        let properties = &mut point.properties;

        // Penetration recovery with the allowed penetration A and the exported error I.
        let allowed = properties.internal_data_a;
        let gap_error = contact.distance - allowed;
        let mut recovery = -(allowed * EXPONENTIAL_RECOVERY);
        if recovery >= recovery_limit {
            recovery = recovery_limit;
        }
        let mut new_allowed = allowed + recovery;
        let mut error = gap_error - recovery;
        let correction = -properties.internal_solver_data - gap_error;
        if correction > recovery * 2.0 + recovery_limit {
            new_allowed -= correction;
            error += correction;
        }
        properties.internal_data_a = if -MASS_EPSILON > new_allowed {
            new_allowed
        } else {
            -MASS_EPSILON
        };

        friction_sum += decode_ufloat8(properties.friction_code);
        impulse_sum += properties.impulse_applied;
        position_sum += contact.position;
        normal_sum += contact.normal;
        let row = PointRow {
            jacobian,
            rhs: -error * step.rhs_factor,
            virtual_mass: step.damping / inverse_effective_mass,
            inverse_effective_mass,
        };
        let pair = if properties.paired {
            points
                .last()
                .map(|(previous, _)| pair_matrix(previous, &row, mass_a, mass_b, step))
        } else {
            None
        };
        points.push((row, pair));
    }

    let friction_max = friction_sum * impulse_sum * inv_count;
    let friction = (friction_max > 0.0).then(|| {
        friction_rows(
            manifold,
            (acc_a, acc_b),
            (mass_a, mass_b),
            (position_sum * inv_count, normal_sum, inv_count),
            friction_max * step.inv_num_steps,
            step,
        )
    });
    let friction = friction.flatten();
    let friction_results = friction.map_or(0, |rows| if rows.angular.is_some() { 4 } else { 3 });
    let results = vec![RowResult::default(); points.len() + friction_results];
    ManifoldRows {
        manifold: manifold_index,
        a,
        b,
        mass_a,
        mass_b,
        points,
        friction,
        results,
    }
}

/// The normal row of a point: `[n, (p − cA) × n, −((p − cB) × n)]` in accumulator space.
fn point_jacobian(position: Vec3, normal: Vec3, a: &Accumulator, b: &Accumulator) -> Jacobian {
    Jacobian {
        direction: normal,
        angular_a: a.local_vector((position - a.center_of_mass).cross(normal)),
        angular_b: b.local_vector(-(position - b.center_of_mass).cross(normal)),
    }
}

/// The inverse 2 × 2 effective mass of a pair of points, with the coupling scaled by 0.99.
fn pair_matrix(
    first: &PointRow,
    second: &PointRow,
    mass_a: InverseMass,
    mass_b: InverseMass,
    step: &StepInfo,
) -> [f32; 3] {
    let coupling = first.jacobian.coupling(&second.jacobian, mass_a, mass_b) * PAIR_COUPLING_FACTOR;
    let (k11, k22) = (first.inverse_effective_mass, second.inverse_effective_mass);
    let inverse = step.damping / (k22 * k11 - coupling * coupling);
    [inverse * k22, inverse * k11, -(inverse * coupling)]
}

/// Builds the friction rows at the centroid of the points.
fn friction_rows(
    manifold: &mut Manifold,
    (acc_a, acc_b): (&Accumulator, &Accumulator),
    (mass_a, mass_b): (InverseMass, InverseMass),
    (centroid, normal_sum, inv_count): (Vec3, Vec3, f32),
    max_impulse: f32,
    step: &StepInfo,
) -> Option<FrictionRows> {
    let first_normal = manifold.points[0].contact.normal;
    let sum_squared = normal_sum.length_squared();
    let normal = if inv_count * inv_count * sum_squared > 0.9999 {
        first_normal
    } else if sum_squared > 0.1 {
        normal_sum * refined_inverse_length(sum_squared)
    } else if first_normal.length_squared() < 0.9 {
        // Retail builds zero rows with no friction; skip them.
        return None;
    } else {
        first_normal
    };

    let state = &mut manifold.solver;
    let mut tangent = axis(state.friction_axis).cross(normal);
    if tangent.length_squared() <= 0.1 {
        state.friction_axis = smallest_axis(normal);
        state.friction[1] = 0.0;
        state.friction[3] = 0.0;
        tangent = axis(state.friction_axis).cross(normal);
    }
    tangent *= refined_inverse_length(tangent.length_squared());
    let direction = normal.cross(tangent);

    let row = |dir: Vec3| Jacobian {
        direction: dir,
        angular_a: acc_a.local_vector((centroid - acc_a.center_of_mass).cross(dir)),
        angular_b: acc_b.local_vector(-(centroid - acc_b.center_of_mass).cross(dir)),
    };
    let rows = [row(direction), row(tangent)];
    let k00 = rows[0].inverse_effective_mass(mass_a, mass_b);
    let k11 = rows[1].inverse_effective_mass(mass_a, mass_b);
    let coupling = rows[0].coupling(&rows[1], mass_a, mass_b);
    let inverse = step.damping / (k11 * k00 - coupling * coupling);
    let matrix = [k11 * inverse, k00 * inverse, -(coupling * inverse)];
    let rhs = [
        state.friction[1] * step.friction_rhs_factor,
        state.friction[3] * step.friction_rhs_factor,
    ];

    let angular = if manifold.points.len() > 1 {
        angular_friction(
            manifold,
            (acc_a, acc_b),
            (mass_a, mass_b),
            centroid,
            normal,
            inv_count,
            step,
        )
    } else {
        None
    };
    Some(FrictionRows {
        rows,
        rhs,
        matrix,
        max_impulse,
        angular,
    })
}

/// Builds the angular friction row about the friction normal, if the points spread out.
fn angular_friction(
    manifold: &mut Manifold,
    (acc_a, acc_b): (&Accumulator, &Accumulator),
    (mass_a, mass_b): (InverseMass, InverseMass),
    centroid: Vec3,
    normal: Vec3,
    inv_count: f32,
    step: &StepInfo,
) -> Option<AngularFrictionRow> {
    if manifold.solver.area_changed {
        let distance_sum = manifold.points.iter().fold(0.0, |sum, point| {
            sum + (point.contact.position - centroid).length()
        });
        let radius = inv_count * distance_sum;
        manifold.solver.radius = radius;
        if 1e-6 > radius {
            return None;
        }
        manifold.solver.area_changed = false;
    }
    let radius = manifold.solver.radius;
    let jacobian = AngularJacobian {
        angular_a: acc_a.local_vector(normal),
        angular_b: acc_b.local_vector(-normal),
    };
    let inverse_effective_mass = jacobian.inverse_effective_mass(mass_a, mass_b) + MASS_EPSILON;
    Some(AngularFrictionRow {
        jacobian,
        rhs: manifold.solver.friction[5] * step.friction_rhs_factor,
        virtual_mass: (step.damping / inverse_effective_mass) * (1.0 / radius),
        radius,
    })
}

/// The unit world axis of an index; index 3 gives the zero vector.
fn axis(index: u8) -> Vec3 {
    match index {
        0 => Vec3::X,
        1 => Vec3::Y,
        2 => Vec3::Z,
        _ => Vec3::ZERO,
    }
}

/// The index of the smallest component of `|normal|`, with the tie rules of the binary.
fn smallest_axis(normal: Vec3) -> u8 {
    let abs = normal.abs();
    if abs.y > abs.x {
        if abs.z > abs.x { 0 } else { 2 }
    } else if abs.z <= abs.y {
        2
    } else {
        1
    }
}

/// `1/√x` refined with one Newton step, as the binary normalizes vectors.
fn refined_inverse_length(length_squared: f32) -> f32 {
    let inverse = 1.0 / length_squared.sqrt();
    0.5 * inverse * (3.0 - length_squared * inverse * inverse)
}

impl ManifoldRows {
    /// Runs one Gauss-Seidel pass over the rows of the manifold.
    pub(crate) fn solve(&mut self, accumulators: &mut [Accumulator]) {
        let (a, b) = pair_mut(accumulators, self.a, self.b);
        let (mass_a, mass_b) = (self.mass_a, self.mass_b);
        for index in 0..self.points.len() {
            let (row, pair) = self.points[index];
            if pair.is_some() {
                continue;
            }
            let next_pair = self
                .points
                .get(index + 1)
                .and_then(|(next, matrix)| matrix.map(|matrix| (next, matrix)));
            if let Some((second, matrix)) = next_pair {
                let (first_result, second_result) = (index, index + 1);
                let impulses = solve_pair(&row, second, matrix, a, b, mass_a, mass_b);
                self.results[first_result].impulse += impulses[0];
                self.results[second_result].impulse += impulses[1];
            } else {
                let residual = row.rhs - row.jacobian.velocity(a, b);
                if !residual.is_sign_negative() {
                    let impulse = residual * row.virtual_mass;
                    row.jacobian.apply(impulse, a, b, mass_a, mass_b);
                    self.results[index].impulse += impulse;
                }
            }
        }
        if let Some(friction) = &self.friction {
            let base = self.points.len();
            solve_friction(friction, &mut self.results[base..], a, b, mass_a, mass_b);
        }
    }

    /// Writes the impulses and the residual errors back into the manifold.
    pub(crate) fn export(
        &self,
        manifold: &mut Manifold,
        accumulators: &[Accumulator],
        step: &StepInfo,
    ) {
        let (a, b) = (&accumulators[self.a], &accumulators[self.b]);
        let rhs_scale = step.substep_delta_time * step.damp_div_tau;
        for (index, (row, _)) in self.points.iter().enumerate() {
            let properties = &mut manifold.points[index].properties;
            properties.impulse_applied = self.results[index].impulse;
            properties.internal_solver_data =
                row.rhs * rhs_scale - row.jacobian.sum_velocity(a, b) * step.delta_time;
        }
        let Some(friction) = &self.friction else {
            return;
        };
        let results = &self.results[self.points.len()..];
        let friction_scale = step.substep_delta_time * step.damp_div_friction_tau;
        let ratio = if friction.angular.is_some() {
            results[3].ratio
        } else {
            results[2].ratio
        };
        let state = &mut manifold.solver.friction;
        for row in 0..2 {
            state[row * 2] = results[row].impulse;
            state[row * 2 + 1] = (friction.rhs[row] * friction_scale
                - friction.rows[row].sum_velocity(a, b) * step.delta_time)
                * ratio;
        }
        if let Some(angular) = &friction.angular {
            state[4] = results[2].impulse;
            state[5] = ((angular.rhs * friction_scale)
                - angular.jacobian.sum_velocity(a, b) * step.delta_time)
                * ratio
                * ratio;
        }
    }
}

/// Solves a pair of points with the 2 × 2 matrix (schema 0x13). Returns both impulses.
fn solve_pair(
    first: &PointRow,
    second: &PointRow,
    [m00, m11, m01]: [f32; 3],
    a: &mut Accumulator,
    b: &mut Accumulator,
    mass_a: InverseMass,
    mass_b: InverseMass,
) -> [f32; 2] {
    let r1 = first.rhs - first.jacobian.velocity(a, b);
    let r2 = second.rhs - second.jacobian.velocity(a, b);
    let x1 = r1 * m00 + r2 * m01;
    let x2 = r2 * m11 + r1 * m01;
    let mut impulses = [0.0, 0.0];
    if x1 <= 0.0 {
        let alone = r2 * second.virtual_mass;
        if alone > 0.0 {
            second.jacobian.apply(alone, a, b, mass_a, mass_b);
            impulses[1] = alone;
            return impulses;
        }
    } else if x2 > 0.0 {
        first.jacobian.apply(x1, a, b, mass_a, mass_b);
        second.jacobian.apply(x2, a, b, mass_a, mass_b);
        return [x1, x2];
    }
    let alone = r1 * first.virtual_mass;
    if alone > 0.0 {
        first.jacobian.apply(alone, a, b, mass_a, mass_b);
        impulses[0] = alone;
    }
    impulses
}

/// Solves the friction rows (schemas 0x15 and 0x16) with the per-iteration clamp.
fn solve_friction(
    friction: &FrictionRows,
    results: &mut [RowResult],
    a: &mut Accumulator,
    b: &mut Accumulator,
    mass_a: InverseMass,
    mass_b: InverseMass,
) {
    let [m00, m11, m01] = friction.matrix;
    let r0 = friction.rhs[0] - friction.rows[0].velocity(a, b);
    let r1 = friction.rhs[1] - friction.rows[1].velocity(a, b);
    let mut x0 = r0 * m00 + r1 * m01;
    let mut x1 = r1 * m11 + r0 * m01;
    let max = friction.max_impulse;
    if let Some(angular) = &friction.angular {
        let mut x2 = (angular.rhs - angular.jacobian.velocity(a, b)) * angular.virtual_mass;
        let squared = (x1 * x1 + x0 * x0) + x2 * x2;
        if squared <= max * max {
            results[3].ratio = 1.0;
        } else {
            let ratio = (1.0 / squared.sqrt()) * max;
            results[3].ratio = ratio;
            x0 *= ratio;
            x1 *= ratio;
            x2 *= ratio;
        }
        friction.rows[0].apply(x0, a, b, mass_a, mass_b);
        results[0].impulse += x0;
        friction.rows[1].apply(x1, a, b, mass_a, mass_b);
        results[1].impulse += x1;
        let torque = x2 * angular.radius;
        angular.jacobian.apply(torque, a, b, mass_a, mass_b);
        results[2].impulse += torque;
    } else {
        let squared = x1 * x1 + x0 * x0;
        let limit = max * max;
        if squared <= limit {
            results[2].ratio = 1.0;
        } else {
            let ratio = (limit / squared).sqrt();
            results[2].ratio = ratio;
            x0 *= ratio;
            x1 *= ratio;
        }
        friction.rows[0].apply(x0, a, b, mass_a, mass_b);
        results[0].impulse += x0;
        friction.rows[1].apply(x1, a, b, mass_a, mass_b);
        results[1].impulse += x1;
    }
}
