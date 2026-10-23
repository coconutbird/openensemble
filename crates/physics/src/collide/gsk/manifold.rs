//! The feature manifold of a GSK pair (`hkpGskManifold`, `MAX_POINTS = 4`).
//!
//! Each point keeps the vertex ids of its two features. A later step rebuilds the point from the
//! vertices at the new transforms (`0x140DCC350`). The manifold changes only through these
//! retail rules:
//!
//! - `find` (`0x140DCC190`): the closest point of GSK matches a point with the same features;
//!   that point moves to slot 0.
//! - `verify` (`0x140DCC350`): each point is rebuilt; an invalid point is removed and the last
//!   point fills its slot.
//! - `add` (`0x140DCCDC0`): a new point goes to slot 0 and the old slot 0 goes to the end. In a
//!   full manifold `drop_index` (`0x140DCD2A0`) selects the point to replace.

use glam::Vec3;

use super::cache::GskCache;
use crate::collide::convex::{ConvexRef, dot, transform_point};
use crate::collide::sink::ContactSink;
use crate::contact::ContactPoint;
use crate::math::Transform;

/// The largest number of points (`hkpGskManifold::MAX_POINTS`).
pub(crate) const MAX_POINTS: usize = 4;

/// The pull of the depth when a full manifold drops a point (`0x140DCD2A0`).
const DEPTH_WEIGHT: f32 = 526.0;

/// The start ratio of the depth test when a full manifold drops a point (`0x140DCD2A0`).
const DEPTH_RATIO: f32 = 0.030_461_745;

/// A point of the manifold (`hkpGskManifold::ContactPoint`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FeaturePoint {
    /// The number of vertices of A.
    pub(crate) dim_a: u8,
    /// The number of vertices of B.
    pub(crate) dim_b: u8,
    /// The vertex ids of A, then of B.
    pub(crate) ids: [u16; 4],
    /// The key of the point in the contact sink.
    pub(crate) key: u32,
}

/// The data that rebuilds a point (the vertex array and the radii of `0x140DCC350`).
pub(crate) struct Frame<'a> {
    pub(crate) a: ConvexRef<'a>,
    pub(crate) b: ConvexRef<'a>,
    pub(crate) world_a: Transform,
    pub(crate) world_b: Transform,
    /// The world separating normal.
    pub(crate) normal: Vec3,
    /// The keep distance (`m_tolerance` of the input).
    pub(crate) tolerance: f32,
}

/// The persistent points of a GSK pair.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GskManifold {
    pub(crate) points: Vec<FeaturePoint>,
}

impl GskManifold {
    /// Removes all points (`0x140DCD220`).
    pub(crate) fn clear(&mut self, sink: &mut ContactSink<'_>) {
        for point in &self.points {
            sink.remove_point(point.key);
        }
        self.points.clear();
    }

    /// Looks for the point of the cache features (`0x140DCC190`). Moves it to slot 0 and
    /// returns `true` if it exists.
    ///
    /// If no point matches and the cache has a single vertex on one side, a point with that same
    /// single vertex becomes invalid, so that `verify` removes it.
    pub(crate) fn find(&mut self, cache: &GskCache) -> bool {
        let total = usize::from(cache.dim_a + cache.dim_b);
        if let Some(index) = self.points.iter().position(|p| {
            p.dim_a == cache.dim_a
                && p.dim_b == cache.dim_b
                && p.ids[..total] == cache.vertices[..total]
        }) {
            self.points.swap(0, index);
            return true;
        }
        if cache.dim_a == 1 || cache.dim_b == 1 {
            let first_b = cache.vertices[usize::from(cache.dim_a)];
            if let Some(point) = self.points.iter_mut().find(|p| {
                (cache.dim_a == 1 && p.dim_a == 1 && p.ids[0] == cache.vertices[0])
                    || (cache.dim_b == 1 && p.dim_b == 1 && p.ids[usize::from(p.dim_a)] == first_b)
            }) {
                point.dim_a = 0;
                point.dim_b = 0;
            }
        }
        false
    }

    /// Rebuilds the points from slot `start` on (`0x140DCC350`). Removes each invalid point from
    /// the sink and returns the kept points with their new geometry.
    pub(crate) fn verify(
        &mut self,
        start: usize,
        frame: &Frame<'_>,
        sink: &mut ContactSink<'_>,
    ) -> Vec<(u32, ContactPoint)> {
        let mut kept = Vec::new();
        let mut index = start;
        while index < self.points.len() {
            let point = self.points[index];
            if let Some(contact) = rebuild(&point, frame) {
                kept.push((point.key, contact));
                index += 1;
            } else {
                sink.remove_point(point.key);
                self.points.swap_remove(index);
            }
        }
        kept
    }

