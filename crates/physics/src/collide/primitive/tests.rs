use glam::{Quat, Vec3};

use super::PrimitiveAgent;
use crate::body::BodyId;
use crate::collide::agent::{Agent, CdBody, CollisionInput};
use crate::collide::dispatch::convex_pair;
use crate::collide::sink::ContactSink;
use crate::contact::{ContactPoint, Manifold, ManifoldId};
use crate::math::Transform;
use crate::shape::Shape;

const TOLERANCE: f32 = 0.1;

/// One body pair with its agent and manifold.
struct Pair {
    agent: Agent,
    manifold: Manifold,
    next_key: u32,
    shape_a: Shape,
    shape_b: Shape,
}

impl Pair {
    fn new(shape_a: Shape, shape_b: Shape) -> Self {
        Self {
            agent: convex_pair(&shape_a, &shape_b),
            manifold: Manifold::new(
                ManifoldId::from_raw(0),
                BodyId::from_raw(0),
                BodyId::from_raw(1),
            ),
            next_key: 0,
            shape_a,
            shape_b,
        }
    }

    fn step(&mut self, transform_a: Transform, transform_b: Transform) {
        let a = CdBody {
            shape: &self.shape_a,
            transform: transform_a,
        };
        let b = CdBody {
            shape: &self.shape_b,
            transform: transform_b,
        };
        let mut sink = ContactSink::new(&mut self.manifold, &mut self.next_key);
        self.agent
            .process(&a, &b, &CollisionInput::new(TOLERANCE), &mut sink);
    }

    fn keys(&self) -> Vec<u32> {
        self.manifold.points.iter().map(|point| point.key).collect()
    }

    fn contact(&self, index: usize) -> ContactPoint {
        self.manifold.points[index].contact
    }
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        rotation: glam::Mat3::IDENTITY,
        translation: Vec3::new(x, y, z),
    }
}

fn assert_contact(actual: ContactPoint, position: Vec3, normal: Vec3, distance: f32) {
    assert!(
        (actual.position - position).abs().max_element() < 1e-5,
        "position {} != {position}",
        actual.position
    );
    assert!(
        (actual.normal - normal).abs().max_element() < 1e-5,
        "normal {} != {normal}",
        actual.normal
    );
    assert!(
        (actual.distance - distance).abs() < 1e-5,
        "distance {} != {distance}",
        actual.distance
    );
}

fn sphere(radius: f32) -> Shape {
    Shape::Sphere { radius }
}

fn capsule_x(radius: f32) -> Shape {
    Shape::Capsule {
        a: Vec3::new(-1.0, 0.0, 0.0),
        b: Vec3::new(1.0, 0.0, 0.0),
        radius,
    }
}

fn unit_box() -> Shape {
    Shape::Box {
        half_extents: Vec3::ONE,
        radius: 0.05,
    }
}

#[test]
fn resting_spheres_make_one_point_on_the_lower_sphere() {
    let mut pair = Pair::new(sphere(0.5), sphere(0.5));
    pair.step(at(0.0, 1.0, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0]);
    assert_contact(pair.contact(0), Vec3::new(0.0, 0.5, 0.0), Vec3::Y, 0.0);
}

#[test]
fn spheres_with_one_centre_use_the_x_axis() {
    let mut pair = Pair::new(sphere(0.5), sphere(0.25));
    pair.step(at(2.0, 0.0, 0.0), at(2.0, 0.0, 0.0));
    assert_contact(pair.contact(0), Vec3::new(2.25, 0.0, 0.0), Vec3::X, -0.75);
}

