//! The time of impact estimate (TIM) of the agent stream machine.
//!
//! `hkAgentNnMachine_processAgent` (`0x1408A3A80`) runs the TIM only for an entry of type 4.
//! `hkAgentNnMachine_createAgent` (`0x1408A35B0`) gives an entry type 4 only if its agent3 has a
//! separating normal function (`+80` of the agent3 functions), else type 2. For a type 4 entry
//! the machine decreases the separating distance of the last step by the motion of the bodies:
//!
//! `estimate = distance - ((p.z + angular) + (p.y + p.x))`, with `p = n * (ΔB - ΔA)` and
//! `angular = |ΔangleB| · radiusB + |ΔangleA| · radiusA`.
//!
//! Retail takes `ΔA` and `ΔB` from the centre of mass of each body and `|Δangle|` from the
//! motion state. The collision world has no motion data, so this port uses the origin of each
//! body and the angle of the rotation change, as described below.

use glam::{Mat3, Vec3};

use crate::collide::agent::CdBody;
use crate::math::Transform;
use crate::shape::Shape;

/// Returns the decrease of the separating distance along `normal` by the motion of both bodies
/// from `old_a` and `old_b` to their current transforms.
pub(crate) fn motion_decrease(
    normal: Vec3,
    a: &CdBody<'_>,
    old_a: &Transform,
    b: &CdBody<'_>,
    old_b: &Transform,
) -> f32 {
    let linear = (b.transform.translation - old_b.translation)
        - (a.transform.translation - old_a.translation);
    let angular = rotation_angle(&old_b.rotation, &b.transform.rotation) * object_radius(b.shape)
        + rotation_angle(&old_a.rotation, &a.transform.rotation) * object_radius(a.shape);
    let product = normal * linear;
    (product.z + angular) + (product.y + product.x)
}

/// Returns an angle measure of the rotation from `old` to `new` without trigonometry.
///
/// The Frobenius norm of `new - old` is `2√2 · sin(θ/2)`, so the result is the chord
/// `2 · sin(θ/2)`. It is a little smaller than the retail `|Δangle| = θ`.
fn rotation_angle(old: &Mat3, new: &Mat3) -> f32 {
    let change = *new - *old;
    let squares = [change.x_axis, change.y_axis, change.z_axis]
        .iter()
        .map(|column| column.length_squared())
        .fold(0.0, |sum, value| sum + value);
    (squares * 0.5).sqrt()
}

/// Returns the object radius of a shape as `hkpRigidBody::updateCachedShapeInfo`
/// (`0x140882130`) computes it: half the diagonal of the shape AABB at tolerance 0.
///
/// Retail adds the distance from the centre of mass to the AABB centre. The collision world
/// does not know the centre of mass, so this term is left out.
fn object_radius(shape: &Shape) -> f32 {
    let bounds = shape.aabb(&Transform::IDENTITY, 0.0);
    let extent = bounds.max - bounds.min;
    let squared = extent * extent;
    (squared.z + (squared.y + squared.x)).sqrt() * 0.5
}

#[cfg(test)]
mod tests {
    use glam::{Mat3, Quat, Vec3};

    use super::{motion_decrease, object_radius, rotation_angle};
    use crate::collide::agent::CdBody;
    use crate::math::Transform;
    use crate::shape::Shape;

    #[test]
    fn the_object_radius_is_half_the_aabb_diagonal() {
        let shape = Shape::Box {
            half_extents: Vec3::new(1.0, 2.0, 2.0),
            radius: 0.0,
        };
        assert!((object_radius(&shape) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn the_rotation_angle_is_the_chord_of_the_turn() {
        // A quarter turn about Z: the chord is 2 sin(45°) = √2.
        let turn = Mat3::from_quat(Quat::from_xyzw(
            0.0,
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ));
        let angle = rotation_angle(&Mat3::IDENTITY, &turn);
        assert!((angle - std::f32::consts::SQRT_2).abs() < 1e-5, "{angle}");
        assert!(rotation_angle(&turn, &turn).abs() < f32::EPSILON);
    }

    #[test]
    fn approaching_bodies_decrease_the_distance() {
        let shape = Shape::Sphere { radius: 0.5 };
        let a = CdBody {
            shape: &shape,
            transform: Transform::IDENTITY,
        };
        let moved = Transform {
            rotation: Mat3::IDENTITY,
            translation: Vec3::new(0.0, 1.5, 0.0),
        };
        let b = CdBody {
            shape: &shape,
            transform: moved,
        };
        let old_b = Transform {
            rotation: Mat3::IDENTITY,
            translation: Vec3::new(0.0, 2.0, 0.0),
        };
        // The normal points from B toward A: B moves 0.5 toward A.
        let decrease = motion_decrease(-Vec3::Y, &a, &Transform::IDENTITY, &b, &old_b);
        assert!((decrease - 0.5).abs() < 1e-6, "{decrease}");
    }
}
