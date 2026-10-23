#![expect(
    dead_code,
    reason = "the plumbing commit adds the vehicle setup before the body creation uses it"
)]
//! The vehicle actions (`hkpUnaryAction` subclasses) that drive the vehicle bodies.
//!
//! `BEntity_setupVehiclePhysics` (`0x1402055D0`, 2008 `entity.cpp:2399-2507`) picks the action
//! by the vehicle type of the physics info. The action constructor sets the inertia. The setup
//! then gives the body the vehicle filter and, except for aircraft, the shared vehicle hull.
//! Each Havok substep applies the actions of the active bodies before the step.

use glam::Vec3;
use pipeline::database::hw1::physics::{PhysicsVehicleType, VehicleInfo};

use ::physics::{BodyId, MotionType, World as PhysicsWorld};

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use crate::world::World;

/// The action of a vehicle body.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VehicleAction {
    /// The vehicle type that selected the action.
    vehicle_type: PhysicsVehicleType,
    /// The body that the action drives.
    body: BodyId,
    /// `mpVehicleData->mWheelRadius`.
    wheel_radius: f32,
    /// `mDistBelowTerrain` (`action+0xD8`) while the action waits in the terrain queue
    /// (`action+0xDC` bit 0).
    distance_below_terrain: Option<f32>,
    /// `mFlagInAir`.
    in_air: bool,
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
    pub(crate) fn setup(
        physics: &mut PhysicsWorld,
        body: BodyId,
        vehicle_type: PhysicsVehicleType,
        info: Option<&VehicleInfo>,
    ) -> Option<(Self, VehicleSetup)> {
        let mass = physics.mass(body)?;
        let filter = match vehicle_type {
            PhysicsVehicleType::None => return None,
            PhysicsVehicleType::Warthog
            | PhysicsVehicleType::Chopper
            | PhysicsVehicleType::Ghost
            | PhysicsVehicleType::Hawk
            | PhysicsVehicleType::Hornet
            | PhysicsVehicleType::Vulture
            | PhysicsVehicleType::Banshee
            | PhysicsVehicleType::Vampire
            | PhysicsVehicleType::Sentinel => {
                // `diag(pitch 1·m, yaw 1·m, roll 0.4·m)` (2008 `physicswarthogaction.cpp:131-138`,
                // `physicsghostaction.cpp:32`, `physicshoverflightaction.cpp:29`).
                physics.set_inertia_local(body, Vec3::new(mass, mass, 0.4 * mass));
                VehicleFilter::Vehicles
            }
            PhysicsVehicleType::Scorpion
            | PhysicsVehicleType::Cobra
            | PhysicsVehicleType::Wolverine
            | PhysicsVehicleType::Grizzly
            | PhysicsVehicleType::Elephant
            | PhysicsVehicleType::Ground
            | PhysicsVehicleType::Gremlin
            | PhysicsVehicleType::Rhino
            | PhysicsVehicleType::Reactor => {
                // `BPhysicsGroundVehicleAction_setVehicleInfo` (`0x1402CEEF0`): the chassis
                // inertia of the vehicle info times the mass.
                let default_info = VehicleInfo::default();
                let info = info.unwrap_or(&default_info);
                physics.set_inertia_local(
                    body,
                    Vec3::new(
                        info.chassis_inertia_pitch * mass,
                        info.chassis_inertia_yaw * mass,
                        info.chassis_inertia_roll * mass,
                    ),
                );
                VehicleFilter::NoTerrainVehicles
            }
        };
        let setup = VehicleSetup {
            filter,
            vehicle_hull: !vehicle_type.is_aircraft(),
        };
        let wheel_radius = info.map_or(1.0, |info| info.wheel_radius);
        let action = Self {
            vehicle_type,
            body,
            wheel_radius,
            distance_below_terrain: None,
            in_air: false,
        };
        Some((action, setup))
    }

    /// `true` if a wheel went below the terrain and the body waits for the fix.
    pub(crate) const fn needs_terrain_resolution(&self) -> bool {
        self.distance_below_terrain.is_some()
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
        let Some(distance) = self.distance_below_terrain.take() else {
            return;
        };
        if physics.motion_type(self.body) != Some(MotionType::BoxInertia) {
            return;
        }
        let (Some(velocity), Some(position), Some(mass)) = (
            physics.linear_velocity(self.body),
            physics.position(self.body),
            physics.mass(self.body),
        ) else {
            return;
        };
        physics.set_linear_velocity(self.body, Vec3::new(velocity.x, 0.0, velocity.z));
        physics.set_position(
            self.body,
            position + Vec3::Y * (distance + self.wheel_radius),
        );
        physics.apply_linear_impulse(self.body, Vec3::Y * (-velocity.y * restitution * mass));
    }

    /// `mFlagInAir`: no wheel touched the terrain in the last substep.
    pub(crate) const fn in_air(&self) -> bool {
        self.in_air
    }

    /// Adds the action state to a checksum.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.vehicle_type as u8));
        checksum.hash_u32(self.body.raw());
        checksum.hash_f32(self.distance_below_terrain.unwrap_or(0.0));
        checksum.hash_u32(u32::from(self.in_air));
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
fn apply_vehicle_action(world: &mut World, unit: EntityId, dt: f32) {
    let _ = (world, unit, dt);
}
