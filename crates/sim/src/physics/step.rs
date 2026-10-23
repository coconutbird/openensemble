//! The physics part of the world update: the terrain penetration fix before the entity updates,
//! and the Havok substeps after them (`BWorld_update`, `0x1405C5690`).

use ::physics::{
    BodyId, ContactAdded, ContactListener, ContactResponse, Layer, World as PhysicsWorld,
};

use crate::entity_id::EntityId;
use crate::world::World;

use super::vehicle_actions::apply_vehicle_actions;

impl World {
    /// `BWorld_resolvePhysicsVehicleTerrainCollisions` (`0x1405D4090`): fixes each vehicle whose
    /// wheel went below the terrain in the last update.
    ///
    /// The vehicle action queues itself once (`BWorld_addPhysicsVehicleTerrainCollisionAsync`,
    /// `0x1405D2E20`). DE sorts the queue by entity id before the fix
    /// (`cmpPhysicsVehicleActionByEntityID`, `0x1405D4030`); the units are in entity id order.
    pub(crate) fn resolve_vehicle_terrain_penetrations(&mut self) {
        for (_, unit) in self.units.iter_mut() {
            let Some(state) = unit.physics_object.as_mut() else {
                continue;
            };
            let restitution = state.restitution;
            if let Some(action) = state.action.as_mut()
                && action.needs_terrain_resolution()
            {
                action.resolve_terrain_penetration(self.physics.world_mut(), restitution);
            }
        }
    }
    /// The units whose vehicle action runs this substep: an active body with an action, in
    /// action creation order.
    pub(crate) fn active_vehicle_actions(&self) -> Vec<EntityId> {
        let world = self.physics.world();
        let mut actions: Vec<(u32, EntityId)> = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                let state = unit.physics_object.as_ref()?;
                (state.action.is_some() && world.is_active(state.body) == Some(true))
                    .then_some((state.action_order, id))
            })
            .collect();
        actions.sort_unstable();
        actions.into_iter().map(|(_, id)| id).collect()
    }

    /// `BPhysicsWorld_update` (`0x1407BBB40`): splits the elapsed time into substeps. Each
    /// substep applies the vehicle actions, then steps the Havok world.
    pub(crate) fn step_physics(&mut self, elapsed: f32) {
        let (count, dt) = PhysicsWorld::substeps(elapsed);
        for _ in 0..count {
            apply_vehicle_actions(self, dt);
            let mut listener = StepListener::new(self.physics.world());
            self.physics.world_mut().step_delta_time(dt, &mut listener);
        }
    }
}

/// The contact listeners of one Havok step.
struct StepListener {
    /// The layer of each body, sorted by body id.
    layers: Vec<(BodyId, u8)>,
}

impl StepListener {
    fn new(world: &PhysicsWorld) -> Self {
        let layers = world
            .bodies()
            .filter_map(|body| Some((body, world.filter_info(body)?.layer())))
            .collect();
        Self { layers }
    }

    fn layer(&self, body: BodyId) -> Option<u8> {
        self.layers
            .binary_search_by_key(&body, |&(id, _)| id)
            .ok()
            .map(|index| self.layers[index].1)
    }
}

impl ContactListener for StepListener {
    fn contact_point_added(&mut self, event: &ContactAdded) -> ContactResponse {
        let mut response = ContactResponse::ACCEPT;
        if let (Some(a), Some(b)) = (self.layer(event.body_a), self.layer(event.body_b)) {
            response.inv_mass_scale = vehicle_against_object_scale(a, b);
        }
        response
    }
}

/// `gVehicleCollisionListener` (`BLayerFilterCollisionListener`, 2008
/// `physicsCollision.cpp:84-126`): a vehicle that touches an object keeps its motion. The
/// vehicle side of the contact gets an inverse mass factor of 0.
fn vehicle_against_object_scale(layer_a: u8, layer_b: u8) -> Option<[f32; 2]> {
    let is_vehicle =
        |layer| layer == Layer::Vehicles as u8 || layer == Layer::NoTerrainVehicles as u8;
    let objects = Layer::Objects as u8;
    if is_vehicle(layer_a) {
        (layer_b == objects).then_some([0.0, 1.0])
    } else if is_vehicle(layer_b) {
        (layer_a == objects).then_some([1.0, 0.0])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::vehicle_against_object_scale;
    use ::physics::Layer;

    #[test]
    fn a_vehicle_has_no_response_to_an_object() {
        let vehicle = Layer::Vehicles as u8;
        let ground = Layer::NoTerrainVehicles as u8;
        let objects = Layer::Objects as u8;
        assert_eq!(
            vehicle_against_object_scale(vehicle, objects),
            Some([0.0, 1.0])
        );
        assert_eq!(
            vehicle_against_object_scale(objects, ground),
            Some([1.0, 0.0])
        );
        assert_eq!(
            vehicle_against_object_scale(vehicle, Layer::Terrain as u8),
            None
        );
        assert_eq!(vehicle_against_object_scale(objects, objects), None);
    }
}
