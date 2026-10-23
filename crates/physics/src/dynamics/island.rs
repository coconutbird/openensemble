//! Simulation islands and the integration of one step (`hkpSimulation_integrate`,
//! `0x140884870`).
//!
//! An island is a set of dynamic bodies that manifolds or constraints join. Fixed and
//! keyframed bodies never join islands. The world finds the islands again at every step from
//! the current manifolds and constraints:
//!
//! - an island with an active body is active, so a contact wakes a sleeping island;
//! - an active island without constraints runs the direct motion step of each body;
//! - an active island with constraints runs the solver, then the motion step from the
//!   accumulators;
//! - an island sleeps when every body passed more than 5 deactivation checks in a row.

use std::collections::BTreeMap;

use super::StepInfo;
use super::contact::{self, ManifoldRows};
use super::deactivation::INACTIVE_CHECKS_TO_SLEEP;
use super::hinge::{self, Hinge, HingeRows};
use super::integrate::{step_direct, step_from_solver};
use super::new_points::apply_pending_restitution;
use super::solver::{Accumulator, begin_solve, end_step};
use crate::body::{Body, BodyId, MotionType};
use crate::contact::Manifold;

/// Integrates one step of every active island and puts resting islands to sleep.
pub(crate) fn step_islands(
    bodies: &mut BTreeMap<BodyId, Body>,
    manifolds: &mut [Manifold],
    hinges: &mut [Hinge],
    step: &StepInfo,
    enable_deactivation: bool,
) {
    let ids: Vec<BodyId> = bodies.keys().copied().collect();
    let mut bodies: Vec<&mut Body> = bodies.values_mut().collect();
    let index_of = |id: BodyId| ids.binary_search(&id).ok();

    let mut islands = Islands::new(&bodies);
    for manifold in manifolds.iter() {
        if let (Some(a), Some(b)) = (index_of(manifold.body_a), index_of(manifold.body_b)) {
            islands.join(a, b);
        }
    }
    for hinge in hinges.iter() {
        if let (Some(a), Some(b)) = (index_of(hinge.body_a), index_of(hinge.body_b)) {
            islands.join(a, b);
        }
    }

    for members in islands.groups(&bodies) {
        if !members.iter().any(|&member| bodies[member].active) {
            continue;
        }
        for &member in &members {
            let body = &mut *bodies[member];
            if !body.active {
                body.active = true;
                body.deactivation.inactive_frames = [0, 0];
            }
        }
        let root = islands.find(members[0]);
        let in_island =
            |index: Option<usize>| index.is_some_and(|i| islands.root_of(i) == Some(root));
        let constrained_manifolds: Vec<usize> = manifolds
            .iter()
            .enumerate()
            .filter(|(_, manifold)| {
                !manifold.points.is_empty()
                    && (in_island(index_of(manifold.body_a))
                        || in_island(index_of(manifold.body_b)))
            })
            .map(|(index, _)| index)
            .collect();
        let constrained_hinges: Vec<usize> = hinges
            .iter()
            .enumerate()
            .filter(|(_, hinge)| {
                in_island(index_of(hinge.body_a)) || in_island(index_of(hinge.body_b))
            })
            .map(|(index, _)| index)
            .collect();

        let checks = if constrained_manifolds.is_empty() && constrained_hinges.is_empty() {
            members
                .iter()
                .map(|&member| step_direct(bodies[member], step))
                .min()
        } else {
            let island = Constraints {
                manifolds: &constrained_manifolds,
                hinges: &constrained_hinges,
            };
            solve_island(
                &mut bodies,
                &members,
                &index_of,
                (manifolds, hinges),
                &island,
                step,
            )
        };
        if enable_deactivation && checks.is_some_and(|checks| checks > INACTIVE_CHECKS_TO_SLEEP) {
            for &member in &members {
                bodies[member].active = false;
            }
        }
    }
}

/// The constraints of one island: indices into the manifold and hinge slices.
struct Constraints<'a> {
    manifolds: &'a [usize],
    hinges: &'a [usize],
}

