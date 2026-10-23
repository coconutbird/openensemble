#![expect(
    dead_code,
    reason = "the plumbing commit adds the state before the body creation uses it"
)]
//! The physics world of a sim `World` and the shared state that `BPhysicsWorld` keeps next to it.

use std::sync::Arc;

use ::physics::{BodyId, FilterInfo, Layer, Shape, World as PhysicsWorld, WorldConfig};

use super::vehicle_hull::vehicle_hull_shape;

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
    filters: SharedFilters,
    /// The fixed terrain heightfield body, after the world has a terrain.
    terrain_body: Option<BodyId>,
    /// The 22-vertex hull that every non-aircraft vehicle uses (`BPhysicsWorld+0x10`).
    vehicle_hull: Arc<Shape>,
    /// The creation index of the next vehicle action (the `hkpWorld::addAction` order).
    next_action_order: u32,
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
        let mut world = PhysicsWorld::new(WorldConfig::default());
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
            filters,
            terrain_body: None,
            vehicle_hull: Arc::new(vehicle_hull_shape()),
            next_action_order: 0,
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

    /// The shared filter infos.
    pub(crate) const fn filters(&self) -> SharedFilters {
        self.filters
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
    pub(crate) fn vehicle_hull(&self) -> &Arc<Shape> {
        &self.vehicle_hull
    }

    /// Gives out the creation index of a new vehicle action.
    pub(crate) const fn next_action_order(&mut self) -> u32 {
        let order = self.next_action_order;
        self.next_action_order += 1;
        order
    }
}

fn create_collision_filter_info(world: &mut PhysicsWorld, layer: Layer) -> FilterInfo {
    let group = world.filter_mut().new_system_group();
    FilterInfo::for_layer(layer, group)
}
