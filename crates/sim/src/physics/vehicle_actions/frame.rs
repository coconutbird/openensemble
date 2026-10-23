//! Vector helpers that the vehicle actions share: the Havok and `BVector` rules that the
//! retail actions call.

use glam::{Mat3, Quat, Vec3};
use physics::{BodyId, World as PhysicsWorld};

use crate::math;

/// `cFloatCompareEpsilon`.
pub(super) const FLOAT_COMPARE_EPSILON: f32 = 0.000_001;

/// The state of a body that an action reads at one point of its update.
#[derive(Clone, Copy, Debug)]
pub(super) struct BodyFrame {
    /// The body origin (`getPosition`).
    pub(super) position: Vec3,
    /// The rotation (`getRotation`).
    pub(super) rotation: Quat,
    /// The rotation as a matrix. Column 0 is right, column 1 is up, column 2 is forward.
    pub(super) matrix: Mat3,
    /// The linear velocity of the centre of mass.
    pub(super) linear_velocity: Vec3,
    /// The angular velocity in world space.
    pub(super) angular_velocity: Vec3,
    /// The mass.
    pub(super) mass: f32,
}

impl BodyFrame {
    /// Reads the current state of a body. Returns `None` for an unknown body.
    pub(super) fn read(physics: &PhysicsWorld, body: BodyId) -> Option<Self> {
        let rotation = physics.rotation(body)?;
        Some(Self {
            position: physics.position(body)?,
            rotation,
            matrix: Mat3::from_quat(rotation),
            linear_velocity: physics.linear_velocity(body)?,
            angular_velocity: physics.angular_velocity(body)?,
            mass: physics.mass(body)?,
        })
    }

    /// The right axis of the body (rotation column 0).
    pub(super) fn right(&self) -> Vec3 {
        self.matrix.x_axis
    }

    /// The up axis of the body (rotation column 1).
    pub(super) fn up(&self) -> Vec3 {
        self.matrix.y_axis
    }

    /// The forward axis of the body (rotation column 2).
    pub(super) fn forward(&self) -> Vec3 {
        self.matrix.z_axis
    }

    /// `hkTransform::setTransformedPos`: a body-space point in world space.
    pub(super) fn transform_point(&self, point: Vec3) -> Vec3 {
        self.matrix * point + self.position
    }
}

/// `hkMath::acos`: 0 or π for a value at or outside ±1.
pub(super) fn hk_acos(value: f32) -> f32 {
    if value.abs() >= 1.0 {
        if value > 0.0 {
            0.0
        } else {
            std::f32::consts::PI
        }
    } else {
        math::acos(value)
    }
}

/// `hkVector4::getMajorAxis`: the index of the component with the largest magnitude.
pub(super) fn major_axis_component(vector: Vec3) -> f32 {
    let abs = vector.abs();
    let index = if abs.x < abs.y {
        if abs.y < abs.z { 2 } else { 1 }
    } else if abs.x < abs.z {
        2
    } else {
        0
    };
    vector[index]
}

/// `BVector::safeNormalize`: the unit vector, or the vector unchanged if its length is not
/// above `cFloatCompareEpsilon`.
pub(super) fn safe_normalize(vector: Vec3) -> Option<Vec3> {
    let length_squared = vector.length_squared();
    (length_squared > FLOAT_COMPARE_EPSILON * FLOAT_COMPARE_EPSILON)
        .then(|| vector * (1.0 / length_squared.sqrt()))
}

/// `BVector::safeNormalize` that keeps the vector if it cannot normalize it.
pub(super) fn safe_normalize_or_keep(vector: Vec3) -> Vec3 {
    safe_normalize(vector).unwrap_or(vector)
}

/// `BVector::normalize`: divides by the length. A zero vector gives NaN, as in retail.
pub(super) fn normalize(vector: Vec3) -> Vec3 {
    vector / vector.length()
}

/// The vector with `y = 0`.
pub(super) fn flat(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, 0.0, vector.z)
}

/// `BVector::xzDistance`.
pub(super) fn xz_distance(a: Vec3, b: Vec3) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    (dx * dx + dz * dz).sqrt()
}

/// `BVector::angleBetweenVector`: the angle between two vectors, with the cosine clamped to
/// ±1.
pub(super) fn angle_between(a: Vec3, b: Vec3) -> f32 {
    let cosine = a.dot(b) / (a.length() * b.length());
    math::acos(cosine.clamp(-1.0, 1.0))
}

/// The signed yaw from `current` to `desired` that the warthog, ghost and hover actions use:
/// the angle between them, negative if `desired × current` points up.
pub(super) fn signed_yaw(desired: Vec3, current: Vec3) -> f32 {
    let angle = angle_between(desired, current);
    if desired.cross(current).y > 0.0 {
        -angle
    } else {
        angle
    }
}

/// `hkpRigidBody::getInertiaWorld`: `R · diag(I) · Rᵀ`.
pub(super) fn world_inertia(matrix: Mat3, inertia: Vec3) -> Mat3 {
    matrix * Mat3::from_diagonal(inertia) * matrix.transpose()
}
