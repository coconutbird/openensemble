//! The geometry queries of `hkpBoxBoxCollisionDetection`.
//!
//! All queries work in the space of box A. Box B is at `aTb` in this space. The extents of
//! both boxes include their convex radius, so the queries see the expanded boxes.

use glam::{Vec3, Vec3Swizzles};

use crate::math::Transform;

/// The `hkReal` epsilon that the retail code adds and compares with (`0x34000000`).
pub(super) const EPS: f32 = 1.192_092_9e-7;

/// The bias that breaks the tie for a zero component when a face query picks a vertex
/// (`0x3727C5AC`).
const TIE_BIAS: f32 = 1.0e-5;

/// The largest inverse axis length that still gives a valid edge axis (`0x14116477C`).
const MAX_EDGE_INV_LENGTH: f32 = 999.999_94;

/// The separation that replaces the separation of a degenerate edge axis (`0x141164780`,
/// about `-1e17`).
const DEGENERATE_SEPARATION: u32 = 0xDBB1_A2BC;

/// The value that marks a rejected axis in the closest-feature search (`0xFF7FFFEE`).
const REJECTED_SEPARATION: u32 = 0xFF7F_FFEE;

/// The two end limits of the edge parameters (`0x3F7FFFFE`).
const EDGE_PARAMETER_LIMIT: f32 = 0.999_999_9;

/// The shortest normal of an edge–edge point before normalisation (`0x1411F3BCC`).
const MIN_EDGE_NORMAL_LENGTH: f32 = 0.01;

/// The number of tries of the closest-feature search.
const CLOSEST_TRIES: u32 = 8;

/// A feature pair of the box–box manifold: the first two bytes of a retail manifold entry.
///
/// - `kind` 0–2: a vertex of B on the face of A with this axis.
/// - `kind` 4–6: a vertex of A on the face of B with the axis `kind - 4`.
/// - Any larger `kind`: an edge of A against an edge of B. The low four bits of `kind` and of
///   `vertex` are the edge axes. Bits 4–6 are the signs of the edge start vertices.
///
/// For a face point, bits 4–6 of `vertex` give the vertex (a set bit is a negative
/// component) and bit 3 is the sign of the face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Feature {
    /// The feature type byte.
    pub(super) kind: u8,
    /// The vertex and sign byte.
    pub(super) vertex: u8,
}

impl Feature {
    /// Returns `true` for a face–vertex feature (kinds 0–6).
    pub(super) fn is_face_vertex(self) -> bool {
        self.kind <= 6
    }

    /// Returns the face sign from bit 3 of the vertex byte.
    pub(super) fn sign(self) -> f32 {
        if self.vertex & 8 == 0 { 1.0 } else { -1.0 }
    }
}

/// Returns the signs of the vertex in bits 4–6 of a byte (the table at `0x1411646D0`).
pub(super) fn vertex_signs(bits: u8) -> Vec3 {
    let sign = |bit: u8| if bits & bit == 0 { 1.0 } else { -1.0 };
    Vec3::new(sign(0x10), sign(0x20), sign(0x40))
}

/// Returns bits 4–6 for the negative components of a vector (`movmskps` of `v < 0`).
pub(super) fn negative_bits(v: Vec3) -> u8 {
    u8::from(v.x < 0.0) << 4 | u8::from(v.y < 0.0) << 5 | u8::from(v.z < 0.0) << 6
}

/// Returns bit 3 if the sign bit of a value is set.
pub(super) fn sign_bit(value: f32) -> u8 {
    if value.is_sign_negative() { 8 } else { 0 }
}

/// The dot product in the retail order.
pub(super) fn dot3(a: Vec3, b: Vec3) -> f32 {
    (a.x * b.x + a.y * b.y) + a.z * b.z
}

/// Rotates a vector by the columns of a rotation in the retail order.
pub(super) fn rotate(columns: &[Vec3; 3], v: Vec3) -> Vec3 {
    (columns[1] * v.y + columns[0] * v.x) + columns[2] * v.z
}

/// Rotates a vector by the transpose of a rotation (`0x14086B850`).
pub(super) fn rotate_inverse(columns: &[Vec3; 3], v: Vec3) -> Vec3 {
    let row = |c: Vec3| (c.y * v.y + c.x * v.x) + c.z * v.z;
    Vec3::new(row(columns[0]), row(columns[1]), row(columns[2]))
}

/// Maps a point by the columns of a rotation and a translation in the retail order.
pub(super) fn apply(columns: &[Vec3; 3], translation: Vec3, v: Vec3) -> Vec3 {
    (columns[1] * v.y + columns[0] * v.x) + (columns[2] * v.z + translation)
}

