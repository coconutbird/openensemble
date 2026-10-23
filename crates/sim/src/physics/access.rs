//! The body access of the gameplay code: queries, impulses and overrides on the body of a unit.
//!
//! The impulse methods are the DE wrappers of `BPhysicsObject` (`applyPointImpulse`,
//! `0x1407B6DB0`; `applyImpulse`, `0x1407B6EA0`; `applyAngularImpulse`, `0x1407B6F40`): they skip
//! fixed and keyframed bodies and wake the body.
//!
//! `BClamshellPhysicsObject` (vtable `0x1411E4120`) overrides `setMass`, `setAngularDamping`,
//! `setLinearVelocity`, `setAngularVelocity`, `applyImpulse`, `applyAngularImpulse`,
//! `setKeyframed`, `setCollisionFilterInfo` and `setLinearDamping`: each one acts on the pelvis,
//! then on the upper and lower bodies. `applyPointImpulse` and the forces have no override and
//! act on the pelvis only.

use glam::{Quat, Vec3};

use ::physics::BodyId;

use super::bodies::entity_rotation;
use super::death::body_and_parts;
use crate::entity_id::EntityId;
use crate::world::World;

impl World {
    /// The body of a unit.
    #[must_use]
    pub fn unit_body(&self, unit: EntityId) -> Option<BodyId> {
        self.units
            .get(unit)?
            .physics_object
            .as_ref()
            .map(super::UnitPhysics::body)
    }

    /// The body of a unit if it can move (not fixed).
    fn dynamic_unit_body(&self, unit: EntityId) -> Option<BodyId> {
        let state = self.units.get(unit)?.physics_object.as_ref()?;
        (!state.fixed).then_some(state.body)
    }

    /// The bodies of a unit for a clamshell override: the main body, then the upper and lower
    /// bodies of a clamshell. `dynamic` skips a fixed body.
    fn unit_bodies(
        &self,
        unit: EntityId,
        dynamic: bool,
    ) -> Option<impl Iterator<Item = BodyId> + use<>> {
        let state = self.units.get(unit)?.physics_object.as_ref()?;
        if dynamic && state.fixed {
            return None;
        }
        Some(body_and_parts(state.body, state.clamshell_bodies()))
    }

    /// The mass of the body of a unit: `max(blueprint mass, 1)`.
    #[must_use]
    pub fn unit_body_mass(&self, unit: EntityId) -> Option<f32> {
        self.physics.world().mass(self.unit_body(unit)?)
    }

    /// Half the size of the box around the blueprint shape of the body of a unit.
    #[must_use]
    pub fn unit_body_half_extents(&self, unit: EntityId) -> Option<Vec3> {
        self.units
            .get(unit)?
            .physics_object
            .as_ref()
            .map(super::UnitPhysics::half_extents)
    }

    /// The origin of the body of a unit: the unit position plus the rotated `CenterOffset`.
    #[must_use]
    pub fn unit_body_position(&self, unit: EntityId) -> Option<Vec3> {
        self.physics.world().position(self.unit_body(unit)?)
    }

    /// The rotation of the body of a unit.
    #[must_use]
    pub fn unit_body_rotation(&self, unit: EntityId) -> Option<Quat> {
        self.physics.world().rotation(self.unit_body(unit)?)
    }

    /// The centre of mass of the body of a unit in world space.
    #[must_use]
    pub fn unit_body_center_of_mass(&self, unit: EntityId) -> Option<Vec3> {
        self.physics
            .world()
            .center_of_mass_world(self.unit_body(unit)?)
    }

    /// The linear velocity of the body of a unit.
    #[must_use]
    pub fn unit_body_linear_velocity(&self, unit: EntityId) -> Option<Vec3> {
        self.physics.world().linear_velocity(self.unit_body(unit)?)
    }

    /// The angular velocity of the body of a unit (world space, rad/s).
    #[must_use]
    pub fn unit_body_angular_velocity(&self, unit: EntityId) -> Option<Vec3> {
        self.physics.world().angular_velocity(self.unit_body(unit)?)
    }

    /// `true` if the body of a unit is awake.
    #[must_use]
    pub fn unit_body_is_active(&self, unit: EntityId) -> Option<bool> {
        self.physics.world().is_active(self.unit_body(unit)?)
    }

    /// `true` if the body of a unit has a contact point with the terrain body. `None` for a
    /// unit without a body.
    #[must_use]
    pub fn unit_body_touches_terrain(&self, unit: EntityId) -> Option<bool> {
        let body = self.unit_body(unit)?;
        let terrain = self.physics.terrain_body();
        Some(self.physics.world().manifolds_of(body).any(|manifold| {
            let other = if manifold.body_a == body {
                manifold.body_b
            } else {
                manifold.body_a
            };
            Some(other) == terrain && !manifold.points.is_empty()
        }))
    }

    /// `applyImpulse`: a linear impulse on the body of a unit, and on each part of a clamshell
    /// (`BClamshellPhysicsObject_applyImpulse`, `0x1407B8300`). Returns `false` if the unit has
    /// no dynamic body.
    pub fn apply_unit_impulse(&mut self, unit: EntityId, impulse: Vec3) -> bool {
        let Some(bodies) = self.unit_bodies(unit, true) else {
            return false;
        };
        let world = self.physics.world_mut();
        for body in bodies {
            world.apply_linear_impulse(body, impulse);
        }
        true
    }