#[test]
fn sphere_points_keep_their_key_and_go_beyond_the_tolerance() {
    let mut pair = Pair::new(sphere(0.5), sphere(0.5));
    pair.step(at(0.0, 1.05, 0.0), at(0.0, 0.0, 0.0));
    pair.step(at(0.0, 1.02, 0.0), at(0.0, 0.0, 0.0));
    pair.step(at(0.0, 0.98, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0]);
    assert_contact(pair.contact(0), Vec3::new(0.0, 0.5, 0.0), Vec3::Y, -0.02);
    pair.step(at(0.0, 1.0999, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0]);
    pair.step(at(0.0, 1.1001, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), Vec::<u32>::new());
    pair.step(at(0.0, 1.0, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [1]);
}

#[test]
fn a_point_that_a_listener_removed_comes_back_with_a_new_key() {
    let mut pair = Pair::new(sphere(0.5), sphere(0.5));
    pair.step(at(0.0, 1.0, 0.0), at(0.0, 0.0, 0.0));
    pair.manifold.points.clear();
    pair.step(at(0.0, 1.0, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [1]);
}

#[test]
fn a_sphere_resting_on_a_capsule_touches_the_capsule_surface() {
    let mut pair = Pair::new(sphere(0.5), capsule_x(0.3));
    pair.step(at(0.2, 0.8, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0]);
    assert_contact(pair.contact(0), Vec3::new(0.2, 0.3, 0.0), Vec3::Y, 0.0);
    pair.step(at(1.6, 0.0, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0]);
    assert_contact(pair.contact(0), Vec3::new(1.3, 0.0, 0.0), Vec3::X, -0.2);
    pair.step(at(0.0, 0.95, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), Vec::<u32>::new());
}

#[test]
fn a_sphere_on_the_capsule_axis_uses_a_perpendicular_normal() {
    let mut pair = Pair::new(sphere(0.5), capsule_x(0.3));
    pair.step(at(0.5, 0.0, 0.0), at(0.0, 0.0, 0.0));
    // The capsule direction is +X with zero Y and Z: the helper takes X and Z, so the
    // normal is (d.z, 0, −d.x) normalized.
    assert_contact(pair.contact(0), Vec3::new(0.5, 0.0, -0.3), -Vec3::Z, -0.8);
}

#[test]
fn a_capsule_against_a_sphere_runs_the_sphere_capsule_agent_swapped() {
    let mut pair = Pair::new(capsule_x(0.3), sphere(0.5));
    pair.step(at(0.0, 0.0, 0.0), at(0.2, 0.82, 0.0));
    assert_eq!(pair.keys(), [0]);
    // The flipped sink moves the point onto the sphere and turns the normal.
    assert_contact(pair.contact(0), Vec3::new(0.2, 0.32, 0.0), -Vec3::Y, 0.02);
}

#[test]
fn a_sphere_on_a_turned_box_touches_the_box_face() {
    let mut pair = Pair::new(sphere(0.5), unit_box());
    let turned = Transform::from_rotation_translation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::new(0.0, 1.0, 0.0),
    );
    pair.step(at(0.3, 2.55, 0.2), turned);
    assert_eq!(pair.keys(), [0]);
    assert_contact(pair.contact(0), Vec3::new(0.3, 2.05, 0.2), Vec3::Y, 0.0);
    pair.step(at(1.15, 2.15, 0.0), turned);
    assert_eq!(pair.keys(), [0]);
    let corner = Vec3::new(1.0, 2.0, 0.0);
    let normal = (Vec3::new(1.15, 2.15, 0.0) - corner).normalize();
    let gap = (Vec3::new(1.15, 2.15, 0.0) - corner).length() - 0.55;
    assert_contact(pair.contact(0), corner + normal * 0.05, normal, gap);
    pair.step(at(0.0, 2.66, 0.0), turned);
    assert_eq!(pair.keys(), Vec::<u32>::new());
}

#[test]
fn a_sphere_centre_inside_the_box_pushes_out_through_the_nearest_face() {
    let mut pair = Pair::new(sphere(0.5), unit_box());
    pair.step(at(0.2, -0.9, 0.0), at(0.0, 0.0, 0.0));
    assert_contact(pair.contact(0), Vec3::new(0.2, -1.05, 0.0), -Vec3::Y, -0.65);
}

#[test]
fn a_box_against_a_sphere_is_symmetric() {
    let mut pair = Pair::new(unit_box(), sphere(0.5));
    pair.step(at(0.0, 0.0, 0.0), at(0.0, 1.6, 0.0));
    assert_contact(pair.contact(0), Vec3::new(0.0, 1.1, 0.0), -Vec3::Y, 0.05);
}

#[test]
fn parallel_resting_capsules_make_two_end_points() {
    let mut pair = Pair::new(capsule_x(0.3), capsule_x(0.3));
    pair.step(at(0.0, 0.6, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0, 1]);
    assert_contact(pair.contact(0), Vec3::new(1.0, 0.3, 0.0), Vec3::Y, 0.0);
    assert_contact(pair.contact(1), Vec3::new(-1.0, 0.3, 0.0), Vec3::Y, 0.0);
    pair.step(at(0.5, 0.62, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0, 1]);
    assert_contact(pair.contact(0), Vec3::new(1.0, 0.3, 0.0), Vec3::Y, 0.02);
    assert_contact(pair.contact(1), Vec3::new(-0.5, 0.3, 0.0), Vec3::Y, 0.02);
    pair.step(at(0.0, 0.71, 0.0), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), Vec::<u32>::new());
}

#[test]
fn crossed_capsules_make_one_point() {
    let mut pair = Pair::new(
        Shape::Capsule {
            a: Vec3::new(0.0, 0.0, -1.0),
            b: Vec3::new(0.0, 0.0, 1.0),
            radius: 0.3,
        },
        capsule_x(0.3),
    );
    pair.step(at(0.25, 0.55, 0.1), at(0.0, 0.0, 0.0));
    assert_eq!(pair.keys(), [0]);
    assert_contact(pair.contact(0), Vec3::new(0.25, 0.3, 0.0), Vec3::Y, -0.05);
}

#[test]
fn every_agent_gives_bit_identical_points_twice() {
    let run = || {
        let shapes = [
            (sphere(0.5), sphere(0.4)),
            (sphere(0.5), capsule_x(0.3)),
            (sphere(0.5), unit_box()),
            (capsule_x(0.3), capsule_x(0.25)),
            (capsule_x(0.3), sphere(0.5)),
        ];
        let turn = Quat::from_euler(glam::EulerRot::YXZ, 0.3, -0.2, 0.1);
        shapes
            .into_iter()
            .map(|(a, b)| {
                let mut pair = Pair::new(a, b);
                for frame in 0..4u8 {
                    let shift = f32::from(frame) * 0.013;
                    pair.step(
                        Transform::from_rotation_translation(
                            turn,
                            Vec3::new(0.1 + shift, 0.55 - shift, 0.07),
                        ),
                        at(0.0, 0.0, 0.0),
                    );
                }
                pair.manifold
            })
            .collect::<Vec<_>>()
    };
    let first = run();
    let counts: Vec<usize> = first.iter().map(|manifold| manifold.points.len()).collect();
    assert!(counts.iter().all(|&count| count > 0), "{counts:?}");
    assert_eq!(first, run());
}

#[test]
fn the_agent_matches_its_shape_pair() {
    assert_eq!(
        PrimitiveAgent::new(&capsule_x(0.3), &capsule_x(0.3)),
        PrimitiveAgent::CapsuleCapsule([None; 3])
    );
}