/// Returns the columns of the rotation of a transform.
pub(super) fn columns(transform: &Transform) -> [Vec3; 3] {
    [
        transform.rotation.x_axis,
        transform.rotation.y_axis,
        transform.rotation.z_axis,
    ]
}

/// The detection context that `hkpBoxBoxAgent::processCollision` (`0x140D956C0`) builds.
pub(super) struct Context {
    /// The rotation columns of `aTb`.
    pub(super) r: [Vec3; 3],
    /// The translation of `aTb`.
    pub(super) t: Vec3,
    /// `Rᵀ t`: the centre of B in the axes of B, with the sign changed.
    pub(super) rt_t: Vec3,
    /// The half extents of A plus its radius.
    pub(super) ext_a: Vec3,
    /// The half extents of B plus its radius.
    pub(super) ext_b: Vec3,
    /// `ext_a` plus the tolerance.
    pub(super) ext_a_tol: Vec3,
    /// `ext_b` plus the tolerance.
    pub(super) ext_b_tol: Vec3,
    /// The collision tolerance.
    pub(super) tolerance: f32,
    /// The rotation columns of `wTa`.
    pub(super) wa: [Vec3; 3],
    /// The translation of `wTa`.
    pub(super) wa_t: Vec3,
    /// The rotation columns of `wTb`.
    pub(super) wb: [Vec3; 3],
    /// The translation of `wTb`.
    pub(super) wb_t: Vec3,
    /// The separations of the axes, in the retail layout: faces of A at 0–2, faces of B at
    /// 4–6, and the edge axes of A axis `i` at `8 + 4 i + j`.
    pub(super) separation: [f32; 19],
}

/// A point that the closest-feature search found.
#[derive(Clone, Copy, Debug)]
pub(super) struct Found {
    /// The feature pair.
    pub(super) feature: Feature,
    /// The face sign (face features only).
    pub(super) sign: f32,
    /// The geometry, or `None` if the manifold has the feature already.
    pub(super) point: Option<FoundPoint>,
}

/// The geometry of a new point of the closest-feature search.
#[derive(Clone, Copy, Debug)]
pub(super) struct FoundPoint {
    /// The point in the space of A (the vertex of B, the vertex of A or the point on the edge
    /// of A).
    pub(super) pos_a: Vec3,
    /// The vertex of B in the space of B, or the vertex of A in the space of B.
    pub(super) pos_b: Vec3,
    /// The edge–edge normal in the space of A (edge features only).
    pub(super) edge_normal: Vec3,
    /// The distance between the expanded boxes.
    pub(super) distance: f32,
}

/// An edge–edge point in the space of A (`0x140DD2210`).
#[derive(Clone, Copy, Debug)]
pub(super) struct EdgePoint {
    /// The closest point on the edge of A.
    pub(super) pos_a: Vec3,
    /// The normal from B toward A.
    pub(super) normal: Vec3,
    /// The distance between the edges.
    pub(super) distance: f32,
}

impl Context {
    /// Builds the context for two boxes: `half_*` are the half extents, `radius_*` the radii.
    pub(super) fn new(
        wta: &Transform,
        wtb: &Transform,
        half_a: Vec3,
        radius_a: f32,
        half_b: Vec3,
        radius_b: f32,
        tolerance: f32,
    ) -> Self {
        let atb = wta.inverse() * *wtb;
        let r = columns(&atb);
        let t = atb.translation;
        let ext_a = Vec3::splat(radius_a) + half_a;
        let ext_b = Vec3::splat(radius_b) + half_b;
        Self {
            r,
            t,
            rt_t: rotate_inverse(&r, t),
            ext_a,
            ext_b,
            ext_a_tol: Vec3::splat(tolerance) + ext_a,
            ext_b_tol: Vec3::splat(tolerance) + ext_b,
            tolerance,
            wa: columns(wta),
            wa_t: wta.translation,
            wb: columns(wtb),
            wb_t: wtb.translation,
            separation: [0.0; 19],
        }
    }

    /// Returns the row `k` of the rotation of `aTb`.
    fn row(&self, k: usize) -> Vec3 {
        Vec3::new(self.r[0][k], self.r[1][k], self.r[2][k])
    }

