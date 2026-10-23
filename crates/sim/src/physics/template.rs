//! The body data of a physics info: what `BEntity_createPhysicsObject` (`0x140204EF0`) reads to
//! build a body.

use std::sync::Arc;

use glam::Vec3;
use pipeline::database::hw1::Vector3;
use pipeline::database::hw1::physics::{
    MotionType as InfoMotionType, PhysicsBlueprint, PhysicsInfo, PhysicsVehicleType, VehicleInfo,
};
use pipeline::hw1::physics::{ResolvedBlueprint, ResolvedPhysicsInfo};

use ::physics::Shape;

use super::shape::{blueprint_box, physics_shape, shape_half_extents};
use crate::sync::SyncChecksum;

/// One rigid body of a physics info: a blueprint and its shape
/// (`BPhysicsObject_ctorFromBlueprint`, `0x1407B5C70`).
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsBlueprintBody {
    name: String,
    shape: Arc<Shape>,
    mass: f32,
    friction: f32,
    restitution: f32,
    linear_damping: f32,
    angular_damping: f32,
    center_of_mass_offset: Vec3,
    half_extents: Vec3,
}

impl PhysicsBlueprintBody {
    /// Makes the body of a resolved blueprint. A blueprint without a shape gets the box of its
    /// half extents.
    #[must_use]
    pub fn from_resolved(resolved: &ResolvedBlueprint<'_>) -> Self {
        let blueprint = resolved.blueprint;
        let shape = resolved.shape.map_or_else(
            || blueprint_box(vector(blueprint.half_extents)),
            physics_shape,
        );
        Self::new(resolved.name, blueprint, shape)
    }

    fn new(name: &str, blueprint: &PhysicsBlueprint, shape: Shape) -> Self {
        Self {
            name: name.to_owned(),
            half_extents: shape_half_extents(&shape),
            shape: Arc::new(shape),
            mass: blueprint.mass,
            friction: blueprint.friction,
            restitution: blueprint.restitution,
            linear_damping: blueprint.linear_damping,
            angular_damping: blueprint.angular_damping,
            center_of_mass_offset: vector(blueprint.center_of_mass_offset),
        }
    }

    /// Makes a body with a box shape, for tests and for code-built bodies.
    #[must_use]
    pub fn with_box(name: &str, blueprint: &PhysicsBlueprint, half_extents: Vec3) -> Self {
        Self::new(name, blueprint, blueprint_box(half_extents))
    }

    /// The blueprint name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The collision shape.
    #[must_use]
    pub fn shape(&self) -> &Arc<Shape> {
        &self.shape
    }

    /// The blueprint mass. The world uses `max(mass, 1)`.
    #[must_use]
    pub const fn mass(&self) -> f32 {
        self.mass
    }

    /// The material friction.
    #[must_use]
    pub const fn friction(&self) -> f32 {
        self.friction
    }

    /// The material restitution.
    #[must_use]
    pub const fn restitution(&self) -> f32 {
        self.restitution
    }

    /// The linear damping.
    #[must_use]
    pub const fn linear_damping(&self) -> f32 {
        self.linear_damping
    }

    /// The angular damping.
    #[must_use]
    pub const fn angular_damping(&self) -> f32 {
        self.angular_damping
    }

    /// The offset that the blueprint adds to the centre of mass of the shape.
    #[must_use]
    pub const fn center_of_mass_offset(&self) -> Vec3 {
        self.center_of_mass_offset
    }

    /// Half the size of the box around the shape.
    #[must_use]
    pub const fn half_extents(&self) -> Vec3 {
        self.half_extents
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_f32(self.mass);
        checksum.hash_f32(self.friction);
        checksum.hash_f32(self.restitution);
        checksum.hash_f32(self.linear_damping);
        checksum.hash_f32(self.angular_damping);
        hash_vec3(checksum, self.center_of_mass_offset);
        hash_vec3(checksum, self.half_extents);
    }
}

/// A physics info with its bodies: the input of `BEntity_createPhysicsObject`.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsTemplate {
    name: String,
    info: PhysicsInfo,
    /// The bodies in info order. A clamshell has upper, lower and pelvis first.
    bodies: Vec<PhysicsBlueprintBody>,
}

