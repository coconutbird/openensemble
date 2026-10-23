//! Tests of the box–box agent.

use glam::{Quat, Vec3};

use super::BoxBoxAgent;
use crate::body::BodyId;
use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;
use crate::contact::{Manifold, ManifoldId};
use crate::math::Transform;
use crate::shape::{DEFAULT_CONVEX_RADIUS, Shape};

const TOLERANCE: f32 = 0.1;

/// One pair of boxes with its agent and manifold.
struct Pair {
    a: Shape,
    b: Shape,
    agent: BoxBoxAgent,
    manifold: Manifold,
    next_key: u32,
}

impl Pair {
    fn new(half_a: Vec3, half_b: Vec3) -> Self {
        Self {
            a: Shape::Box {
                half_extents: half_a,
                radius: DEFAULT_CONVEX_RADIUS,
            },
            b: Shape::Box {
                half_extents: half_b,
                radius: DEFAULT_CONVEX_RADIUS,
            },
            agent: BoxBoxAgent::new(),
            manifold: Manifold::new(
                ManifoldId::from_raw(0),
                BodyId::from_raw(0),
                BodyId::from_raw(1),
            ),
            next_key: 0,
        }
    }

    fn step(&mut self, a: Transform, b: Transform) {
        let body_a = CdBody {
            shape: &self.a,
            transform: a,
        };
        let body_b = CdBody {
            shape: &self.b,
            transform: b,
        };
        let mut sink = ContactSink::new(&mut self.manifold, &mut self.next_key);
        self.agent
            .process(&body_a, &body_b, &CollisionInput::new(TOLERANCE), &mut sink);
        assert_eq!(self.agent.slots.len(), self.manifold.points.len());
    }

    fn keys(&self) -> Vec<u32> {
        let mut keys: Vec<u32> = self.manifold.points.iter().map(|p| p.key).collect();
        keys.sort_unstable();
        keys
    }
}

fn at(rotation: Quat, translation: Vec3) -> Transform {
    Transform::from_rotation_translation(rotation, translation)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// A unit cube (half extent 0.5) above a large ground box (half extents 5, 0.5, 5). The gap
/// between the shrunk boxes is `gap`.
fn resting(gap: f32, x: f32) -> (Transform, Transform) {
    (
        at(Quat::IDENTITY, Vec3::new(x, 1.0 + gap, 0.0)),
        Transform::IDENTITY,
    )
}

fn resting_pair() -> Pair {
    Pair::new(Vec3::splat(0.5), Vec3::new(5.0, 0.5, 5.0))
}

#[test]
fn a_box_resting_face_on_gets_its_four_bottom_vertices_at_zero_distance() {
    let mut pair = resting_pair();
    let (a, b) = resting(0.1, 0.0);
    pair.step(a, b);
    let points = &pair.manifold.points;
    assert_eq!(points.len(), 4);
    for point in points {
        let contact = point.contact;
        assert!(close(contact.distance, 0.0), "{contact:?}");
        assert!(close(contact.normal.y, 1.0), "{contact:?}");
        assert!(close(contact.position.y, 1.1 - 0.55), "{contact:?}");
        assert!(close(contact.position.x.abs(), 0.55), "{contact:?}");
        assert!(close(contact.position.z.abs(), 0.55), "{contact:?}");
    }
    assert!(pair.agent.slots.iter().all(|slot| slot.feature.kind == 5));
    assert_eq!(pair.agent.face_vertex_count, 4);
    assert!(pair.agent.is_complete);
}

#[test]
fn the_keys_stay_over_frames_of_small_motion() {
    let mut pair = resting_pair();
    let (a, b) = resting(0.1, 0.0);
    pair.step(a, b);
    let keys = pair.keys();
    for frame in 1..6_u8 {
        let offset = f32::from(frame) * 0.002;
        let (a, b) = resting(0.1 - offset * 0.5, offset);
        pair.step(a, b);
        assert_eq!(pair.keys(), keys, "frame {frame}");
        let lowest = pair
            .manifold
            .points
            .iter()
            .map(|p| p.contact.distance)
            .fold(f32::MAX, f32::min);
        assert!(close(lowest, -offset * 0.5), "frame {frame}: {lowest}");
    }
}

#[test]
fn the_points_go_beyond_the_keep_distance() {
    let mut pair = resting_pair();
    let (a, b) = resting(0.1, 0.0);
    pair.step(a, b);
    assert_eq!(pair.manifold.points.len(), 4);
    let (a, b) = resting(0.15, 0.0);
    pair.step(a, b);
    assert_eq!(
        pair.manifold.points.len(),
        4,
        "a gap of 0.05 is in the tolerance"
    );
    let (a, b) = resting(0.25, 0.0);
    pair.step(a, b);
    assert_eq!(pair.manifold.points.len(), 0);
    assert_eq!(pair.agent.slots.len(), 0);
    assert_eq!(pair.agent.face_vertex_count, 0);
}

#[test]
fn crossing_edges_make_an_edge_edge_point() {
    let mut pair = Pair::new(Vec3::splat(0.5), Vec3::splat(0.5));
    let reach = std::f32::consts::SQRT_2 * 0.55;
    let a = at(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
        Vec3::new(0.0, 2.0 * reach + 0.05, 0.0),
    );
    let b = at(
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_4),
        Vec3::ZERO,
    );
    pair.step(a, b);
    assert_eq!(pair.manifold.points.len(), 1);
    assert!(pair.agent.slots[0].feature.kind > 6);
    let contact = pair.manifold.points[0].contact;
    assert!(close(contact.distance, 0.05), "{contact:?}");
    assert!(close(contact.normal.y, 1.0), "{contact:?}");
    assert!(close(contact.position.x, 0.0), "{contact:?}");
    assert!(close(contact.position.z, 0.0), "{contact:?}");
    assert!(close(contact.position.y, reach + 0.05), "{contact:?}");
}

