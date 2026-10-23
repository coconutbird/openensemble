//! The GSK closest-feature query (`hkpGsk::getClosestFeature`, `0x140DC8CB0`).
//!
//! GSK keeps a feature of each shape: `dim_a` vertices of A and `dim_b` vertices of B, with at
//! most four vertices in total. Each pass reduces the pair of features to the closest pair
//! (the switch on `8·dim_a + dim_b`), and then adds the support vertex of one shape:
//!
//! - On odd passes it tries shape A first, on even passes shape B first.
//! - A new vertex counts only if it is farther along the search direction than each kept vertex
//!   of its shape by more than `eps · |d|²`. `eps` starts at `1e-10` and doubles each pass.
//! - If no shape gives a new vertex, the features are the closest features.
//! - If the cores touch or overlap, the penetration solver gives the features.

use glam::Vec3;

use super::penetration;
use super::simplex::{
    NEXT_FIRST, NEXT_SECOND, REGION_VERTEX, Vertex, point_tetrahedron, point_triangle,
    segment_segment,
};
use crate::collide::convex::{ConvexRef, dot, inverse_rotate, transform_point};
use crate::math::Transform;

/// The start value of the GSK accuracy factor (`0x140F195FC`).
const START_EPSILON: f32 = 1e-10;

/// The result of the reduction step of one pass.
enum Reduced {
    /// The features are reduced and `dir` is the direction from B to A.
    Done,
    /// The cores touch or overlap.
    Penetration,
}

/// The working state of one GSK query (the retail `hkpGsk` on the stack).
pub(super) struct Gsk<'a> {
    pub(super) shape_a: ConvexRef<'a>,
    pub(super) shape_b: ConvexRef<'a>,
    /// The transform from the space of B to the space of A.
    pub(super) b_to_a: Transform,
    pub(super) dim_a: usize,
    pub(super) dim_b: usize,
    max_dim_a: usize,
    max_dim_b: usize,
    /// `true` if the features changed (`+0x14`); the caller then writes the cache back.
    pub(super) feature_change: bool,
    pub(super) a: [Vertex; 4],
    pub(super) b: [Vertex; 4],
    /// The search direction from B to A, not normalized (`+0x120`).
    pub(super) dir: Vec3,
    /// The closest point on the edge of A after an edge–edge step (`+0x130`).
    pub(super) closest_on_a: Vec3,
    /// The last point–triangle lanes (`+0x160`).
    lanes: [f32; 3],
    /// `dim_b` after the last finished reduction (`+0x170`). Retail reads it before the first
    /// write; this port starts with 0.
    last_dim_b: usize,
    /// A shape that may not get a new vertex in this pass: 1 for A, -1 for B (`+0x174`).
    forbid: i32,
}

impl<'a> Gsk<'a> {
    /// Loads the features of a cache (the start of `0x140DCA870`).
    pub(super) fn new(
        shape_a: ConvexRef<'a>,
        shape_b: ConvexRef<'a>,
        b_to_a: Transform,
        ids: [u16; 4],
        dims: (u8, u8),
        max_dims: (u8, u8),
    ) -> Self {
        let (dim_a, dim_b) = (usize::from(dims.0), usize::from(dims.1));
        let mut a = [Vertex::default(); 4];
        let mut b = [Vertex::default(); 4];
        for (vertex, &id) in a.iter_mut().zip(&ids[..dim_a]) {
            let local = shape_a.vertex(id);
            *vertex = Vertex {
                pos: local,
                local,
                id,
            };
        }
        for (vertex, &id) in b.iter_mut().zip(&ids[dim_a..dim_a + dim_b]) {
            let local = shape_b.vertex(id);
            *vertex = Vertex {
                pos: transform_point(&b_to_a, local),
                local,
                id,
            };
        }
        Self {
            shape_a,
            shape_b,
            b_to_a,
            dim_a,
            dim_b,
            max_dim_a: usize::from(max_dims.0),
            max_dim_b: usize::from(max_dims.1),
            feature_change: false,
            a,
            b,
            dir: Vec3::ZERO,
            closest_on_a: Vec3::ZERO,
            lanes: [0.0; 3],
            last_dim_b: 0,
            forbid: 0,
        }
    }