impl PhysicsTemplate {
    /// Makes the template of a resolved physics info. Returns `None` if the info names no
    /// blueprint, or if a clamshell does not have its three blueprints: retail then makes no
    /// body.
    #[must_use]
    pub fn from_resolved(resolved: &ResolvedPhysicsInfo<'_>) -> Option<Self> {
        let bodies = resolved
            .bodies
            .iter()
            .map(PhysicsBlueprintBody::from_resolved)
            .collect();
        Self::new(resolved.name, resolved.info.clone(), bodies)
    }

    /// Makes a template from an info and its bodies. Returns `None` if the info cannot make a
    /// body.
    #[must_use]
    pub fn new(name: &str, info: PhysicsInfo, bodies: Vec<PhysicsBlueprintBody>) -> Option<Self> {
        let needed = if info.is_clamshell() { 3 } else { 1 };
        (bodies.len() >= needed).then(|| Self {
            name: name.to_owned(),
            info,
            bodies,
        })
    }

    /// Makes the template of a simple dynamic info with one body, for tests and code-built
    /// bodies. The info takes the blueprint name and `center_offset`.
    #[must_use]
    pub fn simple(name: &str, body: PhysicsBlueprintBody, center_offset: Vec3) -> Self {
        let info = PhysicsInfo {
            blueprints: vec![body.name.clone()],
            center_offset: Vector3 {
                x: center_offset.x,
                y: center_offset.y,
                z: center_offset.z,
            },
            ..PhysicsInfo::default()
        };
        Self {
            name: name.to_owned(),
            info,
            bodies: vec![body],
        }
    }

    /// The physics info name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The physics info.
    #[must_use]
    pub const fn info(&self) -> &PhysicsInfo {
        &self.info
    }

    /// `true` for `MotionType fixed`.
    #[must_use]
    pub fn is_fixed(&self) -> bool {
        self.info.motion_type == InfoMotionType::Fixed
    }

    /// `true` for a clamshell (pelvis, upper and lower bodies).
    #[must_use]
    pub const fn is_clamshell(&self) -> bool {
        self.info.is_clamshell()
    }

    /// The `CenterOffset`: the body origin relative to the entity position.
    #[must_use]
    pub const fn center_offset(&self) -> Vec3 {
        vector(self.info.center_offset)
    }

    /// The vehicle type.
    #[must_use]
    pub const fn vehicle_type(&self) -> PhysicsVehicleType {
        self.info.vehicle_type
    }

    /// The vehicle tuning.
    #[must_use]
    pub const fn vehicle_info(&self) -> Option<&VehicleInfo> {
        self.info.vehicle_info.as_ref()
    }

    /// The bodies in info order.
    #[must_use]
    pub fn bodies(&self) -> &[PhysicsBlueprintBody] {
        &self.bodies
    }

    /// The main body: the first blueprint, or the pelvis of a clamshell.
    #[must_use]
    pub fn main_body(&self) -> &PhysicsBlueprintBody {
        if self.is_clamshell() {
            &self.bodies[2]
        } else {
            &self.bodies[0]
        }
    }

    /// The clamshell parts: upper, lower and pelvis.
    #[must_use]
    pub fn clamshell_bodies(&self) -> Option<[&PhysicsBlueprintBody; 3]> {
        self.is_clamshell()
            .then(|| [&self.bodies[0], &self.bodies[1], &self.bodies[2]])
    }

    /// The half size of the main body.
    #[must_use]
    pub fn half_extents(&self) -> Vec3 {
        self.main_body().half_extents()
    }

    /// Adds the template to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_bytes(self.name.as_bytes());
        checksum.hash_u32(u32::from(self.is_fixed()));
        checksum.hash_u32(u32::from(self.info.flags.bits()));
        hash_vec3(checksum, self.center_offset());
        checksum.hash_u32(u32::from(self.vehicle_type() as u8));
        checksum.hash_u32(u32::try_from(self.bodies.len()).unwrap_or(u32::MAX));
        for body in &self.bodies {
            body.hash_state(checksum);
        }
    }
}

/// The two physics infos of a proto object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectPhysicsTemplates {
    /// `<PhysicsInfo>`: the body of the live object.
    pub info: Option<Arc<PhysicsTemplate>>,
    /// `<PhysicsReplacementInfo>`: the body of a physics replacement.
    pub replacement: Option<Arc<PhysicsTemplate>>,
}

const fn vector(value: Vector3) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}