    /// Returns `true` if any axis separates the boxes by more than `threshold`
    /// (`checkIntersection`, `0x140DD0C80`). Else it stores all separations.
    pub(super) fn separated(&mut self, threshold: f32) -> bool {
        let abs = [self.r[0].abs(), self.r[1].abs(), self.r[2].abs()];
        let (ea, eb) = (self.ext_a, self.ext_b);
        let face_a = self.t.abs() - (((abs[1] * eb.y + abs[0] * eb.x) + abs[2] * eb.z) + ea);
        if (0..3).any(|k| threshold < face_a[k]) {
            return true;
        }
        let face_b = self.rt_t.abs()
            - Vec3::new(
                ((ea.y * abs[0].y + ea.x * abs[0].x) + ea.z * abs[0].z) + eb.x,
                ((ea.y * abs[1].y + ea.x * abs[1].x) + ea.z * abs[1].z) + eb.y,
                ((ea.y * abs[2].y + ea.x * abs[2].x) + ea.z * abs[2].z) + eb.z,
            );
        if (0..3).any(|k| threshold < face_b[k]) {
            return true;
        }
        for k in 0..3 {
            self.separation[k] = face_a[k];
            self.separation[4 + k] = face_b[k];
        }
        for i in 0..3 {
            let (i1, i2) = ((i + 1) % 3, (i + 2) % 3);
            let (row_i, row_i1, row_i2) = (self.row(i), self.row(i1), self.row(i2));
            let mut group = [0.0; 3];
            let mut inv_lengths = [0.0; 3];
            for j in 0..3 {
                let len2 = row_i2[j] * row_i2[j] + row_i1[j] * row_i1[j];
                let inv_length = 1.0 / (len2 + EPS).sqrt();
                let t_proj = (self.t[i2] * row_i1[j] - self.t[i1] * row_i2[j]).abs();
                let mut proj = ea[i2] * row_i1[j].abs() + ea[i1] * row_i2[j].abs();
                for k in (0..3).filter(|&k| k != j) {
                    proj += eb[k] * row_i[3 - j - k].abs();
                }
                group[j] = (t_proj - proj) * inv_length;
                inv_lengths[j] = inv_length;
            }
            if group.iter().any(|&separation| threshold < separation) {
                return true;
            }
            for j in 0..3 {
                self.separation[8 + 4 * i + j] = if MAX_EDGE_INV_LENGTH < inv_lengths[j] {
                    f32::from_bits(DEGENERATE_SEPARATION)
                } else {
                    group[j]
                };
            }
        }
        false
    }

    /// Finds the feature pair of the axis with the largest separation
    /// (`findClosestPoint`, `0x140DD15E0`).
    ///
    /// `known` returns `true` for a feature that the manifold has already. The search rejects
    /// an axis whose feature is outside the tolerance and tries again, at most 8 times.
    pub(super) fn find_closest(&mut self, known: impl Fn(Feature) -> bool) -> Option<Found> {
        const ORDER: [usize; 15] = [0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14, 16, 17, 18];
        for _ in 0..CLOSEST_TRIES {
            let mut best = 0;
            for &index in &ORDER[1..] {
                if self.separation[index] > self.separation[best] {
                    best = index;
                }
            }
            let found = match best {
                0..=2 => self.face_a(best, &known),
                4..=6 => self.face_b(best - 4, &known),
                _ => self.edge(best - 8, &known),
            };
            if found.is_some() {
                return found;
            }
            self.separation[best] = f32::from_bits(REJECTED_SEPARATION);
        }
        None
    }

    /// The deepest vertex of B on the face `k` of A.
    fn face_a(&self, k: usize, known: &impl Fn(Feature) -> bool) -> Option<Found> {
        let (sign, mut vertex_b) = if 0.0 > self.t[k] {
            (-1.0, self.ext_b)
        } else {
            (1.0, -self.ext_b)
        };
        let row = self.row(k);
        for j in 0..3 {
            let bias = if 0.0 < self.rt_t[j] {
                TIE_BIAS
            } else {
                -TIE_BIAS
            };
            if (bias * sign + row[j]).is_sign_negative() {
                vertex_b[j] = -vertex_b[j];
            }
        }
        let pos_a = apply(&self.r, self.t, vertex_b);
        let feature = Feature {
            kind: index_u8(k),
            vertex: sign_bit(sign) | negative_bits(vertex_b),
        };
        if known(feature) {
            return Some(Found {
                feature,
                sign,
                point: None,
            });
        }
        if !(0..3).all(|i| pos_a[i].abs() < self.ext_a_tol[i]) {
            return None;
        }
        Some(Found {
            feature,
            sign,
            point: Some(FoundPoint {
                pos_a,
                pos_b: vertex_b,
                edge_normal: Vec3::ZERO,
                distance: pos_a[k] * sign - self.ext_a[k],
            }),
        })
    }