    /// Adds the closest point of the cache features (`0x140DCCDC0`). `existing` has the kept
    /// points in slot order; for a full manifold it must hold all four.
    ///
    /// A rejected point changes nothing. A full manifold that loses its replaced point in the
    /// sink keeps three points.
    pub(crate) fn add(
        &mut self,
        cache: &GskCache,
        contact: ContactPoint,
        existing: &mut Vec<(u32, ContactPoint)>,
        sink: &mut ContactSink<'_>,
    ) {
        let point = |key| FeaturePoint {
            dim_a: cache.dim_a,
            dim_b: cache.dim_b,
            ids: cache.vertices,
            key,
        };
        if self.points.len() < MAX_POINTS {
            let Some(key) = sink.add_point(contact) else {
                return;
            };
            let old_first = self.points.first().copied();
            if let Some(old) = old_first {
                self.points.push(old);
                self.points[0] = point(key);
            } else {
                self.points.push(point(key));
            }
            return;
        }
        let mut candidates = [ContactPoint::default(); 5];
        for (slot, (_, c)) in candidates.iter_mut().zip(existing.iter()) {
            *slot = *c;
        }
        candidates[4] = contact;
        let index = drop_index(&candidates);
        if index == 4 {
            return;
        }
        sink.remove_point(self.points[index].key);
        if index == 0 {
            self.points[0] = self.points[1];
        } else {
            self.points[index] = self.points[0];
        }
        if let Some(key) = sink.add_point(contact) {
            self.points[0] = point(key);
            existing[index] = (key, contact);
        } else {
            self.points[0] = self.points[3];
            self.points.truncate(3);
            existing[index] = existing[3];
            existing.truncate(3);
        }
    }
}

/// Selects the point that a full manifold drops (`0x140DCD2A0`). Index 4 is the new point.
fn drop_index(points: &[ContactPoint; 5]) -> usize {
    let new = points[4];
    let squared = |a: Vec3, b: Vec3| dot(a - b, a - b);
    let mut kept = [false; 5];
    let mut distances = [0.0; 4];
    let (mut far, mut far_index, mut near, mut near_index) = (0.0, 0, f32::MAX, 0);
    for (i, distance) in distances.iter_mut().enumerate() {
        *distance = squared(new.position, points[i].position);
        if *distance > far {
            far = *distance;
            far_index = i;
        }
        if near > *distance {
            near = *distance;
            near_index = i;
        }
    }
    let deeper = new.distance - points[near_index].distance;
    kept[far_index] = true;
    let mut limit = far * 1.05;
    let mut second = 4;
    for i in 0..5 {
        if !kept[i] {
            let d = squared(points[far_index].position, points[i].position);
            if d > limit {
                limit = d;
                second = i;
            }
        }
    }
    kept[second] = true;
    let edge = points[far_index].position - points[second].position;
    let mut third = 0;
    let mut area = 0.0;
    for i in 0..5 {
        if !kept[i] {
            let cross = (points[i].position - points[far_index].position).cross(edge);
            let a = dot(cross, cross);
            if a > area {
                area = a;
                third = i;
            }
        }
    }
    kept[third] = true;
    let mut fourth = 0;
    let mut spread = 0.0;
    for i in 0..5 {
        if !kept[i] {
            let d = squared(points[third].position, points[i].position);
            if d > spread {
                spread = d;
                fourth = i;
            }
        }
    }
    kept[fourth] = true;
    if kept[4] || deeper >= 0.0 || deeper * DEPTH_WEIGHT * deeper <= near {
        return kept.iter().position(|k| !k).unwrap_or(0);
    }
    let mut choice = 4;
    let mut ratio = DEPTH_RATIO;
    for (i, distance) in distances.iter().enumerate() {
        let gap = points[i].distance - new.distance;
        let value = gap * gap + 1e-9;
        if value > distance * ratio {
            if i == 3 {
                return 3;
            }
            choice = i;
            ratio = value / (distance + 1e-9);
        }
    }
    choice
}

