//! The points of two capsules (`hkCollideCapsuleUtilCapsVsCaps`, `0x140DD79B0`).
//!
//! The function finds the closest points of the two segments. This gives point 0. If the
//! segments are almost parallel, it also tests the segment ends and can give points 1 and 2.
//! Each point slot starts with the distance `tolerance`. A slot that the function does not fill
//! keeps that distance, so the agent removes its point.

use glam::Vec3;

use super::{normalize, perpendicular};
use crate::contact::ContactPoint;

/// The smallest squared length that gives a normal (`dword_1411F3B3C`, `FLT_EPSILON`).
const EPSILON: f32 = 1.192_092_9e-7;

/// The relative determinant below which the segments count as parallel (`dword_1411F3B44`).
const PARALLEL_EPSILON: f32 = 9.536_743e-7;

/// The squared sine of the segment angle above which only point 0 exists
/// (`dword_1411F3C90`).
const END_POINT_SINE_SQUARED: f32 = 0.2;

/// The segment of capsule A, turned so that it points the same way as segment B.
struct SegmentA {
    start: Vec3,
    end: Vec3,
    direction: Vec3,
}

/// The parameters of the closest points: `t` on segment A, `u` on segment B and the clamp
/// flags (1: `t` = 1, 2: `t` = 0, 4: `u` = 1, 8: `u` = 0).
struct Closest {
    t: f32,
    u: f32,
    flags: u32,
}

/// Returns the three point slots of capsule A against capsule B. The segments are in world
/// space.
pub(super) fn capsule_capsule(
    segment_a: [Vec3; 2],
    radius_a: f32,
    segment_b: [Vec3; 2],
    radius_b: f32,
    tolerance: f32,
) -> [ContactPoint; 3] {
    let mut points = [ContactPoint {
        distance: tolerance,
        ..ContactPoint::default()
    }; 3];
    let direction_a = segment_a[1] - segment_a[0];
    let direction_b = segment_b[1] - segment_b[0];
    let dot = direction_a.dot(direction_b);
    let (a, dot) = if dot < 0.0 {
        (
            SegmentA {
                start: segment_a[1],
                end: segment_a[0],
                direction: -direction_a,
            },
            -dot,
        )
    } else {
        (
            SegmentA {
                start: segment_a[0],
                end: segment_a[1],
                direction: direction_a,
            },
            dot,
        )
    };
    let offset = segment_b[0] - a.start;
    let offset_a = offset.dot(a.direction);
    let offset_b = offset.dot(direction_b);
    let length_a = a.direction.dot(a.direction);
    let dot_squared = dot * dot;
    let length_b = direction_b.dot(direction_b);
    let lengths = length_b * length_a;
    let determinant = (lengths - dot_squared).abs();
    let closest = closest_parameters(
        offset_a,
        offset_b,
        length_a,
        length_b,
        dot,
        lengths,
        determinant,
    );
    let radius_sum = radius_a + radius_b;
    let on_a = a.direction * closest.t + a.start;
    let on_b = direction_b * closest.u + segment_b[0];
    let delta = on_a - on_b;
    let mut limit = radius_sum + points[0].distance;
    if EPSILON > limit {
        limit = EPSILON;
    }
    let length_squared = delta.dot(delta);
    if limit * limit <= length_squared {
        let inverse_length = 1.0 / length_squared.sqrt();
        points[0].normal = delta * inverse_length;
        points[0].distance = length_squared * inverse_length - radius_sum;
        return points;
    }
    let (normal, length) = if EPSILON > length_squared {
        (normalize(perpendicular(a.direction)), 0.0)
    } else {
        let inverse_length = 1.0 / length_squared.sqrt();
        (delta * inverse_length, length_squared * inverse_length)
    };
    points[0] = ContactPoint {
        position: normal * (radius_b - length) + on_a,
        normal,
        distance: length - radius_sum,
    };
    if determinant > length_a * END_POINT_SINE_SQUARED * length_b {
        return points;
    }
    end_points(
        &mut points,
        &a,
        segment_b,
        offset_a,
        -offset_b,
        &closest,
        [length_a, length_b, dot, radius_sum, radius_b],
    );
    points
}

