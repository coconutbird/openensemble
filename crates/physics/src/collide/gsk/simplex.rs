//! The geometric sub-solvers of the GSK closest-feature query.
//!
//! Each function is a port of one retail helper. The products and sums keep the retail order.

use glam::Vec3;

use crate::collide::convex::dot;

/// A vertex of a GSK feature.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Vertex {
    /// The position in the space of shape A.
    pub(super) pos: Vec3,
    /// The position in the space of its own shape.
    pub(super) local: Vec3,
    /// The vertex id of its own shape.
    pub(super) id: u16,
}

/// The region codes that select the vertex to keep (`0x141164820`). A negative value `v` means
/// "remove vertex `v + 8`" (a value above 3 counts as 1).
pub(super) const REGION_VERTEX: [i8; 8] = [-1, 0, 1, -6, 2, -7, -8, -1];

/// The first other vertex of each triangle vertex (`0x141164828`).
pub(super) const NEXT_FIRST: [usize; 3] = [2, 0, 1];

/// The second other vertex of each triangle vertex (`0x14116482A`).
pub(super) const NEXT_SECOND: [usize; 3] = [1, 2, 0];

/// The result of a point–triangle test (`0x140DCB290`).
#[derive(Clone, Copy, Debug)]
pub(super) struct PointTriangle {
    /// Bit `i` is set if the point is outside edge `i` (lane `i` is negative).
    pub(super) code: u32,
    /// The three edge lanes: edges `(t1, t2)`, `(t2, t0)` and `(t0, t1)`.
    pub(super) lanes: [f32; 3],
    /// The triangle normal `(t2 - t1) × (t0 - t2)`, not normalized.
    pub(super) normal: Vec3,
}

/// Tests a point against the edges of a triangle (`0x140DCB290`).
pub(super) fn point_triangle(point: Vec3, t0: Vec3, t1: Vec3, t2: Vec3) -> PointTriangle {
    let normal = (t2 - t1).cross(t0 - t2);
    let lanes = [
        dot((point - t1).cross(t2 - t1), normal),
        dot((point - t2).cross(t0 - t2), normal),
        dot((point - t0).cross(t1 - t0), normal),
    ];
    let code =
        u32::from(lanes[0] < 0.0) | u32::from(lanes[1] < 0.0) << 1 | u32::from(lanes[2] < 0.0) << 2;
    PointTriangle {
        code,
        lanes,
        normal,
    }
}

/// The closest points of two segments (`0x140DD6EB0`).
#[derive(Clone, Copy, Debug)]
pub(super) struct SegmentSegment {
    /// The closest point on the first segment.
    pub(super) point: Vec3,
    /// The squared distance.
    pub(super) distance_squared: f32,
    /// Bit 0: the first segment clamps to its end; bit 1: to its start; bit 2: the second
    /// segment clamps to its end; bit 3: to its start. Zero means an inner pair.
    pub(super) flags: u32,
}

/// Returns the closest points of the segments `p + s·dp` and `q + t·dq` (`0x140DD6EB0`).
pub(super) fn segment_segment(p: Vec3, dp: Vec3, q: Vec3, dq: Vec3) -> SegmentSegment {
    let offset = q - p;
    let cross_term = dot(dq, dp);
    let along_p = dot(dp, offset);
    let along_q = dot(dq, offset);
    let length_p = dot(dp, dp);
    let length_q = dot(dq, dq);
    let determinant = (length_p * length_q - cross_term * cross_term).abs();
    let numerator = along_p * length_q - along_q * cross_term;
    let (mut s, mut flags);
    if numerator * determinant >= determinant * determinant {
        (s, flags) = (1.0, 1);
    } else if numerator <= 0.0 {
        (s, flags) = (0.0, 2);
    } else if determinant <= (cross_term * cross_term + length_p * length_q) * 9.536_743e-7 {
        (s, flags) = (1.0, 1);
    } else {
        (s, flags) = (numerator / determinant, 0);
    }
    let t_numerator = cross_term * s - along_q;
    let t;
    if t_numerator < length_q && t_numerator > 0.0 {
        t = t_numerator / length_q;
    } else {
        if t_numerator < length_q {
            t = 0.0;
            flags = 8;
        } else {
            t = 1.0;
            flags = 4;
        }
        let s_numerator = cross_term * t + along_p;
        if s_numerator > 0.0 {
            if s_numerator < length_p {
                s = s_numerator / length_p;
            } else {
                s = 1.0;
                flags |= 1;
            }
        } else {
            s = 0.0;
            flags |= 2;
        }
    }
    let point = dp * s + p;
    let difference = point - (dq * t + q);
    SegmentSegment {
        point,
        distance_squared: dot(difference, difference),
        flags,
    }
}

