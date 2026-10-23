//! The vehicle actions (`hkpUnaryAction` subclasses) that drive the vehicle bodies.
//!
//! `BEntity_setupVehiclePhysics` (`0x1402055D0`, 2008 `entity.cpp:2399-2507`) picks the action
//! by the vehicle type of the physics info. The action constructor sets the inertia. The setup
//! then gives the body the vehicle filter and, except for aircraft, the shared vehicle hull.
//! Each Havok substep applies the actions of the active bodies before the step.
//!
//! The actions read their unit and squad through [`VehicleActionInput`], which
//! [`apply_vehicle_actions`] builds from the sim `World` for each substep.

mod frame;
mod ghost;
mod ground;
mod hover;
mod input;
mod reorient;
mod suspension;
mod warthog;
mod world_input;

#[cfg(test)]
mod tests;

use glam::Vec3;
use pipeline::database::hw1::physics::{PhysicsVehicleType, VehicleInfo};

use ::physics::{BodyId, MotionType, World as PhysicsWorld};

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use crate::world::World;

pub(crate) use ghost::GhostAction;
pub(crate) use ground::{GroundAction, GroundVariant};
pub(crate) use hover::HoverAction;
pub(crate) use input::{EarlyTerminateFlags, VehicleActionInput};
pub(crate) use warthog::WarthogAction;

/// The action of a vehicle body.
#[derive(Clone, Debug)]
pub(crate) struct VehicleAction {
    /// The vehicle type that selected the action.
    vehicle_type: PhysicsVehicleType,
    /// The body that the action drives.
    body: BodyId,
    /// The action itself.
    kind: ActionKind,
}

/// The four retail action classes.
#[derive(Clone, Debug)]
pub(crate) enum ActionKind {
    /// `BPhysicsGroundVehicleAction` and its derived actions.
    Ground(GroundAction),
    /// `BPhysicsWarthogAction` (warthog and chopper).
    Warthog(WarthogAction),
    /// `BPhysicsGhostAction`.
    Ghost(GhostAction),
    /// `BPhysicsHoverFlightAction`.
    Hover(HoverAction),
}

/// The shared filter info that a vehicle gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VehicleFilter {
    /// `BPhysicsWorld+0x58` (layer Vehicles): warthog, chopper, ghost and aircraft.
    Vehicles,
    /// `BPhysicsWorld+0x5C` (layer `NoTerrainVehicles`): the ground family, which does its own
    /// terrain contact.
    NoTerrainVehicles,
}

/// What `BEntity_setupVehiclePhysics` does to a vehicle body after the action constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VehicleSetup {
    /// The shared filter info.
    pub(crate) filter: VehicleFilter,
    /// `true` if the body shape becomes the shared vehicle hull (every type but the aircraft).
    pub(crate) vehicle_hull: bool,
}

impl VehicleAction {
    /// Makes the action of a vehicle body as `BEntity_setupVehiclePhysics` does, and sets the
    /// inertia of the body like the action constructor. Returns `None` for
    /// [`PhysicsVehicleType::None`] or an unknown body.
    ///
    /// The warthog, ghost and hover constructors start `mDesiredPos` at the unit position.
    /// The body origin has the same `x` and `z` for an upright body, and the actions use only
    /// those, so the setup takes the body position.
    pub(crate) fn setup(
        physics: &mut PhysicsWorld,
        body: BodyId,
        vehicle_type: PhysicsVehicleType,
        info: Option<&VehicleInfo>,
    ) -> Option<(Self, VehicleSetup)> {
        let position = physics.position(body)?;
        let ground = |physics: &mut PhysicsWorld, variant| {
            let default_info = VehicleInfo::default();
            let info = info.unwrap_or(&default_info);
            ActionKind::Ground(GroundAction::new(physics, body, variant, info))
        };
        let kind = match vehicle_type {
            PhysicsVehicleType::None => return None,
            PhysicsVehicleType::Warthog => {
                ActionKind::Warthog(WarthogAction::new(physics, body, false, position))
            }
            PhysicsVehicleType::Chopper => {
                ActionKind::Warthog(WarthogAction::new(physics, body, true, position))
            }
            PhysicsVehicleType::Ghost => {
                ActionKind::Ghost(GhostAction::new(physics, body, position))
            }
            PhysicsVehicleType::Hawk
            | PhysicsVehicleType::Hornet
            | PhysicsVehicleType::Vulture
            | PhysicsVehicleType::Banshee
            | PhysicsVehicleType::Vampire
            | PhysicsVehicleType::Sentinel => {
                ActionKind::Hover(HoverAction::new(physics, body, position))
            }
            PhysicsVehicleType::Scorpion
            | PhysicsVehicleType::Grizzly
            | PhysicsVehicleType::Elephant
            | PhysicsVehicleType::Rhino
            | PhysicsVehicleType::Reactor => ground(physics, GroundVariant::Scorpion),
            PhysicsVehicleType::Cobra | PhysicsVehicleType::Gremlin => {
                ground(physics, GroundVariant::Cobra)
            }
            PhysicsVehicleType::Wolverine => ground(physics, GroundVariant::Wolverine),
            PhysicsVehicleType::Ground => ground(physics, GroundVariant::Base),
        };
        let filter = if matches!(kind, ActionKind::Ground(_)) {
            VehicleFilter::NoTerrainVehicles
        } else {
            VehicleFilter::Vehicles
        };
        let setup = VehicleSetup {
            filter,
            vehicle_hull: !vehicle_type.is_aircraft(),
        };
        let action = Self {
            vehicle_type,
            body,
            kind,
        };
        Some((action, setup))
    }