/// A tumbling cube that sinks into a ground box, for many frames.
fn tumble(pair: &mut Pair, frames: u16) -> Vec<Vec<(u32, [u32; 7])>> {
    let mut history = Vec::new();
    for frame in 0..frames {
        let time = f32::from(frame) * 0.05;
        let rotation = Quat::from_euler(glam::EulerRot::YXZ, time * 0.7, time * 0.3, time * 0.5);
        let a = at(
            rotation,
            Vec3::new(time.sin() * 0.3, 0.95, time.cos() * 0.2),
        );
        pair.step(a, Transform::IDENTITY);
        assert!(pair.agent.slots.len() <= 8);
        assert!(pair.manifold.points.len() <= 8);
        history.push(
            pair.manifold
                .points
                .iter()
                .map(|p| {
                    let c = p.contact;
                    (
                        p.key,
                        [
                            c.position.x.to_bits(),
                            c.position.y.to_bits(),
                            c.position.z.to_bits(),
                            c.normal.x.to_bits(),
                            c.normal.y.to_bits(),
                            c.normal.z.to_bits(),
                            c.distance.to_bits(),
                        ],
                    )
                })
                .collect(),
        );
    }
    history
}

#[test]
fn the_manifold_never_has_more_than_eight_points() {
    let mut pair = Pair::new(Vec3::splat(0.5), Vec3::new(5.0, 0.5, 5.0));
    let history = tumble(&mut pair, 200);
    assert!(history.iter().any(|points| !points.is_empty()));
}

#[test]
fn the_same_sequence_gives_the_same_manifolds() {
    let mut first = Pair::new(Vec3::splat(0.5), Vec3::new(5.0, 0.5, 5.0));
    let mut second = Pair::new(Vec3::splat(0.5), Vec3::new(5.0, 0.5, 5.0));
    let one = tumble(&mut first, 120);
    let two = tumble(&mut second, 120);
    assert_eq!(one, two);
    assert_eq!(first.agent, second.agent);
}

#[test]
fn the_normal_packing_follows_the_retail_bytes() {
    assert_eq!(super::pack(Vec3::new(0.0, 1.0, -1.0)), [128, 244, 12]);
    let back = super::unpack(super::pack(Vec3::new(0.6, -0.8, 0.0)));
    assert!(
        (back - Vec3::new(0.6, -0.8, 0.0)).abs().max_element() < 0.01,
        "{back}"
    );
}
