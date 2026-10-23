//! The body creation rule of retail (`BObject_initFromProto` and `BEntity_createPhysicsObject`,
//! `0x140204EF0`, 2008 `entity.cpp:2129-2301`).
//!
//! - `BObject_initFromProto` (`0x14029C300`, 2008 `object.cpp:3656-3669`) checks no object
//!   class: every unit and class-0 object with a `PhysicsInfo` gets its body, unless the create
//!   parameters set `mNoPhysics`.
//! - A `MotionType fixed` info makes a fixed body with the building filter and no readback.
//! - Any other info makes a `BoxInertia` body with `max(mass, 1)`, the blueprint material and
//!   damping, a new Objects filter and zero velocities.
//! - A clamshell info makes the pelvis body and the upper and lower bodies, joined by two
//!   limited hinges.
//! - A unit with a vehicle type then gets its vehicle action, filter, inertia and hull. A
//!   class-0 object gets none (`BEntity_setupVehiclePhysics`, `0x1402055D0`, returns for any
//!   class other than Unit).

use std::sync::Arc;

use glam::{Mat3, Quat, Vec3};

use ::physics::{BodyDesc, BodyId, Layer, LimitedHingeDesc, MotionType};

use super::template::{PhysicsBlueprintBody, PhysicsTemplate};
use super::unit::{ClamshellBodies, UnitPhysics};
use super::vehicle_actions::{VehicleAction, VehicleFilter};
use crate::entities::BaseEntity;
use crate::entity_id::{EntityClass, EntityId};
use crate::world::World;

/// The hinge friction of the clamshell (`BClamshellPhysicsObject::setupHingeConstraints`).
const CLAMSHELL_HINGE_FRICTION: f32 = 3.0;
/// The upper hinge limits: `[-π/3, π/8]`.
const UPPER_HINGE_LIMITS: (f32, f32) = (-std::f32::consts::FRAC_PI_3, std::f32::consts::FRAC_PI_8);
/// The lower hinge limits: `[-π/8, π/4]`.
const LOWER_HINGE_LIMITS: (f32, f32) = (-std::f32::consts::FRAC_PI_8, std::f32::consts::FRAC_PI_4);

/// The rotation of an entity: its right, up and forward axes as the body X, Y and Z axes.
#[must_use]
pub(crate) fn entity_rotation(entity: &BaseEntity) -> Quat {
    let forward = entity.forward.normalize_or(Vec3::Z);
    let up = entity.up.normalize_or(Vec3::Y);
    let right = up.cross(forward).normalize_or(Vec3::X);
    let up = forward.cross(right).normalize_or(Vec3::Y);
    Quat::from_mat3(&Mat3::from_cols(right, up, forward))
}

impl World {
    /// Queues the body of a unit or class-0 object for the start of the next world update, or
    /// for [`World::create_pending_physics_objects`]. The entity then has its final spawn
    /// transform.
    pub(crate) fn queue_physics_object(
        &mut self,
        entity_id: EntityId,
        template: Arc<PhysicsTemplate>,
    ) {
        self.physics.queue_object(entity_id, template);
    }

    /// `BObject_initFromProto`: queues the `PhysicsInfo` body of a new class-0 object of a proto
    /// object. A proto object without a `PhysicsInfo` gives no body.
    pub(crate) fn queue_proto_object_physics(&mut self, object_id: EntityId) {
        let Some(template) = self
            .objects
            .get(object_id)
            .and_then(|object| self.prototype_physics(&object.proto_object_name))
            .and_then(|templates| templates.info.clone())
        else {
            return;
        };
        self.queue_physics_object(object_id, template);
    }

    /// Makes the queued bodies in queue order. An entity that has a body or is gone gets none.
    pub fn create_pending_physics_objects(&mut self) {
        for (entity_id, template) in self.physics.take_pending_objects() {
            let _created = self.create_physics_object(entity_id, &template, false);
        }
    }

    /// The physics state of a unit or class-0 object.
    #[must_use]
    pub fn physics_state(&self, entity_id: EntityId) -> Option<&UnitPhysics> {
        self.physics_owner(entity_id)?.1
    }