    /// Runs the query and returns the unit normal from B to A in the space of A and the
    /// distance of the cores.
    pub(super) fn closest_feature(&mut self) -> (Vec3, f32) {
        let mut epsilon = START_EPSILON;
        let mut pass = 0_u32;
        loop {
            pass += 1;
            epsilon *= 2.0;
            self.forbid = 0;
            let old_total = self.dim_a + self.dim_b;
            let reduced = self.reduce();
            if let Reduced::Done = reduced {
                self.last_dim_b = self.dim_b;
            }
            if self.dim_a + self.dim_b != old_total {
                self.feature_change = true;
            }
            let length_squared = dot(self.dir, self.dir);
            let projection = dot(self.a[0].pos - self.b[0].pos, self.dir);
            let limit = length_squared * epsilon;
            if matches!(reduced, Reduced::Penetration)
                || limit >= projection.abs() * projection
                || epsilon > length_squared
            {
                penetration::solve(self);
                break;
            }
            let odd = pass & 1 == 1;
            if odd && self.try_add_a(limit) {
                continue;
            }
            if self.try_add_b(limit) {
                continue;
            }
            if !odd && self.try_add_a(limit) {
                continue;
            }
            break;
        }
        let length_squared = dot(self.dir, self.dir);
        let inverse = 1.0 / length_squared.sqrt();
        let refined = 0.5 * inverse * (3.0 - length_squared * inverse * inverse);
        let normal = self.dir * refined;
        self.dir = normal;
        (normal, dot(self.a[0].pos - self.b[0].pos, normal))
    }

    /// Tries to add the support vertex of A in the direction `-dir`.
    fn try_add_a(&mut self, limit: f32) -> bool {
        if self.dim_a == self.max_dim_a || self.forbid == 1 {
            return false;
        }
        let direction = -self.dir;
        let (vertex, id) = self.shape_a.support(direction);
        if !improves(&self.a[..self.dim_a], vertex, direction, limit, |v| v.pos) {
            return false;
        }
        self.a[self.dim_a] = Vertex {
            pos: vertex,
            local: vertex,
            id,
        };
        self.dim_a += 1;
        self.feature_change = true;
        true
    }

    /// Tries to add the support vertex of B in the direction `dir` (rotated into B).
    fn try_add_b(&mut self, limit: f32) -> bool {
        if self.dim_b == self.max_dim_b || self.forbid == -1 {
            return false;
        }
        let direction = inverse_rotate(&self.b_to_a, self.dir);
        let (vertex, id) = self.shape_b.support(direction);
        if !improves(&self.b[..self.dim_b], vertex, direction, limit, |v| v.local) {
            return false;
        }
        self.b[self.dim_b] = Vertex {
            pos: transform_point(&self.b_to_a, vertex),
            local: vertex,
            id,
        };
        self.dim_b += 1;
        self.feature_change = true;
        true
    }

