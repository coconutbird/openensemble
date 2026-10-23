//! The point of a sphere against a sphere, a capsule or a box.
//!
//! Each function returns the point in the agent order: the normal points from B toward A, the
//! position is on the surface of B, and the distance is positive for a gap. `None` means that
//! the agent removes its point.

use glam::Vec3;

use super::{normalize, perpendicular};
use crate::contact::ContactPoint;
use crate::math::Transform;

/// The normal of two spheres with the same centre (`xmmword_1414A95A0`).
const COINCIDENT_NORMAL: Vec3 = Vec3::X;

/// Returns the point of sphere A against sphere B (`hkpSphereSphereAgent::processCollision`,
/// `0x140D9DCC0`).
///
/// The point exists while `|cA − cB| < rA + rB + tolerance`.
pub(super) fn sphere_sphere(
    centre_a: Vec3,
    radius_a: f32,
    centre_b: Vec3,
    radius_b: f32,
    tolerance: f32,
) -> Option<ContactPoint> {
    let delta = centre_a - centre_b;
    let length_squared = delta.dot(delta);
    let limit = radius_a + radius_b + tolerance;
    if limit * limit <= length_squared {
        return None;
    }
    let (normal, distance) = if length_squared <= 0.0 {
        (COINCIDENT_NORMAL, -(radius_b + radius_a))
    } else {
        let inverse_length = 1.0 / length_squared.sqrt();
        (
            delta * inverse_length,
            length_squared * inverse_length - (radius_b + radius_a),
        )
    };
    Some(ContactPoint {
        position: normal * radius_b + centre_b,
        normal,
        distance,
    })
}

/// Returns the point of the segment from `start` to `end` that is nearest to `point`
/// (`hkLineSegmentUtil::closestPointLineSeg`, `0x140DD7120`).
fn closest_point_on_segment(point: Vec3, start: Vec3, end: Vec3) -> Vec3 {
    let direction = end - start;
    let projection = (point - start).dot(direction);
    let length_squared = direction.dot(direction);
    if projection > 0.0 {
        if projection < length_squared {
            direction * (projection / length_squared) + start
        } else {
            start + direction
        }
    } else {
        start
    }
}

/// Returns the point of sphere A against capsule B (`hkpSphereCapsuleAgent::processCollision`,
/// `0x140D9F220`). `segment` is the capsule segment in world space.
///
/// The point exists while the sphere centre is nearer than
/// `sphere_radius + capsule_radius + tolerance` to the segment.
pub(super) fn sphere_capsule(
    centre: Vec3,
    sphere_radius: f32,
    segment: [Vec3; 2],
    capsule_radius: f32,
    tolerance: f32,
) -> Option<ContactPoint> {
    let closest = closest_point_on_segment(centre, segment[0], segment[1]);
    let delta = centre - closest;
    let radius_sum = sphere_radius + capsule_radius;
    let length_squared = delta.dot(delta);
    let limit = radius_sum + tolerance;
    if length_squared >= limit * limit {
        return None;
    }
    let (direction, length) = if length_squared <= 0.0 {
        (perpendicular(segment[1] - segment[0]), 0.0)
    } else {
        (delta, length_squared.sqrt())
    };
    let normal = normalize(direction);
    Some(ContactPoint {
        position: normal * (capsule_radius - length) + centre,
        normal,
        distance: length - radius_sum,
    })
}

/// Returns the point of sphere A against box B (`hkpSphereBoxAgent::processCollision`,
/// `0x140DA24B0`). `box_transform` is the world transform of the box.
///
/// If the sphere centre is inside the box core, the normal is the face axis with the smallest
/// depth and the point always exists. Otherwise the point exists while its distance is at most
/// `tolerance`.
pub(super) fn sphere_box(
    centre: Vec3,
    sphere_radius: f32,
    box_transform: &Transform,
    half_extents: Vec3,
    box_radius: f32,
    tolerance: f32,
) -> Option<ContactPoint> {
    let local = inverse_transform_point(box_transform, centre);
    let magnitude = local.abs();
    let outside = magnitude.min(half_extents) - magnitude;
    let columns = [
        box_transform.rotation.x_axis,
        box_transform.rotation.y_axis,
        box_transform.rotation.z_axis,
    ];
    let (normal, distance) = if outside.cmplt(Vec3::ZERO).any() {
        let length_squared = outside.dot(outside);
        let inverse_length = 1.0 / length_squared.sqrt();
        let distance = length_squared * inverse_length - (box_radius + sphere_radius);
        if distance > tolerance {
            return None;
        }
        let scaled = (outside * inverse_length).to_array();
        let signs = local.to_array();
        let local_normal: [f32; 3] = std::array::from_fn(|axis| {
            let value = if signs[axis].is_sign_negative() {
                -scaled[axis]
            } else {
                scaled[axis]
            };
            -value
        });
        let normal = (columns[1] * local_normal[1] + columns[0] * local_normal[0])
            + columns[2] * local_normal[2];
        (normal, distance)
    } else {
        let depth = (magnitude - half_extents).to_array();
        let axis = if depth[0] <= depth[1] {
            if depth[1] > depth[2] { 1 } else { 2 }
        } else if depth[0] > depth[2] {
            0
        } else {
            2
        };
        let normal = if local.to_array()[axis] < 0.0 {
            -columns[axis]
        } else {
            columns[axis]
        };
        (normal, (depth[axis] - box_radius) - sphere_radius)
    };
    Some(ContactPoint {
        position: normal * (-distance - sphere_radius) + centre,
        normal,
        distance,
    })
}

/// Moves a world point into the space of `transform` in the order of
/// `hkVector4::setTransformedInversePos` (`0x14086B920`): `(r1·v.y + r0·v.x) + r2·v.z`, where
/// `ri` is row `i` of the inverse rotation.
fn inverse_transform_point(transform: &Transform, point: Vec3) -> Vec3 {
    let v = point - transform.translation;
    let rotation = transform.rotation;
    let component = |axis: Vec3| (axis.y * v.y + axis.x * v.x) + axis.z * v.z;
    Vec3::new(
        component(rotation.x_axis),
        component(rotation.y_axis),
        component(rotation.z_axis),
    )
}