/// Runs the solver over an island and integrates its bodies. Returns the smallest count of
/// passed deactivation checks.
fn solve_island(
    bodies: &mut [&mut Body],
    members: &[usize],
    index_of: &dyn Fn(BodyId) -> Option<usize>,
    (manifolds, hinges): (&mut [Manifold], &mut [Hinge]),
    constraints: &Constraints<'_>,
    step: &StepInfo,
) -> Option<u8> {
    // Accumulators of the members first, then of the fixed and keyframed partners.
    let mut accumulators: Vec<Accumulator> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; bodies.len()];
    for &member in members {
        slot[member] = Some(accumulators.len());
        accumulators.push(Accumulator::build(bodies[member], step));
    }
    let mut accumulator_of =
        |index: usize, bodies: &mut [&mut Body], accumulators: &mut Vec<Accumulator>| {
            if let Some(existing) = slot[index] {
                return existing;
            }
            slot[index] = Some(accumulators.len());
            accumulators.push(Accumulator::build(bodies[index], step));
            accumulators.len() - 1
        };

    let mut manifold_rows: Vec<ManifoldRows> = Vec::with_capacity(constraints.manifolds.len());
    for &manifold_index in constraints.manifolds {
        let manifold = &mut manifolds[manifold_index];
        let (Some(a), Some(b)) = (index_of(manifold.body_a), index_of(manifold.body_b)) else {
            continue;
        };
        apply_pending_restitution(manifold, bodies[a], bodies[b], step);
        let accumulator_a = accumulator_of(a, bodies, &mut accumulators);
        let accumulator_b = accumulator_of(b, bodies, &mut accumulators);
        manifold_rows.push(contact::build(
            manifold,
            manifold_index,
            &accumulators,
            (accumulator_a, accumulator_b),
            step,
        ));
    }
    let mut hinge_rows: Vec<HingeRows> = Vec::with_capacity(constraints.hinges.len());
    for &hinge_index in constraints.hinges {
        let hinge = &hinges[hinge_index];
        let (Some(a), Some(b)) = (index_of(hinge.body_a), index_of(hinge.body_b)) else {
            continue;
        };
        let accumulator_a = accumulator_of(a, bodies, &mut accumulators);
        let accumulator_b = accumulator_of(b, bodies, &mut accumulators);
        let transforms = (bodies[a].transform(), bodies[b].transform());
        hinge_rows.push(hinge::build(
            hinge,
            hinge_index,
            (&transforms.0, &transforms.1),
            &accumulators,
            (accumulator_a, accumulator_b),
            step,
        ));
    }

    begin_solve(&mut accumulators, step);
    for iteration in 0..step.num_steps {
        for rows in &mut manifold_rows {
            rows.solve(&mut accumulators);
        }
        for rows in &mut hinge_rows {
            rows.solve(&mut accumulators, step);
        }
        end_step(&mut accumulators, step, iteration + 1 == step.num_steps);
    }
    for rows in &manifold_rows {
        rows.export(&mut manifolds[rows.manifold], &accumulators, step);
    }
    for rows in &hinge_rows {
        rows.export(&mut hinges[rows.hinge], &accumulators, step);
    }

    members
        .iter()
        .filter_map(|&member| {
            let accumulator = accumulators[slot[member]?];
            Some(step_from_solver(bodies[member], &accumulator, step))
        })
        .min()
}

/// A union-find over the moving bodies. Only dynamic bodies join; a keyframed body stays an
/// island of its own.
struct Islands {
    /// The parent of each body, or `None` for a fixed body.
    parent: Vec<Option<usize>>,
    /// `true` for each dynamic body.
    dynamic: Vec<bool>,
}

impl Islands {
    fn new(bodies: &[&mut Body]) -> Self {
        Self {
            parent: bodies
                .iter()
                .enumerate()
                .map(|(index, body)| (body.motion != MotionType::Fixed).then_some(index))
                .collect(),
            dynamic: bodies.iter().map(|body| body.is_dynamic()).collect(),
        }
    }

    fn find(&self, mut index: usize) -> usize {
        while let Some(parent) = self.parent[index] {
            if parent == index {
                break;
            }
            index = parent;
        }
        index
    }

    fn root_of(&self, index: usize) -> Option<usize> {
        self.parent[index].map(|_| self.find(index))
    }

    /// Joins the islands of two bodies if both are dynamic. The smaller root wins, so the
    /// result does not depend on the join order.
    fn join(&mut self, a: usize, b: usize) {
        if !(self.dynamic[a] && self.dynamic[b]) {
            return;
        }
        let (root_a, root_b) = (self.find(a), self.find(b));
        if root_a != root_b {
            let (low, high) = if root_a < root_b {
                (root_a, root_b)
            } else {
                (root_b, root_a)
            };
            self.parent[high] = Some(low);
        }
    }

    /// The islands in the order of their smallest body, each with its bodies in id order.
    fn groups(&self, bodies: &[&mut Body]) -> Vec<Vec<usize>> {
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for index in 0..bodies.len() {
            if let Some(root) = self.root_of(index) {
                groups.entry(root).or_default().push(index);
            }
        }
        groups.into_values().collect()
    }
}
