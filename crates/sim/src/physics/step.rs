//! The physics part of the world update: the terrain penetration fix before the entity updates,
//! and the Havok substeps after them (`BWorld_update`, `0x1405C5690`).

use ::physics::{
    BodyId, ContactAdded, ContactListener, ContactResponse, Layer, World as PhysicsWorld,
};

use crate::entity_id::EntityId;
use crate::world::World;

use super::unit::{CollisionRecord, UnitPhysicsContacts};
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
    /// substep applies the vehicle actions, then steps the Havok world. The listener reports
    /// the new contact points of each step to their units.
    pub(crate) fn step_physics(&mut self, elapsed: f32) {
        for (_, unit) in self.units.iter_mut() {
            if let Some(state) = unit.physics_object.as_mut() {
                state.contacts = UnitPhysicsContacts {
                    first_collision: state.contacts.first_collision,
                    ..UnitPhysicsContacts::default()
                };
            }
        }
        let (count, dt) = PhysicsWorld::substeps(elapsed);
        for _ in 0..count {
            apply_vehicle_actions(self, dt);
            let mut listener = StepListener::new(self);
            self.physics.world_mut().step_delta_time(dt, &mut listener);
            for (unit_id, record) in listener.records {
                if let Some(state) = self
                    .units
                    .get_mut(unit_id)
                    .and_then(|unit| unit.physics_object.as_mut())
                {
                    record_contact(&mut state.contacts, record);
                }
            }
        }
    }
}

/// Adds one new contact point to the contacts of a unit (`BUnitActionPhysics::collision`, the
/// landing test of `BUnitActionThrown::contactPointAddedCallback`).
fn record_contact(contacts: &mut UnitPhysicsContacts, record: CollisionRecord) {
    if contacts.first_collision.is_none() {
        contacts.first_collision = Some(record);
    }
    if record.other.is_some() {
        contacts.body_contacts = contacts.body_contacts.saturating_add(1);
        contacts.body_impact_speed = contacts.body_impact_speed.max(-record.projected_velocity);
    } else {
        contacts.terrain_impact_speed = contacts
            .terrain_impact_speed
            .max(-record.projected_velocity);
        contacts.landed |= record.projected_velocity < 0.0;
    }
}

/// A body that the listener knows.
#[derive(Clone, Copy)]
struct ListenerBody {
    body: BodyId,
    layer: u8,
    unit: Option<EntityId>,
    /// The body has `gVehicleCollisionListener` (a vehicle body).
    vehicle: bool,
}

/// The contact listeners of one Havok step.
struct StepListener {
    /// The bodies, sorted by body id.
    bodies: Vec<ListenerBody>,
    terrain: Option<BodyId>,
    /// The contact points of the units, in report order.
    records: Vec<(EntityId, CollisionRecord)>,
}

impl StepListener {
    fn new(world: &World) -> Self {
        let physics = world.physics.world();
        let mut bodies: Vec<ListenerBody> = physics
            .bodies()
            .filter_map(|body| {
                Some(ListenerBody {
                    body,
                    layer: physics.filter_info(body)?.layer(),
                    unit: None,
                    vehicle: false,
                })
            })
            .collect();
        for (unit_id, unit) in world.units.iter() {
            let Some(state) = unit.physics_object.as_ref() else {
                continue;
            };
            let parts = state.clamshell_bodies().unwrap_or([state.body; 2]);
            for body in [state.body, parts[0], parts[1]] {
                if let Ok(index) = bodies.binary_search_by_key(&body, |entry| entry.body) {
                    bodies[index].unit = Some(unit_id);
                    bodies[index].vehicle |= body == state.body && state.action.is_some();
                }
            }
        }
        Self {
            bodies,
            terrain: world.physics.terrain_body(),
            records: Vec::new(),
        }
    }

    fn body(&self, body: BodyId) -> Option<ListenerBody> {
        self.bodies
            .binary_search_by_key(&body, |entry| entry.body)
            .ok()
            .map(|index| self.bodies[index])
    }

    fn record(&mut self, own: ListenerBody, other: BodyId, event: &ContactAdded, sign: f32) {
        if let Some(unit) = own.unit {
            self.records.push((
                unit,
                CollisionRecord {
                    position: event.contact.position,
                    normal: event.contact.normal * sign,
                    projected_velocity: event.projected_velocity,
                    other: (Some(other) != self.terrain).then_some(other),
                },
            ));
        }
    }
}

impl ContactListener for StepListener {
    fn contact_point_added(&mut self, event: &ContactAdded) -> ContactResponse {
        let mut response = ContactResponse::ACCEPT;
        let (Some(a), Some(b)) = (self.body(event.body_a), self.body(event.body_b)) else {
            return response;
        };
        if a.vehicle || b.vehicle {
            response.inv_mass_scale = vehicle_against_object_scale(a.layer, b.layer);
        }
        // The normal points from B toward A.
        self.record(a, b.body, event, 1.0);
        self.record(b, a.body, event, -1.0);
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