    /// The deepest vertex of A on the face `k` of B.
    fn face_b(&self, k: usize, known: &impl Fn(Feature) -> bool) -> Option<Found> {
        let (sign, mut vertex_a) = if 0.0 > self.rt_t[k] {
            (-1.0, -self.ext_a)
        } else {
            (1.0, self.ext_a)
        };
        let column = self.r[k];
        for i in 0..3 {
            let bias = if 0.0 < self.t[i] { -TIE_BIAS } else { TIE_BIAS };
            if (bias * -sign + column[i]).is_sign_negative() {
                vertex_a[i] = -vertex_a[i];
            }
        }
        let pos_b = rotate_inverse(&self.r, vertex_a - self.t);
        let feature = Feature {
            kind: index_u8(k + 4),
            vertex: sign_bit(sign) | negative_bits(vertex_a),
        };
        if known(feature) {
            return Some(Found {
                feature,
                sign,
                point: None,
            });
        }
        if !(0..3).all(|i| pos_b[i].abs() < self.ext_b_tol[i]) {
            return None;
        }
        Some(Found {
            feature,
            sign,
            point: Some(FoundPoint {
                pos_a: vertex_a,
                pos_b,
                edge_normal: Vec3::ZERO,
                distance: -(pos_b[k] * sign) - self.ext_b[k],
            }),
        })
    }

    /// The closest edges for the edge axis `index` (`4 i + j`).
    fn edge(&self, index: usize, known: &impl Fn(Feature) -> bool) -> Option<Found> {
        let (i, j) = (index >> 2, index & 3);
        let mut unit = Vec3::ZERO;
        unit[i] = 1.0;
        let c = self.r[j];
        let mut axis = unit.yzx() * c.zxy() - unit.zxy() * c.yzx();
        if 0.0 > dot3(axis, self.t) {
            axis = -axis;
        }
        let axis_b = rotate_inverse(&self.r, axis);
        let mut a_bits = axis;
        a_bits[i] = 1.0;
        let mut b_bits = axis_b;
        b_bits[j] = 1.0;
        let feature = Feature {
            kind: 0x80 | negative_bits(a_bits) | index_u8(i),
            vertex: (!negative_bits(b_bits) & 0x70) | index_u8(j),
        };
        if known(feature) {
            return Some(Found {
                feature,
                sign: 1.0,
                point: None,
            });
        }
        let edge = self.edge_edge(feature)?;
        Some(Found {
            feature,
            sign: 1.0,
            point: Some(FoundPoint {
                pos_a: edge.pos_a,
                pos_b: Vec3::ZERO,
                edge_normal: edge.normal,
                distance: edge.distance,
            }),
        })
    }

    /// Finds the closest points of an edge pair (`checkEdgeEdge`, `0x140DD2210`).
    ///
    /// Returns `None` if the closest points are not inside both edges, the edges are almost
    /// parallel, or the distance is more than the tolerance.
    pub(super) fn edge_edge(&self, feature: Feature) -> Option<EdgePoint> {
        let i = usize::from(feature.kind & 0xF);
        let j = usize::from(feature.vertex & 0xF);
        let signs_a = vertex_signs(feature.kind);
        let mut vertex_a = self.ext_a * signs_a;
        let vertex_b = vertex_signs(feature.vertex) * self.ext_b;
        let half_b = self.ext_b[j];
        let dir_b = self.r[j] * (half_b * 2.0);
        let dir_a = self.ext_a[i] * -2.0;
        let bb = (half_b * half_b) * 4.0;
        let ab = dir_a * dir_b[i];
        let diff = apply(&self.r, self.t, vertex_b) - vertex_a;
        let diff_b = dot3(diff, dir_b);
        let den = ((dir_a * dir_a) * bb - ab * ab).abs();
        let num = ((dir_a * diff[i]) * bb) - diff_b * ab;
        if den * EPS >= num || num >= den * EDGE_PARAMETER_LIMIT {
            return None;
        }
        let s = num / den;
        let u = s * ab - diff_b;
        if bb * EPS >= u || u >= bb * EDGE_PARAMETER_LIMIT {
            return None;
        }
        vertex_a[i] += s * dir_a;
        let mut half_a = Vec3::ZERO;
        half_a[i] = dir_a * 0.5;
        let cross = half_a.yzx() * dir_b.zxy() - half_a.zxy() * dir_b.yzx();
        let len2 = dot3(cross, cross);
        let inv_length = 1.0 / len2.sqrt();
        if MIN_EDGE_NORMAL_LENGTH > len2 * inv_length {
            return None;
        }
        let mut normal = cross * inv_length;
        if dot3(signs_a, normal) < 0.0 {
            normal = -normal;
        }
        let distance = dot3(normal, diff);
        (distance <= self.tolerance).then_some(EdgePoint {
            pos_a: vertex_a,
            normal: -normal,
            distance,
        })
    }
}

/// Changes an axis index (0–6) into a byte.
pub(super) fn index_u8(index: usize) -> u8 {
    u8::try_from(index).unwrap_or(u8::MAX)
}