    /// The entity and the physics state of a unit or class-0 object.
    fn physics_owner(&self, entity_id: EntityId) -> Option<(&BaseEntity, Option<&UnitPhysics>)> {
        match entity_id.class()? {
            EntityClass::Unit => self
                .units
                .get(entity_id)
                .map(|unit| (&unit.base, unit.physics_object.as_ref())),
            EntityClass::Object => self
                .objects
                .get(entity_id)
                .map(|object| (&object.base, object.physics_object.as_ref())),
            _ => None,
        }
    }

    /// The physics slot of a unit or class-0 object.
    pub(crate) fn physics_slot_mut(
        &mut self,
        entity_id: EntityId,
    ) -> Option<&mut Option<UnitPhysics>> {
        match entity_id.class()? {
            EntityClass::Unit => self
                .units
                .get_mut(entity_id)
                .map(|unit| &mut unit.physics_object),
            EntityClass::Object => self
                .objects
                .get_mut(entity_id)
                .map(|object| &mut object.physics_object),
            _ => None,
        }
    }

    /// `BEntity_createPhysicsObject`: makes the body of a unit or class-0 object at its
    /// transform. Returns `false` if the entity is gone or has a body.
    pub(crate) fn create_physics_object(
        &mut self,
        entity_id: EntityId,
        template: &PhysicsTemplate,
        complete_on_inactive: bool,
    ) -> bool {
        let Some((base, None)) = self.physics_owner(entity_id) else {
            return false;
        };
        let rotation = entity_rotation(base);
        let position = base.position;
        let fixed = template.is_fixed();
        let center_offset = template.center_offset();
        let main = template.main_body();
        let filter = self.physics.create_collision_filter_info(Layer::Objects);
        let origin = position + rotation * center_offset;
        let motion = if fixed {
            MotionType::Fixed
        } else {
            MotionType::BoxInertia
        };
        let body = self.add_blueprint_body(main, motion, origin, rotation, filter);
        let mut state = UnitPhysics::new(body, center_offset, template.name().to_owned(), fixed);
        state.restitution = main.restitution();
        state.half_extents = main.half_extents();
        if let Some([upper, lower, _]) = template.clamshell_bodies() {
            state.clamshell = Some(self.add_clamshell(
                body,
                (upper, lower),
                template.info().upper_height_offset,
                template.info().lower_height_offset,
            ));
        }
        let world = self.physics.world_mut();
        if !fixed {
            // `BUnitActionPhysics` (action type 21) with `CompleteOnInactivePhysics`, then zero
            // velocities.
            state.flags.complete_on_inactive = complete_on_inactive;
            world.set_linear_velocity(body, Vec3::ZERO);
            world.set_angular_velocity(body, Vec3::ZERO);
        }
        if entity_id.class() == Some(EntityClass::Unit) {
            self.setup_vehicle_physics(&mut state, template);
        }
        if fixed {
            let building = self.physics.filters().building;
            self.physics.world_mut().set_filter_info(body, building);
        }
        if let Some(slot) = self.physics_slot_mut(entity_id) {
            *slot = Some(state);
        }
        true
    }

    /// `BObject::createPhysicsReplacement` (2008 `object.cpp:6735-6769`): replaces a unit with
    /// a new unit of the same proto object at the same transform. The new unit gets the body of
    /// the `PhysicsReplacementInfo` with `CompleteOnInactivePhysics` (`BObject_initFromProto`
    /// with `mPhysicsReplacement`). Returns the new unit, or `None` if the source is gone.
    pub(crate) fn spawn_physics_replacement(
        &mut self,
        source_id: EntityId,
        template: &PhysicsTemplate,
    ) -> Option<EntityId> {
        if !self.units.contains(source_id) {
            return None;
        }
        let replacement_id = self.units.allocate_id();
        let replacement = self
            .units
            .get(source_id)?
            .create_physics_replacement(replacement_id, template);
        let _removed = self.remove_unit(source_id)?;
        self.units.insert(replacement_id, replacement);
        let _created = self.create_physics_object(replacement_id, template, true);
        Some(replacement_id)
    }