    /// Reduces the features to the closest pair (the switch at `0x140DC8DFC`).
    fn reduce(&mut self) -> Reduced {
        loop {
            match (self.dim_a, self.dim_b) {
                (1, 1) => {
                    self.dir = self.a[0].pos - self.b[0].pos;
                    return Reduced::Done;
                }
                (1, 2) => {
                    self.point_segment_b();
                    return Reduced::Done;
                }
                (2, 1) => {
                    self.segment_point_a();
                    return Reduced::Done;
                }
                (1, 3) => {
                    if self.point_face(false) {
                        return Reduced::Done;
                    }
                }
                (3, 1) => {
                    if self.point_face(true) {
                        return Reduced::Done;
                    }
                }
                (1, 4) => {
                    let tetrahedron = self.b.map(|v| v.pos);
                    let Some(index) = point_tetrahedron(self.a[0].pos, tetrahedron, 1.0) else {
                        return Reduced::Penetration;
                    };
                    self.dim_b = 3;
                    self.b[index] = self.b[3];
                }
                (4, 1) => {
                    let tetrahedron = self.a.map(|v| v.pos);
                    let Some(index) = point_tetrahedron(self.b[0].pos, tetrahedron, -1.0) else {
                        return Reduced::Penetration;
                    };
                    self.dim_a = 3;
                    self.a[index] = self.a[3];
                }
                (2, 2) => {
                    if self.edge_edge() {
                        return Reduced::Done;
                    }
                }
                (2, 3) | (3, 2) => match self.edge_face() {
                    EdgeFace::Penetration => return Reduced::Penetration,
                    EdgeFace::EdgeEdge => {}
                    EdgeFace::Done => return Reduced::Done,
                },
                _ => return Reduced::Penetration,
            }
        }
    }

    /// Case `(1, 2)`: a vertex of A against an edge of B.
    fn point_segment_b(&mut self) {
        let (a0, b0, b1) = (self.a[0].pos, self.b[0].pos, self.b[1].pos);
        let to_end = b1 - a0;
        let edge = b1 - b0;
        let to_start = b0 - a0;
        let end_negative = dot(to_end, edge).is_sign_negative();
        let start_negative = dot(to_start, edge).is_sign_negative();
        if end_negative == start_negative {
            if end_negative {
                self.b[0] = self.b[1];
            }
            self.dim_b = 1;
            self.dir = self.a[0].pos - self.b[0].pos;
        } else {
            self.dir = edge.cross(to_end.cross(to_start));
        }
    }

    /// Case `(2, 1)`: an edge of A against a vertex of B.
    fn segment_point_a(&mut self) {
        let (a0, a1, b0) = (self.a[0].pos, self.a[1].pos, self.b[0].pos);
        let to_end = a1 - b0;
        let edge = a1 - a0;
        let to_start = a0 - b0;
        let end_negative = dot(to_end, edge).is_sign_negative();
        let start_negative = dot(to_start, edge).is_sign_negative();
        if end_negative == start_negative {
            if end_negative {
                self.a[0] = self.a[1];
            }
            self.dim_a = 1;
            self.dir = self.a[0].pos - self.b[0].pos;
        } else {
            self.dir = to_end.cross(to_start).cross(edge);
        }
    }

    /// Cases `(1, 3)` and `(3, 1)`: a vertex against a triangle. Returns `true` if the vertex is
    /// over the face; else the triangle drops to an edge or a vertex.
    fn point_face(&mut self, triangle_is_a: bool) -> bool {
        let (point, mut tri) = if triangle_is_a {
            (self.b[0].pos, self.a)
        } else {
            (self.a[0].pos, self.b)
        };
        let result = point_triangle(point, tri[0].pos, tri[1].pos, tri[2].pos);
        self.lanes = result.lanes;
        self.dir = result.normal;
        let mut dim = 3;
        let over_face = result.code == 7;
        if over_face {
            if dot(self.a[0].pos - self.b[0].pos, result.normal).is_sign_negative() {
                self.dir = -result.normal;
                tri.swap(0, 1);
                self.lanes.swap(0, 1);
                self.feature_change = true;
            }
        } else {
            let region = REGION_VERTEX[usize::try_from(result.code).unwrap_or(0)];
            if let Ok(index) = usize::try_from(region) {
                let first = NEXT_FIRST[index];
                let second = NEXT_SECOND[index];
                let origin = tri[index].pos;
                let offset = point - origin;
                let edge_first = tri[first].pos - origin;
                let edge_second = tri[second].pos - origin;
                dim = 2;
                let last = tri[2];
                if dot(edge_first, offset).is_sign_negative() {
                    let along_second = dot(edge_second, offset);
                    tri[first] = last;
                    if along_second.is_sign_negative() {
                        let slot = if second == 2 { first } else { second };
                        dim = 1;
                        tri[slot] = tri[1];
                    }
                } else {
                    tri[second] = last;
                }
            } else {
                let mut slot = usize::try_from(i32::from(region) + 8).unwrap_or(1);
                if slot > 3 {
                    slot = 1;
                }
                dim = 2;
                tri[slot] = tri[2];
            }
        }
        if triangle_is_a {
            (self.a, self.dim_a) = (tri, dim);
        } else {
            (self.b, self.dim_b) = (tri, dim);
        }
        over_face
    }

