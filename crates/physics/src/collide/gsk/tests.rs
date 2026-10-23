//! Tests of the GSK agent: contacts of convex pairs over steps.

use std::sync::Arc;

use glam::{Quat, Vec3};

use super::GskAgent;
use crate::body::BodyId;
use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;
use crate::contact::{Manifold, ManifoldId};
use crate::math::Transform;
use crate::shape::{DEFAULT_CONVEX_RADIUS, Shape};

/// The top of the ground box: half height 0.5 plus the convex radius.
const GROUND_TOP: f32 = 0.55;

fn ground() -> Shape {
    Shape::Box {
        half_extents: Vec3::new(4.0, 0.5, 4.0),
        radius: DEFAULT_CONVEX_RADIUS,
    }
}

fn at(translation: Vec3) -> Transform {
    Transform::from_rotation_translation(Quat::IDENTITY, translation)
}

/// Returns a small rotation about `axis` without trigonometry (about `2·amount` radians).
fn tilt(axis: Vec3, amount: f32) -> Quat {
    let axis = axis.normalize() * amount;
    Quat::from_xyzw(axis.x, axis.y, axis.z, 1.0).normalize()
}

/// One pair of bodies with its agent and manifold.
struct Pair {
    agent: GskAgent,
    manifold: Manifold,
    next_key: u32,
    input: CollisionInput,
}

impl Pair {
    fn new(a: &Shape, b: &Shape) -> Self {
        Self {
            agent: GskAgent::new(a, b),
            manifold: Manifold::new(
                ManifoldId::from_raw(0),
                BodyId::from_raw(0),
                BodyId::from_raw(1),
            ),
            next_key: 0,
            input: CollisionInput::new(0.1),
        }
    }

    fn step(&mut self, a: &Shape, a_transform: Transform, b: &Shape, b_transform: Transform) {
        let body_a = CdBody {
            shape: a,
            transform: a_transform,
        };
        let body_b = CdBody {
            shape: b,
            transform: b_transform,
        };
        let mut sink = ContactSink::new(&mut self.manifold, &mut self.next_key);
        self.agent.process(&body_a, &body_b, &self.input, &mut sink);
    }

