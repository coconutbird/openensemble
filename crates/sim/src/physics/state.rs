//! The physics world of a sim `World` and the shared state that `BPhysicsWorld` keeps next to it.

use std::sync::Arc;

use glam::Vec3;

use ::physics::{BodyId, FilterInfo, Layer, Shape, World as PhysicsWorld, WorldConfig};

use super::template::PhysicsTemplate;
use super::vehicle_hull::vehicle_hull_shape;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// The shared collision filter infos of `BPhysicsWorld_init` (`0x1407BB390`).
///
/// Each info has its own system group, so the bodies that share an info never collide with
/// each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SharedFilters {
    /// `BPhysicsWorld+0x58`: layer Vehicles.
    pub(crate) vehicle: FilterInfo,
    /// `BPhysicsWorld+0x5C`: layer `NoTerrainVehicles`.
    pub(crate) no_terrain_vehicle: FilterInfo,
    /// `BPhysicsWorld+0x60`: layer Phantoms.
    pub(crate) phantom: FilterInfo,
    /// `BPhysicsWorld+0x64`: layer Buildings.
    pub(crate) building: FilterInfo,
    /// `BPhysicsWorld+0x68`: layer Uncollidables.
    pub(crate) uncollidable: FilterInfo,
}

/// The physics of one sim `World` (`BPhysicsWorld`, `BWorld+0x250`).
#[derive(Clone, Debug)]
pub struct SimPhysics {
    world: PhysicsWorld,
    gravity: Vec3,
    filters: SharedFilters,
    /// The fixed terrain heightfield body, after the world has a terrain.
    terrain_body: Option<BodyId>,
    /// The 22-vertex hull that every non-aircraft vehicle uses (`BPhysicsWorld+0x10`).
    vehicle_hull: Arc<Shape>,
    /// The creation index of the next vehicle action (the `hkpWorld::addAction` order).
    next_action_order: u32,
    /// The units that wait for their body, in the order of their proto setup.
    pending_objects: Vec<(EntityId, Arc<PhysicsTemplate>)>,
}

impl Default for SimPhysics {
    fn default() -> Self {
        Self::new()
    }
}

impl SimPhysics {
    /// Makes the world like `BPhysicsWorld_init`: the retail world settings, then the five shared
    /// filter infos in retail order, then the vehicle hull.
    #[must_use]
    pub fn new() -> Self {
        let config = WorldConfig::default();
        let gravity = config.gravity;
        let mut world = PhysicsWorld::new(config);
        let mut create = |layer| create_collision_filter_info(&mut world, layer);
        let filters = SharedFilters {
            vehicle: create(Layer::Vehicles),
            no_terrain_vehicle: create(Layer::NoTerrainVehicles),
            phantom: create(Layer::Phantoms),
            building: create(Layer::Buildings),
            uncollidable: create(Layer::Uncollidables),
        };
        Self {
            world,
            gravity,
            filters,
            terrain_body: None,
            vehicle_hull: Arc::new(vehicle_hull_shape()),
            next_action_order: 0,
            pending_objects: Vec::new(),
        }
    }

    /// The Havok world.
    #[must_use]
    pub fn world(&self) -> &PhysicsWorld {
        &self.world
    }

    /// The Havok world, for the bodies, the impulses and the step.
    pub(crate) fn world_mut(&mut self) -> &mut PhysicsWorld {
        &mut self.world
    }

    /// The world gravity: (0, −21.7, 0).
    #[must_use]
    pub const fn gravity(&self) -> Vec3 {
        self.gravity
    }

    /// The shared filter infos.
    pub(crate) const fn filters(&self) -> SharedFilters {
        self.filters
    }

    /// The shared filter info of the vehicles (layer Vehicles).
    #[must_use]
    pub const fn vehicle_filter(&self) -> FilterInfo {
        self.filters.vehicle
    }

    /// The shared filter info of the ground vehicles (layer `NoTerrainVehicles`).
    #[must_use]
    pub const fn no_terrain_vehicle_filter(&self) -> FilterInfo {
        self.filters.no_terrain_vehicle
    }

    /// The shared filter info of the buildings (layer Buildings).
    #[must_use]
    pub const fn building_filter(&self) -> FilterInfo {
        self.filters.building
    }

    /// `BPhysicsWorld::createCollisionFilterInfo(layer)`: the layer with a new system group.
    pub(crate) fn create_collision_filter_info(&mut self, layer: Layer) -> FilterInfo {
        create_collision_filter_info(&mut self.world, layer)
    }

    /// The fixed terrain body, if the world has a terrain.
    #[must_use]
    pub const fn terrain_body(&self) -> Option<BodyId> {
        self.terrain_body
    }

    pub(crate) const fn set_terrain_body(&mut self, body: Option<BodyId>) {
        self.terrain_body = body;
    }

    /// The shared 22-vertex vehicle hull.
    #[must_use]
    pub fn vehicle_hull(&self) -> &Arc<Shape> {
        &self.vehicle_hull
    }

    /// Gives out the creation index of a new vehicle action.
    pub(crate) const fn next_action_order(&mut self) -> u32 {
        let order = self.next_action_order;
        self.next_action_order += 1;
        order
    }

    /// Queues the body of a unit. A unit that is in the queue is not added again.
    pub(crate) fn queue_object(&mut self, unit: EntityId, template: Arc<PhysicsTemplate>) {
        if self
            .pending_objects
            .iter()
            .all(|(queued, _)| *queued != unit)
        {
            self.pending_objects.push((unit, template));
        }
    }

    /// Removes a unit from the body queue.
    pub(crate) fn cancel_pending_object(&mut self, unit: EntityId) {
        self.pending_objects.retain(|(queued, _)| *queued != unit);
    }

    /// Takes the body queue.
    pub(crate) fn take_pending_objects(&mut self) -> Vec<(EntityId, Arc<PhysicsTemplate>)> {
        std::mem::take(&mut self.pending_objects)
    }

    /// Adds the state of every body to a checksum, in body id order: the transform, the
    /// velocities and whether the body is awake.
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        let world = &self.world;
        checksum.hash_u32(self.next_action_order);
        for body in world.bodies() {
            checksum.hash_u32(body.raw());
            for vector in [
                world.position(body),
                world.linear_velocity(body),
                world.angular_velocity(body),
            ]
            .into_iter()
            .flatten()
            {
                checksum.hash_vec3(vector.x, vector.y, vector.z);
            }
            if let Some(rotation) = world.rotation(body) {
                checksum.hash_vec3(rotation.x, rotation.y, rotation.z);
                checksum.hash_f32(rotation.w);
            }
            checksum.hash_u32(u32::from(world.is_active(body) == Some(true)));
        }
        for (unit, template) in &self.pending_objects {
            checksum.hash_u32(unit.as_u32());
            template.hash_state(checksum);
        }
    }
}

fn create_collision_filter_info(world: &mut PhysicsWorld, layer: Layer) -> FilterInfo {
    let group = world.filter_mut().new_system_group();
    FilterInfo::for_layer(layer, group)
}