    /// Case `(2, 2)`: two edges. Returns `true` for an inner pair; else the edges drop to
    /// their clamped ends.
    fn edge_edge(&mut self) -> bool {
        let edge_a = self.a[1].pos - self.a[0].pos;
        let edge_b = self.b[1].pos - self.b[0].pos;
        let result = segment_segment(self.a[0].pos, edge_a, self.b[0].pos, edge_b);
        self.closest_on_a = result.point;
        if result.flags == 0 {
            let normal = edge_a.cross(edge_b);
            let side = dot(self.a[0].pos - self.b[0].pos, normal);
            self.dir = if side.is_sign_negative() {
                -normal
            } else {
                normal
            };
            return true;
        }
        if result.flags & 1 != 0 {
            self.a[0] = self.a[1];
            self.dim_a = 1;
        } else if result.flags & 2 != 0 {
            self.dim_a = 1;
        }
        if result.flags & 4 != 0 {
            self.b[0] = self.b[1];
            self.dim_b = 1;
        } else if result.flags & 8 != 0 {
            self.dim_b = 1;
        }
        false
    }
}

/// Returns `true` if `vertex` is farther along `direction` than each kept vertex by more than
/// the limit (the checks after each support call, for example at `0x140DC94F5`).
fn improves(
    kept: &[Vertex],
    vertex: Vec3,
    direction: Vec3,
    limit: f32,
    position: impl Fn(&Vertex) -> Vec3,
) -> bool {
    let gain = |other: &Vertex| {
        let d = dot(vertex - position(other), direction);
        d.abs() * d
    };
    if gain(&kept[0]) <= limit {
        return false;
    }
    // Retail checks the third vertex before the second one; both must pass.
    kept[1..].iter().rev().all(|other| limit <= gain(other))
}

/// The result of the edge–triangle step (`0x140DCB4B0`).
enum EdgeFace {
    /// The edge crosses the triangle.
    Penetration,
    /// The triangle dropped to an edge: run the edge–edge case.
    EdgeEdge,
    /// The edge dropped to a vertex over the face.
    Done,
}

