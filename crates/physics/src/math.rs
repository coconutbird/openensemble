//! Rigid transforms.

use std::ops::Mul;

use glam::{Mat3, Quat, Vec3};

/// A rigid transform: a rotation matrix and a translation (Havok `hkTransform`).
///
/// A point `p` maps to `rotation * p + translation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// The rotation. The columns are the local X, Y and Z axes in the parent space.
    pub rotation: Mat3,
    /// The translation in the parent space.
    pub translation: Vec3,
}

impl Transform {
    /// The transform that changes nothing.
    pub const IDENTITY: Self = Self {
        rotation: Mat3::IDENTITY,
        translation: Vec3::ZERO,
    };

    /// Makes a transform from a rotation quaternion and a translation.
    #[must_use]
    pub fn from_rotation_translation(rotation: Quat, translation: Vec3) -> Self {
        Self {
            rotation: Mat3::from_quat(rotation),
            translation,
        }
    }

    /// Maps a point from the local space to the parent space.
    #[must_use]
    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        self.rotation * point + self.translation
    }

    /// Rotates a direction from the local space to the parent space.
    #[must_use]
    pub fn transform_vector(&self, vector: Vec3) -> Vec3 {
        self.rotation * vector
    }

    /// Returns the inverse transform. The rotation must be orthonormal.
    #[must_use]
    pub fn inverse(&self) -> Self {
        let rotation = self.rotation.transpose();
        Self {
            rotation,
            translation: -(rotation * self.translation),
        }
    }
}

impl Mul for Transform {
    type Output = Self;

    /// Returns the transform that applies `rhs` first and then `self`.
    fn mul(self, rhs: Self) -> Self {
        Self {
            rotation: self.rotation * rhs.rotation,
            translation: self.rotation * rhs.translation + self.translation,
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::{Quat, Vec3};

    use super::Transform;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).abs().max_element() < 1e-5
    }

    #[test]
    fn composition_applies_the_right_transform_first() {
        let turn = Transform::from_rotation_translation(
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::ZERO,
        );
        let shift = Transform::from_rotation_translation(Quat::IDENTITY, Vec3::new(1.0, 0.0, 0.0));
        let point = (turn * shift).transform_point(Vec3::ZERO);
        assert!(close(point, Vec3::new(0.0, 0.0, -1.0)), "{point}");
    }

    #[test]
    fn inverse_undoes_the_transform() {
        let transform = Transform::from_rotation_translation(
            Quat::from_euler(glam::EulerRot::YXZ, 0.3, -0.7, 1.1),
            Vec3::new(4.0, -2.0, 9.0),
        );
        let point = Vec3::new(-3.0, 5.0, 0.5);
        let back = transform
            .inverse()
            .transform_point(transform.transform_point(point));
        assert!(close(back, point), "{back}");
    }
}
