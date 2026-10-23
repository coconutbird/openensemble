use std::sync::Arc;

use glam::Vec3;
use physics::{BodyDesc, MotionType, Shape};
use pipeline::database::hw1::physics::PhysicsVehicleType;

use crate::entities::UnitState;
use crate::physics::UnitPhysics;
use crate::physics::vehicle_actions::VehicleAction;
use crate::world::World;

/// A world with one squad of one unit with a body and the action of the type.
fn world_with_vehicle(vehicle_type: PhysicsVehicleType) -> (World, crate::entity_id::EntityId) {
    let mut world = World::with_seed(1);
    world.init_players(2);
    let position = Vec3::new(10.0, 0.0, 10.0);
    let squad_id = world.create_squad_at(1, position);
    let unit_id = world.create_unit_at(1, position);
    let physics = world.physics.world_mut();
    let mut desc = BodyDesc::new(
        Arc::new(Shape::Sphere { radius: 1.0 }),
        MotionType::BoxInertia,
        position,
    );
    desc.mass = 100.0;
    let body = physics.add_body(desc);
    let (action, _) = VehicleAction::setup(physics, body, vehicle_type, None).expect("vehicle");
    let unit = world.units.get_mut(unit_id).expect("unit");
    unit.squad_id = Some(squad_id);
    unit.turn_rate_degrees = 90.0;
    unit.desired_velocity = 10.0;
    unit.state = UnitState::Moving;
    let mut state = UnitPhysics::new(body, Vec3::ZERO, String::from("test"), false);
    state.action = Some(action);
    unit.physics_object = Some(state);
    world.squads.get_mut(squad_id).expect("squad").unit_ids = vec![unit_id];
    (world, squad_id)
}

#[test]
fn a_ground_vehicle_squad_starts_and_moves_its_turn_radius_point() {
    let (mut world, squad_id) = world_with_vehicle(PhysicsVehicleType::Scorpion);
    world.update_squad_turn_radius(squad_id, 0.05);
    let squad = world.squads.get_mut(squad_id).expect("squad");
    assert!(squad.turn_radius().is_some());
    squad.base.position.z += 20.0;
    world.update_squad_turn_radius(squad_id, 0.05);
    let (position, _) = world
        .squads
        .get(squad_id)
        .and_then(crate::entities::squads::Squad::turn_radius)
        .expect("point");
    assert!((position.z - 11.0).abs() < 1e-4, "{position}");
}

#[test]
fn an_aircraft_squad_has_no_turn_radius_point() {
    let (mut world, squad_id) = world_with_vehicle(PhysicsVehicleType::Hornet);
    world.update_squad_turn_radius(squad_id, 0.05);
    assert!(
        world
            .squads
            .get(squad_id)
            .and_then(crate::entities::squads::Squad::turn_radius)
            .is_none()
    );
}