    /// `BEntity_setupVehiclePhysics` (`0x1402055D0`): the action, the vehicle filter and the
    /// hull of a unit with a vehicle type.
    fn setup_vehicle_physics(&mut self, state: &mut UnitPhysics, template: &PhysicsTemplate) {
        let Some((action, setup)) = VehicleAction::setup(
            self.physics.world_mut(),
            state.body,
            template.vehicle_type(),
            template.vehicle_info(),
        ) else {
            return;
        };
        state.action = Some(action);
        state.action_order = self.physics.next_action_order();
        let filters = self.physics.filters();
        let filter = match setup.filter {
            VehicleFilter::Vehicles => filters.vehicle,
            VehicleFilter::NoTerrainVehicles => filters.no_terrain_vehicle,
        };
        let hull = setup
            .vehicle_hull
            .then(|| Arc::clone(self.physics.vehicle_hull()));
        let world = self.physics.world_mut();
        world.set_filter_info(state.body, filter);
        if let Some(hull) = hull {
            world.set_shape(state.body, hull);
        }
    }

    fn add_blueprint_body(
        &mut self,
        blueprint: &PhysicsBlueprintBody,
        motion: MotionType,
        position: Vec3,
        rotation: Quat,
        filter: ::physics::FilterInfo,
    ) -> BodyId {
        let mut desc = BodyDesc::new(Arc::clone(blueprint.shape()), motion, position);
        desc.rotation = rotation;
        desc.mass = blueprint.mass();
        desc.center_of_mass_offset = blueprint.center_of_mass_offset();
        desc.friction = blueprint.friction();
        desc.restitution = blueprint.restitution();
        desc.linear_damping = blueprint.linear_damping();
        desc.angular_damping = blueprint.angular_damping();
        desc.filter = filter;
        self.physics.world_mut().add_body(desc)
    }

    /// `BClamshellPhysicsObject_ctor` (`0x1407B7720`) and `setupHingeConstraints`
    /// (`0x1407B7AB0`): the upper and lower bodies above the pelvis origin, with the pelvis
    /// filter, and two limited hinges about the right axis at the pelvis origin.
    fn add_clamshell(
        &mut self,
        pelvis: BodyId,
        (upper, lower): (&PhysicsBlueprintBody, &PhysicsBlueprintBody),
        upper_height_offset: f32,
        lower_height_offset: f32,
    ) -> ClamshellBodies {
        let world = self.physics.world();
        let pelvis_position = world.position(pelvis).unwrap_or(Vec3::ZERO);
        let rotation = world.rotation(pelvis).unwrap_or(Quat::IDENTITY);
        let filter = world.filter_info(pelvis).unwrap_or_default();
        let upper_body = self.add_blueprint_body(
            upper,
            MotionType::BoxInertia,
            pelvis_position + Vec3::Y * upper_height_offset,
            rotation,
            filter,
        );
        let lower_body = self.add_blueprint_body(
            lower,
            MotionType::BoxInertia,
            pelvis_position + Vec3::Y * lower_height_offset,
            rotation,
            filter,
        );
        let axis = rotation * Vec3::X;
        let mut hinge = |body_b, (min_angle, max_angle)| {
            self.physics
                .world_mut()
                .add_limited_hinge(&LimitedHingeDesc {
                    body_a: pelvis,
                    body_b,
                    pivot: pelvis_position,
                    axis,
                    min_angle,
                    max_angle,
                    max_friction_torque: CLAMSHELL_HINGE_FRICTION,
                })
        };
        let upper_hinge = hinge(upper_body, UPPER_HINGE_LIMITS);
        let lower_hinge = hinge(lower_body, LOWER_HINGE_LIMITS);
        ClamshellBodies {
            upper: upper_body,
            lower: lower_body,
            hinges: [upper_hinge, lower_hinge],
        }
    }

    /// Removes the body of a unit or class-0 object with its clamshell parts and hinges
    /// (`BEntity::releasePhysicsObject`), or cancels its queued body.
    pub(crate) fn release_physics_object(&mut self, entity_id: EntityId) {
        self.physics.cancel_pending_object(entity_id);
        let Some(state) = self.physics_slot_mut(entity_id).and_then(Option::take) else {
            return;
        };
        let world = self.physics.world_mut();
        if let Some(clamshell) = &state.clamshell {
            for hinge in clamshell.hinges.into_iter().flatten() {
                world.remove_constraint(hinge);
            }
            world.remove_body(clamshell.upper);
            world.remove_body(clamshell.lower);
        }
        world.remove_body(state.body);
    }
}

#[cfg(test)]
mod tests;
