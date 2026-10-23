//! The penetration case of GSK (`0x140DCADF0`).
//!
//! If the core shapes touch or overlap, retail calls its penetration depth solver
//! (`0x140DE7020`, `TtPenetration`). That solver is not ported. This module uses an expanding
//! polytope on the Minkowski difference instead. It keeps the retail exit rules of
//! `0x140DCADF0`:
//!
//! - The features are the vertices of the closest polytope face, at most three of each shape.
//! - A repeated vertex id drops from its feature.
//! - The search direction is the penetration normal from B to A.

use glam::Vec3;

use super::simplex::{Vertex, segment_segment};
use super::solver::Gsk;
use crate::collide::convex::{dot, inverse_rotate, transform_point};

/// The largest number of expansion steps.
const MAX_STEPS: usize = 64;

/// The expansion stops when the new support point is at most this much farther.
const ACCURACY: f32 = 1e-5;

/// A point of the Minkowski difference `A - B` with the two shape vertices.
#[derive(Clone, Copy)]
struct Support {
    point: Vec3,
    a: Vertex,
    b: Vertex,
}

/// A face of the polytope with its outward unit normal and its distance from the origin.
#[derive(Clone, Copy)]
struct Face {
    corners: [usize; 3],
    normal: Vec3,
    distance: f32,
}

/// Sets the features and the direction of a GSK query whose cores overlap.
pub(super) fn solve(gsk: &mut Gsk<'_>) {
    let mut directions = vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
    for x in [1.0, -1.0] {
        for y in [1.0, -1.0] {
            for z in [1.0, -1.0] {
                directions.push(Vec3::new(x, y, z));
            }
        }
    }
    let mut candidates: Vec<Support> = Vec::new();
    for direction in directions {
        let candidate = support(gsk, direction);
        if candidates.iter().all(|c| c.point != candidate.point) {
            candidates.push(candidate);
        }
    }
    let Some(start) = start_tetrahedron(&candidates) else {
        gsk.dim_a = 1;
        gsk.dim_b = 1;
        gsk.dir = Vec3::Y;
        return;
    };
    let mut points: Vec<Support> = start.iter().map(|&i| candidates[i]).collect();
    let inside = points.iter().fold(Vec3::ZERO, |sum, p| sum + p.point) * 0.25;
    let mut faces: Vec<Face> = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]]
        .iter()
        .filter_map(|&corners| face(&points, corners, inside))
        .collect();
    for (i, candidate) in candidates.iter().enumerate() {
        if !start.contains(&i) {
            insert(&mut points, &mut faces, *candidate, inside);
        }
    }
    let mut best = None;
    for _ in 0..MAX_STEPS {
        let Some(closest) = faces
            .iter()
            .copied()
            .reduce(|a, b| if b.distance < a.distance { b } else { a })
        else {
            break;
        };
        best = Some(closest);
        let new = support(gsk, closest.normal);
        if dot(new.point, closest.normal) - closest.distance <= ACCURACY {
            break;
        }
        insert(&mut points, &mut faces, new, inside);
    }
    let Some(closest) = best else {
        gsk.dim_a = 1;
        gsk.dim_b = 1;
        gsk.dir = Vec3::Y;
        return;
    };
    let corners = closest.corners.map(|i| points[i]);
    gsk.a[..3].copy_from_slice(&corners.map(|c| c.a));
    gsk.b[..3].copy_from_slice(&corners.map(|c| c.b));
    gsk.dim_a = unique_dim(&mut gsk.a);
    gsk.dim_b = unique_dim(&mut gsk.b);
    while gsk.dim_a + gsk.dim_b > 4 {
        if gsk.dim_b <= gsk.dim_a {
            gsk.dim_b -= 1;
        } else {
            gsk.dim_a -= 1;
        }
    }
    if gsk.dim_a == 2 && gsk.dim_b == 2 {
        let result = segment_segment(
            gsk.a[0].pos,
            gsk.a[1].pos - gsk.a[0].pos,
            gsk.b[0].pos,
            gsk.b[1].pos - gsk.b[0].pos,
        );
        gsk.closest_on_a = result.point;
    }
    gsk.dir = -closest.normal;
    gsk.feature_change = true;
}

/// Drops repeated ids from a feature of three vertices, like `0x140DCADF0`.
fn unique_dim(vertices: &mut [Vertex; 4]) -> usize {
    let mut dim = if vertices[1].id == vertices[2].id || vertices[0].id == vertices[2].id {
        2
    } else {
        3
    };
    if vertices[1].id == vertices[0].id {
        dim -= 1;
        vertices[0] = vertices[dim];
    }
    dim
}

fn support(gsk: &Gsk<'_>, direction: Vec3) -> Support {
    let (a, a_id) = gsk.shape_a.support(direction);
    let (b_local, b_id) = gsk.shape_b.support(inverse_rotate(&gsk.b_to_a, -direction));
    let b = transform_point(&gsk.b_to_a, b_local);
    Support {
        point: a - b,
        a: Vertex {
            pos: a,
            local: a,
            id: a_id,
        },
        b: Vertex {
            pos: b,
            local: b_local,
            id: b_id,
        },
    }
}

/// Makes a face with its normal away from the inner point `inside`.
fn face(points: &[Support], corners: [usize; 3], inside: Vec3) -> Option<Face> {
    let [p0, p1, p2] = corners.map(|i| points[i].point);
    let normal = (p1 - p0).cross(p2 - p0);
    let length = dot(normal, normal).sqrt();
    if length <= f32::EPSILON {
        return None;
    }
    let mut normal = normal / length;
    let mut corners = corners;
    if dot(normal, p0 - inside) < 0.0 {
        normal = -normal;
        corners.swap(1, 2);
    }
    Some(Face {
        corners,
        normal,
        distance: dot(normal, p0),
    })
}

/// Returns four candidates that span a volume, the first ones found.
fn start_tetrahedron(candidates: &[Support]) -> Option<[usize; 4]> {
    let n = candidates.len();
    let point = |i: usize| candidates[i].point;
    for i in 1..n {
        for j in (i + 1)..n {
            let normal = (point(i) - point(0)).cross(point(j) - point(0));
            if dot(normal, normal) <= f32::EPSILON {
                continue;
            }
            for k in (j + 1)..n {
                if dot(normal, point(k) - point(0)).abs() > f32::EPSILON {
                    return Some([0, i, j, k]);
                }
            }
        }
    }
    None
}

/// Adds a point to the hull: removes the faces that see it and closes the hole.
fn insert(points: &mut Vec<Support>, faces: &mut Vec<Face>, new: Support, inside: Vec3) {
    let index = points.len();
    points.push(new);
    let mut horizon: Vec<[usize; 2]> = Vec::new();
    faces.retain(|f| {
        let visible = dot(new.point - points[f.corners[0]].point, f.normal) > 0.0;
        if visible {
            for edge in [[0, 1], [1, 2], [2, 0]] {
                let edge = [f.corners[edge[0]], f.corners[edge[1]]];
                if let Some(pos) = horizon.iter().position(|e| *e == [edge[1], edge[0]]) {
                    horizon.remove(pos);
                } else {
                    horizon.push(edge);
                }
            }
        }
        !visible
    });
    faces.extend(
        horizon
            .iter()
            .filter_map(|edge| face(points, [edge[0], edge[1], index], inside)),
    );
}
