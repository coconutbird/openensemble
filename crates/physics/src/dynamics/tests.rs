//! Tests of the dynamics against hand-built manifolds.

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::Vec3;

use super::solver::{Accumulator, AccumulatorKind, begin_solve, end_step};
use super::{DeactivationInfo, DynamicsConfig, step_islands};
use crate::body::{Body, BodyDesc, BodyId, MotionType};
use crate::contact::{ContactPoint, Manifold, ManifoldId};
use crate::shape::{DEFAULT_CONVEX_RADIUS, Shape};

fn retail_config() -> DynamicsConfig {
    DynamicsConfig {
        gravity: Vec3::new(0.0, -21.7, 0.0),
        solver_tau: 0.6,
        solver_damping: 1.0,
        solver_iterations: 4,
        deactivation_reference_distance: 0.02,
        enable_deactivation: true,
    }
}

const DT: f32 = 0.0125;

#[test]
fn low_deactivation_class_matches_the_retail_table() {
    let info = DeactivationInfo::low(21.7, 0.25, 0.02);
    assert!((info.linear_velocity_threshold_inv - 4.6083).abs() < 1e-3);
    assert!((info.angular_velocity_threshold_inv - 2.1236).abs() < 1e-3);
    assert!((info.slow_object_velocity_multiplier - 0.968).abs() < 1e-5);
    assert!((info.relative_sleep_velocity_threshold - 0.05).abs() < 1e-6);
    assert!((info.max_distance_squared[1] - 0.0064).abs() < 1e-7);
    // Each hkHalf store keeps the top 16 bits: half(half(0.04)²) and half(that · half(0.16)).
    let half = |value: f32| f32::from_bits(value.to_bits() & 0xffff_0000);
    let first = half(half(0.04) * half(0.04));
    assert_eq!(info.max_rotation_squared[0].to_bits(), first.to_bits());
    assert_eq!(
        info.max_rotation_squared[1].to_bits(),
        half(first * half(0.16)).to_bits()
    );
    assert!((info.max_rotation_squared[0] - 0.0016).abs() < 5e-5);
    assert!((info.max_rotation_squared[1] - 0.000_256).abs() < 1e-5);
}

#[test]
fn the_solver_steps_give_the_retail_free_fall_sums() {
    let step = retail_config().step_info(DT);
    let mut accumulators = [Accumulator {
        kind: AccumulatorKind::Dynamic,
        inverse_mass: 1.0,
        inverse_inertia: Vec3::ONE,
        // A fast body, so that solver deactivation does not slow it.
        linear_velocity: Vec3::new(50.0, 0.0, 0.0),
        ..Accumulator::FIXED
    }];
    begin_solve(&mut accumulators, &step);
    for iteration in 0..4 {
        end_step(&mut accumulators, &step, iteration == 3);
    }
    let g = -21.7 * DT;
    let [accumulator] = accumulators;
    assert!((accumulator.linear_velocity.y - g).abs() < 1e-6);
    assert!((accumulator.sum_linear.y - 0.625 * g).abs() < 1e-6);
    assert!((accumulator.linear_velocity.x - 50.0).abs() < 1e-4);
    assert!((accumulator.sum_linear.x - 50.0).abs() < 1e-4);
}

fn ground_and_box() -> BTreeMap<BodyId, Body> {
    let mut bodies = BTreeMap::new();
    let ground = BodyDesc::new(
        Arc::new(Shape::Sphere { radius: 1.0 }),
        MotionType::Fixed,
        Vec3::new(0.0, -10.0, 0.0),
    );
    let mut cube = BodyDesc::new(
        Arc::new(Shape::Box {
            half_extents: Vec3::splat(0.5),
            radius: DEFAULT_CONVEX_RADIUS,
        }),
        MotionType::BoxInertia,
        Vec3::new(0.0, 0.55, 0.0),
    );
    cube.mass = 10.0;
    cube.restitution = 0.0;
    cube.angular_damping = 0.0;
    for (index, desc) in [ground, cube].iter().enumerate() {
        let id = BodyId::from_raw(u32::try_from(index).expect("small"));
        bodies.insert(id, Body::new(id, desc));
    }
    bodies
}

const CORNERS: [(f32, f32); 4] = [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)];

/// The contact of a lower corner of the cube with the plane y = 0, as the heightfield agent
/// reports it (ground is body A, so the normal points down toward it).
fn corner_contact(cube: &Body, (x, z): (f32, f32)) -> ContactPoint {
    let centre = cube.transform().transform_point(Vec3::new(x, -0.5, z));
    ContactPoint {
        position: centre - Vec3::Y * DEFAULT_CONVEX_RADIUS,
        normal: Vec3::NEG_Y,
        distance: centre.y - DEFAULT_CONVEX_RADIUS,
    }
}

/// A manifold with the four lower corners as processed points.
fn corner_manifold(cube: &Body) -> Manifold {
    let mut manifold = Manifold::new(
        ManifoldId::from_raw(0),
        BodyId::from_raw(0),
        BodyId::from_raw(1),
    );
    for (key, corner) in (0..).zip(CORNERS) {
        let contact = corner_contact(cube, corner);
        let index = manifold.push_point(key, contact);
        let properties = &mut manifold.points[index].properties;
        properties.is_new = false;
        properties.friction_code = super::encode_ufloat8(1.0);
        properties.internal_data_a = contact.distance;
    }
    manifold
}

#[test]
fn a_box_held_by_corner_contacts_stays_at_rest_and_then_sleeps() {
    let mut bodies = ground_and_box();
    let step = retail_config().step_info(DT);
    let cube = BodyId::from_raw(1);
    let mut manifolds = [corner_manifold(&bodies[&cube])];
    let mut slept = None;
    for frame in 0..200 {
        step_islands(&mut bodies, &mut manifolds, &mut [], &step, true);
        // Move the points with the body and keep their solver data, like the agent.
        for (point, corner) in manifolds[0].points.iter_mut().zip(CORNERS) {
            point.contact = corner_contact(&bodies[&cube], corner);
        }
        if slept.is_none() && !bodies[&cube].active {
            slept = Some(frame);
        }
        if slept.is_some() {
            break;
        }
    }
    let height = bodies[&cube].position().y;
    assert!((height - 0.55).abs() < 0.01, "{height}");
    assert!(slept.is_some(), "the resting box sleeps");
}

#[test]
fn paired_points_follow_the_retail_add_and_remove_rules() {
    let mut manifold = Manifold::new(
        ManifoldId::from_raw(0),
        BodyId::from_raw(0),
        BodyId::from_raw(1),
    );
    for key in 0..4 {
        manifold.push_point(key, ContactPoint::default());
    }
    let paired: Vec<bool> = manifold
        .points
        .iter()
        .map(|p| p.properties.paired)
        .collect();
    assert_eq!(paired, [false, true, false, true]);
    manifold.remove_point(0);
    let paired: Vec<bool> = manifold
        .points
        .iter()
        .map(|p| p.properties.paired)
        .collect();
    assert_eq!(paired, [false, false, true]);
    assert!(manifold.solver.area_changed);
}
