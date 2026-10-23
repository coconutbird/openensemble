//! Tests of the vehicle actions against a physics world with a heightfield.

mod ground;
mod hover;
mod setup;
mod warthog;

use std::sync::Arc;

use glam::{Mat3, Quat, Vec3};
use physics::{BodyDesc, BodyId, FilterInfo, HeightField, Layer, MotionType, Shape, World};
use pipeline::database::hw1::physics::{PhysicsVehicleType, VehicleInfo};

use super::input::{MoveActionInput, SquadInput, UnitInput, VehicleActionInput};
use super::{EarlyTerminateFlags, VehicleAction, VehicleFilter, VehicleSetup};
use crate::physics::SimPhysics;
use crate::sync::SyncChecksum;

/// One sim tick.
const TICK: f32 = 0.05;
/// The `CenterOffset` of the ground vehicles.
const GROUND_CENTER_OFFSET: Vec3 = Vec3::new(0.0, 2.28, 0.0);

/// A physics world with a terrain and one vehicle.
struct Scene {
    physics: SimPhysics,
    body: BodyId,
    action: VehicleAction,
    setup: VehicleSetup,
    center_offset: Vec3,
    flags: EarlyTerminateFlags,
}

impl Scene {
    /// Makes a terrain from `height(x, z)` on a 64 × 64 grid with tile size 2, and a vehicle
    /// body of the type at `unit_position`, turned by `yaw` about world up.
    fn new(
        vehicle_type: PhysicsVehicleType,
        info: Option<&VehicleInfo>,
        unit_position: Vec3,
        yaw: f32,
        height: impl Fn(f32, f32) -> f32,
    ) -> Self {
        let mut physics = SimPhysics::new();
        add_terrain(physics.world_mut(), height);
        let center_offset = if vehicle_type.is_aircraft() {
            Vec3::new(0.0, 0.28, 0.0)
        } else {
            GROUND_CENTER_OFFSET
        };
        let rotation = crate::math::quat_from_rotation_y(yaw);
        let mut desc = BodyDesc::new(
            Arc::new(Shape::Box {
                half_extents: Vec3::new(1.5, 1.0, 3.0),
                radius: physics::DEFAULT_CONVEX_RADIUS,
            }),
            MotionType::BoxInertia,
            unit_position + rotation * center_offset,
        );
        desc.rotation = rotation;
        desc.mass = 150.0;
        desc.friction = 0.5;
        desc.restitution = 0.1;
        desc.deactivation_enabled = false;
        let body = physics.world_mut().add_body(desc);
        let (action, setup) =
            VehicleAction::setup(physics.world_mut(), body, vehicle_type, info).expect("vehicle");
        let filter = match setup.filter {
            VehicleFilter::Vehicles => physics.filters().vehicle,
            VehicleFilter::NoTerrainVehicles => physics.filters().no_terrain_vehicle,
        };
        physics.world_mut().set_filter_info(body, filter);
        if setup.vehicle_hull {
            let hull = Arc::clone(physics.vehicle_hull());
            physics.world_mut().set_shape(body, hull);
        }
        Self {
            physics,
            body,
            action,
            setup,
            center_offset,
            flags: EarlyTerminateFlags::default(),
        }
    }

    fn world(&self) -> &World {
        self.physics.world()
    }

    fn position(&self) -> Vec3 {
        self.world().position(self.body).expect("body")
    }

    fn rotation(&self) -> Mat3 {
        Mat3::from_quat(self.world().rotation(self.body).expect("body"))
    }

    fn velocity(&self) -> Vec3 {
        self.world().linear_velocity(self.body).expect("body")
    }

    /// The unit as `BUnitActionPhysics::update` reads it back: the body position minus the
    /// rotated centre offset, and the body axes.
    fn unit(&self) -> UnitInput {
        let rotation = self.rotation();
        UnitInput {
            position: self.position() - rotation * self.center_offset,
            forward: rotation.z_axis,
            right: rotation.x_axis,
            obstruction_radius: 3.0,
            proto_desired_velocity: 20.0,
            max_velocity: 30.0,
            reverse_speed: -1.0,
            turn_rate_degrees: 120.0,
            has_physics_action: true,
            doing_fatality: false,
            ranged_target: None,
        }
    }

    /// Runs one sim tick: the readback input, then each substep applies the action and steps
    /// the world.
    fn tick(&mut self, mut input: impl FnMut(UnitInput) -> VehicleActionInput) -> Option<f32> {
        let input = input(self.unit());
        let (count, dt) = World::substeps(TICK);
        let mut squad_height = None;
        for _ in 0..count {
            squad_height = self
                .action
                .apply(self.physics.world_mut(), &input, &mut self.flags, dt);
            self.physics.world_mut().step_delta_time(dt, &mut ());
        }
        if self.action.needs_terrain_resolution() {
            self.action
                .resolve_terrain_penetration(self.physics.world_mut(), 0.1);
        }
        squad_height
    }

    /// The bits of the body state and of the action state.
    fn state_bits(&self) -> Vec<u32> {
        let mut checksum = SyncChecksum::new();
        self.action.hash_state(&mut checksum);
        let world = self.world();
        let rotation = world.rotation(self.body).expect("body");
        [
            self.position(),
            self.velocity(),
            world.angular_velocity(self.body).expect("body"),
        ]
        .iter()
        .flat_map(glam::Vec3::to_array)
        .chain(rotation.to_array())
        .map(f32::to_bits)
        .chain(std::iter::once(checksum.value()))
        .collect()
    }
}

/// Adds a terrain body with the retail material: a 64 × 64 grid with tile size 2.
fn add_terrain(world: &mut World, height: impl Fn(f32, f32) -> f32) {
    const SIZE: u16 = 64;
    const TILE: f32 = 2.0;
    let heights = (0..SIZE * SIZE)
        .map(|index| {
            height(
                f32::from(index % SIZE) * TILE,
                f32::from(index / SIZE) * TILE,
            )
        })
        .collect();
    let field = HeightField::new(usize::from(SIZE), TILE, heights).expect("terrain");
    let position = field.body_position();
    let group = world.filter_mut().new_system_group();
    let mut desc = BodyDesc::new(
        Arc::new(Shape::HeightField(Arc::new(field))),
        MotionType::Fixed,
        position,
    );
    desc.friction = 1.9;
    desc.restitution = 1.0;
    desc.filter = FilterInfo::for_layer(Layer::Terrain, group);
    world.add_body(desc);
}

/// A squad at `position` with its turn-radius point there too, facing `forward`.
fn squad_at(position: Vec3, forward: Vec3, move_action: Option<MoveActionInput>) -> SquadInput {
    SquadInput {
        position,
        turn_radius_position: Vec3::new(position.x, 0.0, position.z),
        turn_radius_forward: forward,
        desired_velocity: 20.0,
        stasis_speed_multiplier: 1.0,
        move_action,
    }
}

/// A working move action toward `target` at `velocity`.
fn moving_to(target: Vec3, velocity: f32) -> MoveActionInput {
    MoveActionInput {
        target,
        working: true,
        waiting: false,
        velocity,
        future_positions: [target; 5],
    }
}

/// The yaw of a forward axis, measured from `+z` toward `+x`.
fn yaw_of(forward: Vec3) -> f32 {
    crate::math::atan2(forward.x, forward.z)
}

/// The rotation of a quaternion as a matrix, for the up axis checks.
fn up_of(rotation: Quat) -> Vec3 {
    Mat3::from_quat(rotation).y_axis
}