/// Rebuilds one point from its vertices (one case of `0x140DCC350`). Returns `None` for an
/// invalid point.
fn rebuild(point: &FeaturePoint, frame: &Frame<'_>) -> Option<ContactPoint> {
    let dim_a = usize::from(point.dim_a);
    let dim_b = usize::from(point.dim_b);
    let vertex_a = |i: usize| transform_point(&frame.world_a, frame.a.vertex(point.ids[i]));
    let vertex_b = |i: usize| transform_point(&frame.world_b, frame.b.vertex(point.ids[dim_a + i]));
    let radius_a = frame.a.radius();
    let radius_b = frame.b.radius();
    let reach = (radius_a + frame.tolerance) + radius_b;
    let reach_squared = reach * reach;
    let (normal, on_a, offset) = match (dim_a, dim_b) {
        (1, 1) => {
            let a = vertex_a(0);
            let offset = a - vertex_b(0);
            if dot(offset, offset) > reach_squared {
                return None;
            }
            (frame.normal, a, offset)
        }
        (1, 2) => {
            let (p, b0, b1) = (vertex_a(0), vertex_b(0), vertex_b(1));
            let edge = b1 - b0;
            let to_end = b1 - p;
            let (s_end, s_start) = (dot(to_end, edge), dot(p - b0, edge));
            let cross = to_end.cross(edge);
            if dot(cross, cross) > dot(edge, edge) * reach_squared || s_end < 0.0 || s_start < 0.0 {
                return None;
            }
            let t = s_end / (s_end + s_start);
            (
                frame.normal,
                p,
                p - ((Vec3::ONE - Vec3::splat(t)) * b0 + t * b1),
            )
        }
        (2, 1) => {
            let (a0, a1, q) = (vertex_a(0), vertex_a(1), vertex_b(0));
            let edge = a1 - a0;
            let to_end = a1 - q;
            let (s_end, s_start) = (dot(to_end, edge), dot(q - a0, edge));
            let cross = to_end.cross(edge);
            if dot(cross, cross) > dot(edge, edge) * reach_squared || s_end < 0.0 || s_start < 0.0 {
                return None;
            }
            let t = s_end / (s_end + s_start);
            let on_a = (Vec3::ONE - Vec3::splat(t)) * a1 + t * a0;
            (frame.normal, on_a, on_a - q)
        }
        (2, 2) => {
            let (a0, a1, b0, b1) = (vertex_a(0), vertex_a(1), vertex_b(0), vertex_b(1));
            let (edge_a, edge_b) = (a1 - a0, b1 - b0);
            let normal = edge_a.cross(edge_b);
            let side_a = edge_a.cross(normal);
            let side_b = edge_b.cross(normal);
            let s = [
                dot(side_b, a0 - b0),
                dot(side_a, a0 - b0),
                dot(side_b, a1 - b1),
                dot(side_a, a1 - b1),
            ];
            let negative = s.map(|v| v < 0.0);
            let valid =
                negative == [true, true, false, false] || negative == [false, false, true, true];
            if !valid {
                return None;
            }
            let t = s[0] / (s[0] - s[2]);
            let u = s[1] / (s[1] - s[3]);
            let on_a = (Vec3::ONE - Vec3::splat(t)) * a0 + a1 * t;
            let on_b = (Vec3::ONE - Vec3::splat(u)) * b0 + b1 * u;
            (frame.normal, on_a, on_a - on_b)
        }
        (1, 3) => {
            let p = vertex_a(0);
            let (b0, b1, b2) = (vertex_b(0), vertex_b(1), vertex_b(2));
            let n = unit((b2 - b1).cross(b0 - b2));
            let lanes = [
                dot((p - b1).cross(b2 - b1), n),
                dot((p - b2).cross(b0 - b2), n),
                dot((p - b0).cross(b1 - b0), n),
            ];
            (face_normal(n, lanes, frame.normal)?, p, p - b0)
        }
        (3, 1) => {
            let (a0, a1, a2) = (vertex_a(0), vertex_a(1), vertex_a(2));
            let q = vertex_b(0);
            let n = unit((a2 - a1).cross(a0 - a2));
            let offset = a0 - q;
            let lanes = [
                dot((q - a1).cross(a2 - a1), n),
                dot((q - a2).cross(a0 - a2), n),
                dot((q - a0).cross(a1 - a0), n),
            ];
            let on_a = n * dot(offset, n) + q;
            (face_normal(n, lanes, frame.normal)?, on_a, offset)
        }
        _ => return None,
    };
    let along = dot(normal, offset);
    let distance = along - (radius_b + radius_a);
    if frame.tolerance <= distance {
        return None;
    }
    Some(ContactPoint {
        position: normal * (radius_b - along) + on_a,
        normal,
        distance,
    })
}

/// Returns the face normal turned toward the separating normal, or `None` if the vertex is not
/// over the face (the end of the face cases of `0x140DCC350`).
fn face_normal(normal: Vec3, lanes: [f32; 3], separating: Vec3) -> Option<Vec3> {
    if lanes.iter().any(|lane| *lane >= 0.0) {
        return None;
    }
    Some(if dot(separating, normal) < 0.0 {
        -normal
    } else {
        normal
    })
}

/// Normalizes like the retail code: `1/sqrt` with one Newton step.
fn unit(v: Vec3) -> Vec3 {
    let length_squared = dot(v, v);
    let inverse = 1.0 / length_squared.sqrt();
    let refined = 0.5 * inverse * (3.0 - length_squared * inverse * inverse);
    v * refined
}