    fn keys(&self) -> Vec<u32> {
        let mut keys: Vec<u32> = self.manifold.points.iter().map(|p| p.key).collect();
        keys.sort_unstable();
        keys
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn a_sphere_on_a_box_has_one_point_at_the_gap() {
    let sphere = Shape::Sphere { radius: 0.5 };
    let floor = ground();
    let mut pair = Pair::new(&sphere, &floor);
    let center = Vec3::new(0.3, GROUND_TOP + 0.5 + 0.02, -0.2);
    for _ in 0..3 {
        pair.step(&sphere, at(center), &floor, Transform::IDENTITY);
    }
    assert_eq!(pair.manifold.points.len(), 1);
    let contact = pair.manifold.points[0].contact;
    assert!(close(contact.distance, 0.02), "{contact:?}");
    assert!((contact.normal - Vec3::Y).length() < 1e-5, "{contact:?}");
    assert!(
        (contact.position - Vec3::new(0.3, GROUND_TOP, -0.2)).length() < 1e-4,
        "{contact:?}"
    );
    assert_eq!(pair.keys(), [0]);
}

#[test]
fn a_capsule_lying_on_a_box_gets_two_points() {
    let capsule = Shape::Capsule {
        a: Vec3::new(-0.6, 0.0, 0.0),
        b: Vec3::new(0.6, 0.0, 0.0),
        radius: 0.2,
    };
    let floor = ground();
    let mut pair = Pair::new(&capsule, &floor);
    let center = Vec3::new(0.0, GROUND_TOP + 0.2 + 0.002, 0.0);
    for frame in 0..12 {
        let sign = if frame % 2 == 0 { 1.0 } else { -1.0 };
        let transform = Transform::from_rotation_translation(tilt(Vec3::Z, 0.005 * sign), center);
        pair.step(&capsule, transform, &floor, Transform::IDENTITY);
    }
    assert_eq!(pair.manifold.points.len(), 2, "{:?}", pair.manifold.points);
    let mut xs: Vec<f32> = pair
        .manifold
        .points
        .iter()
        .map(|p| p.contact.position.x)
        .collect();
    xs.sort_by(f32::total_cmp);
    assert!(close(xs[0], -0.6) && close(xs[1], 0.6), "{xs:?}");
    for point in &pair.manifold.points {
        assert!(point.contact.normal.y > 0.99);
        assert!(point.contact.distance.abs() < 0.01);
    }
}

fn rocking_box() -> Shape {
    Shape::Transform {
        transform: at(Vec3::new(0.1, 0.3, -0.1)),
        child: Arc::new(Shape::Box {
            half_extents: Vec3::new(0.5, 0.25, 0.5),
            radius: DEFAULT_CONVEX_RADIUS,
        }),
    }
}

/// Runs a box in a transform that rocks over its four lower corners on the ground.
fn run_rocking(frames: usize) -> Vec<Vec<u32>> {
    let body = rocking_box();
    let floor = ground();
    let mut pair = Pair::new(&body, &floor);
    // The child box bottom is at 0.3 - 0.3 = 0 in the body space.
    let origin = Vec3::new(0.0, GROUND_TOP + 0.003, 0.0);
    let axes = [
        Vec3::new(1.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, -1.0),
    ];
    let mut history = Vec::new();
    for frame in 0..frames {
        let rotation = tilt(axes[frame % 4], 0.005);
        pair.step(
            &body,
            Transform::from_rotation_translation(rotation, origin),
            &floor,
            Transform::IDENTITY,
        );
        history.push(pair.keys());
    }
    history
}

#[test]
fn a_rocking_box_in_a_transform_builds_four_points_with_stable_keys() {
    let history = run_rocking(24);
    let last = history.last().expect("frames");
    assert_eq!(last.len(), 4, "{history:?}");
    let first_full = history
        .iter()
        .position(|keys| keys.len() == 4)
        .expect("four points");
    assert!(
        history[first_full..].iter().all(|keys| keys == last),
        "{history:?}"
    );
}

#[test]
fn shapes_beyond_the_keep_distance_lose_their_points() {
    let sphere = Shape::Sphere { radius: 0.5 };
    let floor = ground();
    let mut pair = Pair::new(&sphere, &floor);
    // Off the face diagonals: exactly over the centre GSK stops on the diagonal edge of the
    // top face (a vertex–edge feature), which only makes a point below `create_contact`.
    let touching = Vec3::new(0.3, GROUND_TOP + 0.5 + 0.01, -0.2);
    pair.step(&sphere, at(touching), &floor, Transform::IDENTITY);
    assert_eq!(pair.manifold.points.len(), 1);
    pair.step(
        &sphere,
        at(touching + Vec3::Y * 0.2),
        &floor,
        Transform::IDENTITY,
    );
    assert_eq!(pair.manifold.points.len(), 0);
}

#[test]
fn a_cylinder_standing_on_a_box_makes_contact() {
    let cylinder = Shape::Cylinder {
        a: Vec3::new(0.0, -0.5, 0.0),
        b: Vec3::new(0.0, 0.5, 0.0),
        radius: 0.3,
        convex_radius: DEFAULT_CONVEX_RADIUS,
    };
    let floor = ground();
    let mut pair = Pair::new(&cylinder, &floor);
    let center = Vec3::new(0.0, GROUND_TOP + 0.5 + DEFAULT_CONVEX_RADIUS + 0.01, 0.0);
    for _ in 0..3 {
        pair.step(&cylinder, at(center), &floor, Transform::IDENTITY);
    }
    assert_ne!(pair.manifold.points.len(), 0);
    for point in &pair.manifold.points {
        assert!(close(point.contact.distance, 0.01), "{point:?}");
        assert!(
            (point.contact.normal - Vec3::Y).length() < 1e-4,
            "{point:?}"
        );
    }
}

#[test]
fn a_twice_run_sequence_is_bit_identical() {
    let run = || {
        let body = rocking_box();
        let floor = ground();
        let mut pair = Pair::new(&body, &floor);
        let origin = Vec3::new(0.0, GROUND_TOP + 0.003, 0.0);
        let mut bits = Vec::new();
        for frame in 0..16_u16 {
            let axis = Vec3::new(1.0, 0.0, f32::from(frame % 3) - 1.0);
            let rotation = tilt(axis, 0.004);
            pair.step(
                &body,
                Transform::from_rotation_translation(rotation, origin),
                &floor,
                Transform::IDENTITY,
            );
            for point in &pair.manifold.points {
                let c = point.contact;
                bits.push((
                    point.key,
                    c.position.to_array().map(f32::to_bits),
                    c.normal.to_array().map(f32::to_bits),
                    c.distance.to_bits(),
                ));
            }
        }
        bits
    };
    let first = run();
    assert_ne!(first.len(), 0);
    assert_eq!(first, run());
}

#[test]
fn overlapping_cores_use_the_penetration_depth() {
    let sphere = Shape::Sphere { radius: 0.5 };
    let floor = ground();
    let mut pair = Pair::new(&sphere, &floor);
    pair.step(
        &sphere,
        at(Vec3::new(0.3, 0.3, -0.2)),
        &floor,
        Transform::IDENTITY,
    );
    assert_eq!(pair.manifold.points.len(), 1, "{:?}", pair.manifold.points);
    let contact = pair.manifold.points[0].contact;
    assert!(
        close(contact.distance, 0.3 - 0.5 - 0.05 - 0.5),
        "{contact:?}"
    );
    assert!((contact.normal - Vec3::Y).length() < 1e-4, "{contact:?}");
}