    #[cfg(test)]
    /// The vehicle type that selected the action.
    pub(crate) const fn vehicle_type(&self) -> PhysicsVehicleType {
        self.vehicle_type
    }

    /// The action class and its state.
    pub(crate) const fn kind(&self) -> &ActionKind {
        &self.kind
    }

    /// `true` for the hover flight action, which reads [`AirInput`].
    pub(crate) const fn is_hover(&self) -> bool {
        matches!(self.kind, ActionKind::Hover(_))
    }

    /// Applies the action for one Havok substep of `dt` seconds (`applyAction`). A keyframed
    /// body skips the action.
    pub(crate) fn apply(
        &mut self,
        physics: &mut PhysicsWorld,
        input: &VehicleActionInput,
        flags: &mut EarlyTerminateFlags,
        dt: f32,
    ) -> Option<f32> {
        if physics.motion_type(self.body) == Some(MotionType::Keyframed) {
            return None;
        }
        let body = self.body;
        match &mut self.kind {
            ActionKind::Ground(ground) => {
                ground.apply(physics, body, input.squad.as_ref(), dt);
                None
            }
            ActionKind::Warthog(warthog) => {
                warthog.apply(physics, body, input, flags, dt);
                None
            }
            ActionKind::Ghost(ghost) => {
                ghost.apply(physics, body, input, dt);
                None
            }
            ActionKind::Hover(hover) => hover.apply(physics, body, input, dt),
        }
    }

    /// `true` if a wheel went below the terrain and the body waits for the fix.
    pub(crate) fn needs_terrain_resolution(&self) -> bool {
        match &self.kind {
            ActionKind::Ground(ground) => ground.needs_terrain_resolution(),
            _ => false,
        }
    }

    /// `BPhysicsGroundVehicleAction::mainThreadTerrainCollisionResolution` (`0x1402D0480`, 2008
    /// `physicsgroundvehicleaction.cpp:558-591`): stop the vertical velocity, lift the body by
    /// the depth plus the wheel radius, then push it up by the old downward speed times the
    /// restitution of the body material.
    pub(crate) fn resolve_terrain_penetration(
        &mut self,
        physics: &mut PhysicsWorld,
        restitution: f32,
    ) {
        if let ActionKind::Ground(ground) = &mut self.kind {
            ground.resolve_terrain_penetration(physics, self.body, restitution);
        }
    }

    #[cfg(test)]
    /// `mFlagInAir`: no wheel touched the terrain in the last substep. Aircraft are never in
    /// the air in this sense.
    pub(crate) fn in_air(&self) -> bool {
        match &self.kind {
            ActionKind::Ground(ground) => ground.in_air(),
            ActionKind::Warthog(warthog) => warthog.in_air(),
            ActionKind::Ghost(ghost) => ghost.in_air(),
            ActionKind::Hover(_) => false,
        }
    }

    /// Adds the action state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.vehicle_type as u8));
        checksum.hash_u32(self.body.raw());
        match &self.kind {
            ActionKind::Ground(ground) => ground.hash_state(checksum),
            ActionKind::Warthog(warthog) => warthog.hash_state(checksum),
            ActionKind::Ghost(ghost) => ghost.hash_state(checksum),
            ActionKind::Hover(hover) => hover.hash_state(checksum),
        }
    }
}

/// Applies the action of each active vehicle body for one Havok substep of `dt` seconds, in
/// action creation order.
pub(crate) fn apply_vehicle_actions(world: &mut World, dt: f32) {
    for unit in world.active_vehicle_actions() {
        apply_vehicle_action(world, unit, dt);
    }
}

/// Applies the action of one unit for one substep.
fn apply_vehicle_action(world: &mut World, unit_id: EntityId, dt: f32) {
    let Some(mut action) = world
        .units
        .get_mut(unit_id)
        .and_then(|unit| unit.physics_object.as_mut())
        .and_then(|state| state.action.take())
    else {
        return;
    };
    let input = world_input::build(world, unit_id, &action);
    let flags = world
        .units
        .get(unit_id)
        .and_then(|unit| unit.physics_object.as_ref())
        .map(|state| state.flags);
    let mut early = EarlyTerminateFlags {
        allow: flags.is_some_and(|flags| flags.allow_early_terminate),
        terminate: flags.is_some_and(|flags| flags.early_terminate),
    };
    let squad_height = action.apply(world.physics.world_mut(), &input, &mut early, dt);
    let squad_id = world.units.get(unit_id).and_then(|unit| unit.squad_id);
    if let Some(state) = world
        .units
        .get_mut(unit_id)
        .and_then(|unit| unit.physics_object.as_mut())
    {
        state.flags.allow_early_terminate = early.allow;
        state.flags.early_terminate = early.terminate;
        state.action = Some(action);
    }
    if let (Some(height), Some(squad)) = (
        squad_height,
        squad_id.and_then(|id| world.squads.get_mut(id)),
    ) {
        squad.base.position.y = height;
    }
}

/// The unit right axis from its forward and up axes, as the entity matrix stores it.
pub(crate) fn unit_right(forward: Vec3, up: Vec3) -> Vec3 {
    up.cross(forward)
}