impl Gsk<'_> {
    /// Cases `(2, 3)` and `(3, 2)`: an edge against a triangle (`0x140DCB4B0`).
    fn edge_face(&mut self) -> EdgeFace {
        let edge_is_a = self.dim_a == 2;
        let forbid_value = if edge_is_a { 1 } else { -1 };
        let from_edge_edge = self.last_dim_b == 2;
        let (edge, tri) = if edge_is_a {
            (self.a, self.b)
        } else {
            (self.b, self.a)
        };
        let (mut edge, mut tri) = (edge, tri);
        let (e0, e1) = (edge[0].pos, edge[1].pos);
        let (t0, t1, t2) = (tri[0].pos, tri[1].pos, tri[2].pos);
        let outcome = if from_edge_edge {
            let side_first = t1 - t2;
            let side_second = t0 - t2;
            let normal = side_first.cross(side_second);
            let s_end = dot(e1 - t0, normal);
            let s_start = dot(e0 - t0, normal);
            let mut crossing = false;
            if s_start.is_sign_negative() != s_end.is_sign_negative() {
                let t = s_start / (s_start - s_end);
                let point = (Vec3::ONE - Vec3::splat(t)) * e0 + e1 * t;
                let result = point_triangle(point, t0, t1, t2);
                self.lanes = result.lanes;
                if result.code == 7 {
                    return EdgeFace::Penetration;
                }
                crossing = true;
            }
            if !crossing {
                if s_start.abs() > s_end.abs() {
                    edge.swap(0, 1);
                }
                let near = edge[0].pos;
                let result = point_triangle(near, t0, t1, t2);
                self.lanes = result.lanes;
                self.dir = result.normal;
                if result.code == 7 {
                    let (a0, b0) = if edge_is_a { (near, t0) } else { (t0, near) };
                    if dot(a0 - b0, result.normal).is_sign_negative() {
                        self.dir = -result.normal;
                        tri.swap(0, 1);
                        self.lanes.swap(0, 1);
                        self.feature_change = true;
                    }
                    self.store_edge_face(edge_is_a, edge, 1, tri, 3);
                    return EdgeFace::Done;
                }
            }
            let along = edge[1].pos - edge[0].pos;
            let first = segment_segment(edge[0].pos, along, t2, side_first);
            let second = segment_segment(edge[0].pos, along, t2, side_second);
            let slot = usize::from(first.distance_squared > second.distance_squared);
            tri[slot] = tri[2];
            EdgeFace::EdgeEdge
        } else {
            let old = self.lanes;
            let s_end = dot(e1 - t0, self.dir);
            let s_start = dot(e0 - t0, self.dir);
            if (s_start * s_end).is_sign_negative() {
                let t = s_start / (s_start - s_end);
                let point = (Vec3::ONE - Vec3::splat(t)) * e0 + e1 * t;
                let result = point_triangle(point, t0, t1, t2);
                self.lanes = result.lanes;
                if result.code == 7 {
                    return EdgeFace::Penetration;
                }
            }
            let result = point_triangle(e1, t0, t1, t2);
            self.lanes = result.lanes;
            let new = result.lanes;
            if result.code == 7 {
                self.forbid = forbid_value;
                edge[0] = edge[1];
                self.store_edge_face(edge_is_a, edge, 1, tri, 3);
                return EdgeFace::Done;
            }
            let drop_first = |tri: &mut [Vertex; 4]| tri[0] = tri[2];
            let drop_second = |tri: &mut [Vertex; 4]| tri[1] = tri[2];
            match result.code {
                5 => drop_second(&mut tri),
                3 => {}
                1 => {
                    if (new[1] - old[1]) * old[2] <= (new[2] - old[2]) * old[1] {
                        drop_second(&mut tri);
                    }
                }
                2 => {
                    if (new[2] - old[2]) * old[0] > (new[0] - old[0]) * old[2] {
                        drop_first(&mut tri);
                    }
                }
                4 => {
                    if (new[0] - old[0]) * old[1] <= (new[1] - old[1]) * old[0] {
                        drop_first(&mut tri);
                    } else {
                        drop_second(&mut tri);
                    }
                }
                _ => drop_first(&mut tri),
            }
            EdgeFace::EdgeEdge
        };
        self.store_edge_face(edge_is_a, edge, 2, tri, 2);
        outcome
    }

    /// Writes the edge and triangle features back after the edge–triangle step.
    fn store_edge_face(
        &mut self,
        edge_is_a: bool,
        edge: [Vertex; 4],
        edge_dim: usize,
        tri: [Vertex; 4],
        tri_dim: usize,
    ) {
        if edge_is_a {
            (self.a, self.dim_a, self.b, self.dim_b) = (edge, edge_dim, tri, tri_dim);
        } else {
            (self.b, self.dim_b, self.a, self.dim_a) = (edge, edge_dim, tri, tri_dim);
        }
    }
}
