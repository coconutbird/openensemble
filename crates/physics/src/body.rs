//! Bodies: identifiers, descriptions and the internal rigid-body state.

use std::sync::Arc;

use glam::{Mat3, Quat, Vec3};

use crate::dynamics::{self, MassProperties, encode_ufloat8};
use crate::filter::FilterInfo;
use crate::math::Transform;
use crate::shape::Shape;

/// Identifies a body in a physics world.
///
/// The world gives out the ids in creation order and never reuses an id. The broadphase, the
/// contact pairs and the solver use this order as the deterministic order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyId(u32);

impl BodyId {
    /// Makes an id from its raw value.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw value of the id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// The motion type of a body (the Havok 5.1 `hkpMotion` types that Halo Wars uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionType {
    /// `MOTION_FIXED` (7): the body never moves. Buildings and the terrain use it.
    Fixed,
    /// `MOTION_KEYFRAMED` (6): the body moves with its velocity, but contacts and impulses do
    /// not change it.
    Keyframed,
    /// `MOTION_BOX_INERTIA` (4): a dynamic body with a diagonal inertia tensor. Every dynamic
    /// Halo Wars body uses it.
    BoxInertia,
}

/// The description of a new body (`hkpRigidBodyCinfo` as `BPhysicsObject` fills it).
#[derive(Clone, Debug)]
pub struct BodyDesc {
    /// The collision shape in body space.
    pub shape: Arc<Shape>,
    /// The motion type.
    pub motion: MotionType,
    /// The position of the body origin in world space.
    pub position: Vec3,
    /// The rotation of the body. The world normalizes it.
    pub rotation: Quat,
    /// The mass. The world uses `max(mass, 1)` like `setupObjectRepresentation`.
    pub mass: f32,
    /// The offset that the world adds to the centre of mass of the shape.
    pub center_of_mass_offset: Vec3,
    /// The friction of the material.
    pub friction: f32,
    /// The restitution of the material.
    pub restitution: f32,
    /// The linear damping (1/s).
    pub linear_damping: f32,
    /// The angular damping (1/s).
    pub angular_damping: f32,
    /// The maximum linear speed. Havok stores it as an `hkUFloat8`, so 200 becomes 202.8.
    pub max_linear_velocity: f32,
    /// The maximum angular speed, stored as an `hkUFloat8` like the linear speed.
    pub max_angular_velocity: f32,
    /// The collision filter information.
    pub filter: FilterInfo,
    /// `true` for the spatial deactivator (the retail default), `false` for
    /// `DEACTIVATOR_NEVER`.
    pub deactivation_enabled: bool,
}

impl BodyDesc {
    /// Makes a description with the `hkpRigidBodyCinfo` defaults that retail keeps: maximum
    /// velocities 200 and the spatial deactivator. The material and the damping are the
    /// Havok defaults (friction 0.5, restitution 0.4, angular damping 0.05).
    #[must_use]
    pub fn new(shape: Arc<Shape>, motion: MotionType, position: Vec3) -> Self {
        Self {
            shape,
            motion,
            position,
            rotation: Quat::IDENTITY,
            mass: 1.0,
            center_of_mass_offset: Vec3::ZERO,
            friction: 0.5,
            restitution: 0.4,
            linear_damping: 0.0,
            angular_damping: 0.05,
            max_linear_velocity: 200.0,
            max_angular_velocity: 200.0,
            filter: FilterInfo::default(),
            deactivation_enabled: true,
        }
    }
}

/// The spatial deactivator state of a body (`hkpMotion` deactivation fields).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeactivationState {
    /// The frame counter (`m_deactivationIntegrateCounter`). 0xFF disables the checks.
    pub(crate) counter: u8,
    /// The number of passed checks for the high and the low frequency (7 bits, saturates at
    /// 64).
    pub(crate) inactive_frames: [u8; 2],
    /// The reference centre of mass of each frequency.
    pub(crate) reference_position: [Vec3; 2],
    /// The reference rotation of each frequency, packed to 4 × 8 bits.
    pub(crate) reference_rotation: [u32; 2],
}

impl DeactivationState {
    /// The state of `hkpRigidBody_setDeactivator` (`0x140881F70`): the spatial deactivator
    /// starts its counter at a phase of 0 to 15, `DEACTIVATOR_NEVER` disables it.
    pub(crate) fn new(enabled: bool, phase: u32) -> Self {
        Self {
            counter: if enabled {
                u8::try_from(phase & 0xf).unwrap_or(0)
            } else {
                0xff
            },
            inactive_frames: [0, 0],
            reference_position: [Vec3::ZERO; 2],
            reference_rotation: [0; 2],
        }
    }
}