/// Finds the tetrahedron vertex to remove for a point (`0x140DCBDA0`).
///
/// Only the three faces that touch the new vertex `t[3]` count. `sign` is `1` if the point is
/// a vertex of A and the tetrahedron is of B, and `-1` for the other case. Returns `None` if the
/// point is inside (the penetration case).
pub(super) fn point_tetrahedron(point: Vec3, t: [Vec3; 4], sign: f32) -> Option<usize> {
    let normals = [
        (t[2] - t[1]).cross(t[3] - t[1]),
        (t[0] - t[2]).cross(t[3] - t[2]),
        (t[1] - t[0]).cross(t[3] - t[0]),
    ];
    let offset = (point - t[3]) * sign;
    let distances = normals.map(|n| (offset.y * n.y + offset.x * n.x) + n.z * offset.z);
    let mut ratios = [0.0; 3];
    for (ratio, (normal, distance)) in ratios.iter_mut().zip(normals.iter().zip(distances)) {
        let length = dot(*normal, *normal);
        *ratio = if length == 0.0 {
            f32::MAX
        } else {
            distance.abs() * distance / length
        };
    }
    let (mut smaller, mut larger, mut result) = (ratios[0], ratios[1], 1);
    if ratios[0] > ratios[1] {
        (smaller, larger, result) = (ratios[1], ratios[0], 0);
    }
    let (second, largest);
    if larger <= ratios[2] {
        result = 2;
        (second, largest) = (larger, ratios[2]);
    } else {
        largest = larger;
        second = if smaller > ratios[2] {
            smaller
        } else {
            ratios[2]
        };
    }
    if largest < 1.000_000_1e-6 {
        return None;
    }
    if largest <= second * 1.1 {
        return tetrahedron_tie(point, t, distances);
    }
    Some(result)
}

/// Breaks a near tie between two faces of the tetrahedron test (`0x140DCC000`).
fn tetrahedron_tie(point: Vec3, t: [Vec3; 4], mut distances: [f32; 3]) -> Option<usize> {
    for i in 0..3 {
        let (first, second) = (NEXT_FIRST[i], NEXT_SECOND[i]);
        if distances[second] >= 0.0 && distances[first] >= 0.0 {
            let to_point = point - t[i];
            let to_apex = t[3] - t[i];
            let edge_second = t[second] - t[i];
            let edge_first = t[first] - t[i];
            let point_second = dot(to_point, edge_second);
            let point_first = dot(to_point, edge_first);
            let apex_second = dot(to_apex, edge_second);
            let apex_first = dot(to_apex, edge_first);
            if apex_second * point_first <= apex_first * point_second {
                distances[second] = -1.0;
            } else {
                distances[first] = -1.0;
            }
        }
    }
    (0..3).find(|&i| distances[i] > 0.0)
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::{point_triangle, segment_segment};

    #[test]
    fn a_point_over_the_triangle_is_in_the_face_region() {
        let result = point_triangle(Vec3::new(0.2, 0.2, 1.0), Vec3::ZERO, Vec3::X, Vec3::Y);
        assert_eq!(result.code, 7);
        assert_eq!(result.normal, Vec3::Z);
    }

    #[test]
    fn crossing_segments_meet_in_the_middle() {
        let result = segment_segment(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 1.0),
            Vec3::new(0.0, 2.0, 0.0),
        );
        assert_eq!(result.flags, 0);
        assert!((result.point - Vec3::ZERO).length() < 1e-6);
        assert!((result.distance_squared - 1.0).abs() < 1e-6);
    }
}