/// Returns the parameters of the closest points of the two segments, with the clamps and the
/// parallel test of `0x140DD7A99`–`0x140DD7C4F`.
fn closest_parameters(
    offset_a: f32,
    offset_b: f32,
    length_a: f32,
    length_b: f32,
    dot: f32,
    lengths: f32,
    determinant: f32,
) -> Closest {
    let numerator = length_b * offset_a - offset_b * dot;
    let (mut t, mut flags) = if numerator * determinant >= determinant * determinant {
        (1.0, 1)
    } else if numerator <= 0.0 {
        (0.0, 2)
    } else if determinant > (lengths.abs() + dot * dot) * PARALLEL_EPSILON {
        (numerator / determinant, 0)
    } else {
        (1.0, 1)
    };
    let along_b = t * dot - offset_b;
    let u = if along_b >= length_b {
        flags = 4;
        1.0
    } else if along_b <= 0.0 {
        flags = 8;
        0.0
    } else {
        return Closest {
            t,
            u: along_b / length_b,
            flags,
        };
    };
    let along_a = u * dot + offset_a;
    if along_a <= 0.0 {
        t = 0.0;
        flags |= 2;
    } else if along_a >= length_a {
        t = 1.0;
        flags |= 1;
    } else {
        t = along_a / length_a;
    }
    Closest { t, u, flags }
}

/// Fills points 1 and 2 from the segment ends (`0x140DD7E7D`–`0x140DD80C2`).
///
/// Pass 1 starts at the start of segment A and the start of segment B. Pass 2 starts at the
/// two ends, with both directions turned. A pass is skipped if the closest points already use
/// one of its two ends. `scalars` holds the squared lengths of A and B, the dot product of
/// the directions, the radius sum and the radius of B.
fn end_points(
    points: &mut [ContactPoint; 3],
    a: &SegmentA,
    segment_b: [Vec3; 2],
    offset_a: f32,
    offset_b_negated: f32,
    closest: &Closest,
    scalars: [f32; 5],
) {
    let [length_a, length_b, dot, radius_sum, radius_b] = scalars;
    let inverse_lengths = 1.0 / (length_b * length_a);
    let inverse_length_a = length_b * inverse_lengths;
    let inverse_length_b = length_a * inverse_lengths;
    let mut start_a = a.start;
    let mut start_b = segment_b[0];
    let mut direction_a = a.direction;
    let mut direction_b = segment_b[1] - segment_b[0];
    let mut projection_a = offset_a;
    let mut projection_b = offset_b_negated;
    let mut mask = 10;
    let normal_0 = points[0].normal;
    for point in &mut points[1..] {
        if closest.flags & mask == 0 {
            let pair = if projection_b > 0.0 {
                if projection_b > length_b || (projection_a > 0.0 && projection_a > length_a) {
                    None
                } else if projection_a > 0.0
                    && projection_b * projection_b * inverse_length_b
                        > projection_a * projection_a * inverse_length_a
                {
                    Some((
                        start_a + direction_a * (projection_a * inverse_length_a),
                        start_b,
                    ))
                } else {
                    Some((
                        start_a,
                        start_b + direction_b * (projection_b * inverse_length_b),
                    ))
                }
            } else if projection_a <= 0.0 {
                Some((start_a, start_b))
            } else if projection_a > length_a {
                None
            } else {
                Some((
                    start_a + direction_a * (projection_a * inverse_length_a),
                    start_b,
                ))
            };
            if let Some((on_a, on_b)) = pair {
                end_point(point, normal_0, on_a, on_b, radius_sum, radius_b);
            }
        }
        start_a = a.end;
        start_b = segment_b[1];
        projection_b = (length_b - projection_b) - dot;
        projection_a = (length_a - projection_a) - dot;
        direction_a = -direction_a;
        direction_b = -direction_b;
        mask = 5;
    }
}

/// Fills one end point slot if the two points are nearer than `radius_sum` plus the slot
/// distance (`0x140DD7F5E`–`0x140DD805E`). A zero length takes the normal of point 0.
fn end_point(
    point: &mut ContactPoint,
    normal_0: Vec3,
    on_a: Vec3,
    on_b: Vec3,
    radius_sum: f32,
    radius_b: f32,
) {
    let limit = radius_sum + point.distance;
    let delta = on_a - on_b;
    let length_squared = delta.dot(delta);
    if limit * limit <= length_squared {
        return;
    }
    let length = length_squared.sqrt();
    let normal = if length_squared > EPSILON {
        delta * (1.0 / length_squared.sqrt())
    } else {
        normal_0
    };
    *point = ContactPoint {
        position: normal * (radius_b - length) + on_a,
        normal,
        distance: length - radius_sum,
    };
}