    /// `applyPointImpulse`: an impulse at a world point on the body of a unit. Returns `false`
    /// if the unit has no dynamic body.
    pub fn apply_unit_point_impulse(&mut self, unit: EntityId, impulse: Vec3, point: Vec3) -> bool {
        let Some(body) = self.dynamic_unit_body(unit) else {
            return false;
        };
        self.physics
            .world_mut()
            .apply_point_impulse(body, impulse, point);
        true
    }

    /// `applyAngularImpulse`: an angular impulse (world space) on the body of a unit, and on
    /// each part of a clamshell (`BClamshellPhysicsObject_applyAngularImpulse`,
    /// `0x1407B8340`). Returns `false` if the unit has no dynamic body.
    pub fn apply_unit_angular_impulse(&mut self, unit: EntityId, impulse: Vec3) -> bool {
        let Some(bodies) = self.unit_bodies(unit, true) else {
            return false;
        };
        let world = self.physics.world_mut();
        for body in bodies {
            world.apply_angular_impulse(body, impulse);
        }
        true
    }

    /// `hkpRigidBody::applyForce(dt, force)`: a linear impulse of `force · dt` on the body of a
    /// unit. Returns `false` if the unit has no dynamic body.
    pub fn apply_unit_force(&mut self, unit: EntityId, dt: f32, force: Vec3) -> bool {
        let Some(body) = self.dynamic_unit_body(unit) else {
            return false;
        };
        self.physics.world_mut().apply_force(body, dt, force);
        true
    }

    /// `hkpRigidBody::applyForce(dt, force, point)`: an impulse of `force · dt` at a world
    /// point. Returns `false` if the unit has no dynamic body.
    pub fn apply_unit_force_at_point(
        &mut self,
        unit: EntityId,
        dt: f32,
        force: Vec3,
        point: Vec3,
    ) -> bool {
        let Some(body) = self.dynamic_unit_body(unit) else {
            return false;
        };
        self.physics
            .world_mut()
            .apply_force_at_point(body, dt, force, point);
        true
    }

    /// `setLinearVelocity` on the body of a unit, and on each part of a clamshell
    /// (`BClamshellPhysicsObject_setLinearVelocity`, `0x1407B8280`). Returns `false` if the
    /// unit has no dynamic body or the velocity is not finite.
    pub fn set_unit_body_linear_velocity(&mut self, unit: EntityId, velocity: Vec3) -> bool {
        if !velocity.is_finite() {
            return false;
        }
        let Some(bodies) = self.unit_bodies(unit, true) else {
            return false;
        };
        let world = self.physics.world_mut();
        for body in bodies {
            world.set_linear_velocity(body, velocity);
        }
        true
    }

    /// `setAngularVelocity` on the body of a unit, and on each part of a clamshell
    /// (`BClamshellPhysicsObject_setAngularVelocity`, `0x1407B82C0`). Returns `false` if the
    /// unit has no dynamic body or the velocity is not finite.
    pub fn set_unit_body_angular_velocity(&mut self, unit: EntityId, velocity: Vec3) -> bool {
        if !velocity.is_finite() {
            return false;
        }
        let Some(bodies) = self.unit_bodies(unit, true) else {
            return false;
        };
        let world = self.physics.world_mut();
        for body in bodies {
            world.set_angular_velocity(body, velocity);
        }
        true
    }

    /// The linear and angular damping of the body of a unit.
    #[must_use]
    pub fn unit_body_damping(&self, unit: EntityId) -> Option<(f32, f32)> {
        self.physics.world().damping(self.unit_body(unit)?)
    }

    /// `setLinearDamping` on the body of a unit, and on each part of a clamshell
    /// (`BClamshellPhysicsObject_setLinearDamping`, `0x1407B8BD0`). Returns `false` if the unit
    /// has no body.
    pub(crate) fn set_unit_body_linear_damping(&mut self, unit: EntityId, damping: f32) -> bool {
        let Some(bodies) = self.unit_bodies(unit, false) else {
            return false;
        };
        let world = self.physics.world_mut();
        for body in bodies {
            let (_, angular) = world.damping(body).unwrap_or_default();
            world.set_damping(body, damping, angular);
        }
        true
    }

    /// `setAngularDamping` on the body of a unit, and on each part of a clamshell
    /// (`BClamshellPhysicsObject_setAngularDamping`, `0x1407B8B80`). Returns `false` if the
    /// unit has no body.
    pub(crate) fn set_unit_body_angular_damping(&mut self, unit: EntityId, damping: f32) -> bool {
        let Some(bodies) = self.unit_bodies(unit, false) else {
            return false;
        };
        let world = self.physics.world_mut();
        for body in bodies {
            let (linear, _) = world.damping(body).unwrap_or_default();
            world.set_damping(body, linear, damping);
        }
        true
    }

    /// `BEntity::setPosition(pos, overridePhysics = true)` and `setRotation`: moves the body of a
    /// unit to the unit transform. The body origin is the unit position plus the rotated
    /// `CenterOffset`. Returns `false` if the unit has no body.
    pub(crate) fn move_unit_body_to_unit(&mut self, unit: EntityId) -> bool {
        let Some(entity) = self.units.get(unit) else {
            return false;
        };
        let Some(state) = entity.physics_object.as_ref() else {
            return false;
        };
        let rotation = entity_rotation(&entity.base);
        let origin = entity.base.position + rotation * state.center_offset;
        let body = state.body;
        let world = self.physics.world_mut();
        world.set_rotation(body, rotation);
        world.set_position(body, origin);
        true
    }
}