/// The state of a body in the world.
#[derive(Clone, Debug)]
pub(crate) struct Body {
    pub(crate) shape: Arc<Shape>,
    pub(crate) motion: MotionType,
    /// The rotation (the motion state quaternion).
    pub(crate) rotation: Quat,
    /// The world centre of mass (`m_sweptTransform.m_centerOfMass1`).
    pub(crate) center_of_mass: Vec3,
    /// The centre of mass in body space.
    pub(crate) local_center_of_mass: Vec3,
    pub(crate) linear_velocity: Vec3,
    pub(crate) angular_velocity: Vec3,
    /// The mass properties of a dynamic body. Kept while the body is fixed or keyframed.
    pub(crate) mass: f32,
    /// The diagonal inertia in body space.
    pub(crate) inertia: Vec3,
    pub(crate) friction: f32,
    pub(crate) restitution: f32,
    pub(crate) linear_damping: f32,
    pub(crate) angular_damping: f32,
    pub(crate) max_linear_velocity: u8,
    pub(crate) max_angular_velocity: u8,
    pub(crate) filter: FilterInfo,
    pub(crate) active: bool,
    pub(crate) deactivation: DeactivationState,
}

impl Body {
    /// Builds the body state from a description, like `setupObjectRepresentation` and the
    /// `hkpRigidBody` constructor.
    pub(crate) fn new(id: BodyId, desc: &BodyDesc) -> Self {
        let mass = desc.mass.max(1.0);
        let properties = dynamics::shape_volume_mass_properties(&desc.shape, mass);
        let (inertia, local_center) = properties.map_or((Vec3::ONE, Vec3::ZERO), |p| {
            (box_inertia(&p), p.center_of_mass)
        });
        let rotation = normalize_or_identity(desc.rotation);
        let local_center_of_mass = local_center + desc.center_of_mass_offset;
        let enabled = desc.deactivation_enabled && desc.motion != MotionType::Fixed;
        Self {
            shape: Arc::clone(&desc.shape),
            motion: desc.motion,
            rotation,
            center_of_mass: Mat3::from_quat(rotation) * local_center_of_mass + desc.position,
            local_center_of_mass,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            mass,
            inertia,
            friction: desc.friction,
            restitution: desc.restitution,
            linear_damping: desc.linear_damping,
            angular_damping: desc.angular_damping,
            max_linear_velocity: encode_ufloat8(desc.max_linear_velocity),
            max_angular_velocity: encode_ufloat8(desc.max_angular_velocity),
            filter: desc.filter,
            active: desc.motion != MotionType::Fixed,
            deactivation: DeactivationState::new(enabled, id.raw()),
        }
    }

    /// The rotation matrix of the body.
    pub(crate) fn rotation_matrix(&self) -> Mat3 {
        Mat3::from_quat(self.rotation)
    }

    /// The position of the body origin: `centre of mass − R · local centre of mass`.
    pub(crate) fn position(&self) -> Vec3 {
        self.center_of_mass - self.rotation_matrix() * self.local_center_of_mass
    }

    /// The transform from body space to world space.
    pub(crate) fn transform(&self) -> Transform {
        let rotation = self.rotation_matrix();
        Transform {
            rotation,
            translation: self.center_of_mass - rotation * self.local_center_of_mass,
        }
    }

    /// The inverse mass, 0 for fixed and keyframed bodies.
    pub(crate) fn inverse_mass(&self) -> f32 {
        match self.motion {
            MotionType::BoxInertia => 1.0 / self.mass,
            MotionType::Fixed | MotionType::Keyframed => 0.0,
        }
    }

    /// The diagonal inverse inertia in body space, 0 for fixed and keyframed bodies
    /// (`hkpBoxMotion_setInertiaLocal`, `0x140895740`).
    pub(crate) fn inverse_inertia_local(&self) -> Vec3 {
        match self.motion {
            MotionType::BoxInertia => Vec3::ONE / self.inertia,
            MotionType::Fixed | MotionType::Keyframed => Vec3::ZERO,
        }
    }

    /// The inverse inertia tensor in world space: `R · diag(1/I) · Rᵀ`.
    pub(crate) fn inverse_inertia_world(&self) -> Mat3 {
        let rotation = self.rotation_matrix();
        rotation * Mat3::from_diagonal(self.inverse_inertia_local()) * rotation.transpose()
    }

    /// The velocity of the body at a world point.
    pub(crate) fn point_velocity(&self, point: Vec3) -> Vec3 {
        self.linear_velocity + self.angular_velocity.cross(point - self.center_of_mass)
    }

    /// `true` for a body that the solver can move.
    pub(crate) fn is_dynamic(&self) -> bool {
        self.motion == MotionType::BoxInertia
    }
}

/// The diagonal of the inertia tensor, as `hkpBoxMotion` keeps it.
fn box_inertia(properties: &MassProperties) -> Vec3 {
    Vec3::new(
        properties.inertia.x_axis.x,
        properties.inertia.y_axis.y,
        properties.inertia.z_axis.z,
    )
}

/// Normalizes a rotation, or returns the identity for an invalid one.
pub(crate) fn normalize_or_identity(rotation: Quat) -> Quat {
    let length = rotation.length();
    if length.is_finite() && length > 0.0 {
        rotation / length
    } else {
        Quat::IDENTITY
    }
}
