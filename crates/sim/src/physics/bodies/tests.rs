//! The body creation rule for class-0 objects (`BObject_initFromProto`, `0x14029C300`).

use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

use ::physics::{Layer, MotionType};

use crate::entity_id::{EntityClass, EntityId};
use crate::physics::ObjectPhysicsTemplates;
use crate::physics::test_support::{
    TERRAIN_HEIGHT, box_template, hover_template, world_with_flat_terrain,
};
use crate::scenario::create_object_from_prototype;
use crate::world::World;

const ROCK: &str = "test_rock";
const CENTER_OFFSET: Vec3 = Vec3::new(0.0, 0.5, 0.0);
const SPAWN: Vec3 = Vec3::new(14.0, TERRAIN_HEIGHT + 4.0, 14.0);

fn rock_database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: ROCK.to_owned(),
            object_class: Some("Object".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    }
}

/// A world with a class-0 rock object of `info` above the flat terrain. The body is still in
/// the queue.
fn world_with_rock(info: ObjectPhysicsTemplates) -> (World, EntityId) {
    let mut world = world_with_flat_terrain();
    world.set_prototype_physics(ROCK, info);
    let rock = create_object_from_prototype(&mut world, 1, SPAWN, Vec3::Z, ROCK, &rock_database())
        .expect("the rock spawns");
    (world, rock)
}

fn rock_info() -> ObjectPhysicsTemplates {
    ObjectPhysicsTemplates {
        info: Some(box_template(ROCK, Vec3::splat(0.5), 20.0, CENTER_OFFSET)),
        replacement: None,
    }
}

#[test]
fn a_class_zero_object_with_a_physics_info_gets_a_dynamic_body() {
    let (mut world, rock) = world_with_rock(rock_info());
    assert_eq!(rock.class(), Some(EntityClass::Object));
    assert!(world.physics_state(rock).is_none());

    world.create_pending_physics_objects();

    let state = world.physics_state(rock).expect("the rock has a body");
    assert_eq!(state.info(), ROCK);
    assert!(!state.is_fixed());
    let body = state.body();
    let havok = world.physics().world();
    assert_eq!(havok.motion_type(body), Some(MotionType::BoxInertia));
    assert_eq!(havok.position(body), Some(SPAWN + CENTER_OFFSET));
    assert_eq!(
        havok.filter_info(body).map(::physics::FilterInfo::layer),
        Some(Layer::Objects as u8)
    );
}

#[test]
fn a_class_zero_object_without_a_physics_info_gets_no_body() {
    let (mut world, rock) = world_with_rock(ObjectPhysicsTemplates {
        info: None,
        replacement: rock_info().info,
    });
    world.create_pending_physics_objects();
    assert!(world.physics_state(rock).is_none());
}

/// `BEntity_setupVehiclePhysics` (`0x1402055D0`) returns for any class other than Unit.
#[test]
fn a_class_zero_object_gets_no_vehicle_action() {
    let (mut world, rock) = world_with_rock(ObjectPhysicsTemplates {
        info: Some(hover_template(ROCK, Vec3::splat(0.5))),
        replacement: None,
    });
    world.create_pending_physics_objects();

    let state = world.physics_state(rock).expect("the rock has a body");
    assert!(!state.has_vehicle_action());
    assert_eq!(
        world
            .physics()
            .world()
            .filter_info(state.body())
            .map(::physics::FilterInfo::layer),
        Some(Layer::Objects as u8)
    );
}

/// `BUnitActionPhysics_update` in `BObject::update`: the object takes `body position −
/// R·centerOffset` and the body axes before the step.
#[test]
fn the_object_reads_back_its_falling_body() {
    let (mut world, rock) = world_with_rock(rock_info());
    world.create_pending_physics_objects();
    for _ in 0..10 {
        let body = world.physics_state(rock).expect("the rock body").body();
        let havok = world.physics().world();
        let position = havok.position(body).expect("the body position");
        let rotation = havok.rotation(body).expect("the body rotation");
        let velocity = havok.linear_velocity(body).expect("the body velocity");
        world.update_entities(0.05);
        let object = world.get_object(rock).expect("the rock");
        assert_eq!(object.base.position, position - rotation * CENTER_OFFSET);
        assert_eq!(object.base.forward, rotation * Vec3::Z);
        assert_eq!(object.base.up, rotation * Vec3::Y);
        assert_eq!(object.base.velocity, velocity);
    }
    assert!(world.get_object(rock).expect("the rock").base.position.y < SPAWN.y);
}

#[test]
fn removing_an_object_removes_its_body() {
    let (mut world, rock) = world_with_rock(rock_info());
    world.create_pending_physics_objects();
    let body = world.physics_state(rock).expect("the rock body").body();

    assert!(world.remove_object(rock).is_some());
    assert_eq!(world.physics().world().position(body), None);
}

#[test]
fn the_trigger_no_physics_parameter_cancels_the_queued_object_body() {
    let (mut world, rock) = world_with_rock(rock_info());
    world.release_physics_object(rock);
    world.create_pending_physics_objects();
    assert!(world.physics_state(rock).is_none());
}

#[test]
fn the_checksum_covers_the_object_body() {
    let run = || {
        let (mut world, rock) = world_with_rock(rock_info());
        world.create_pending_physics_objects();
        for _ in 0..5 {
            world.update_entities(0.05);
        }
        (world, rock)
    };
    let (first, _) = run();
    let (mut second, rock) = run();
    assert_eq!(first.checksum(), second.checksum());

    let state = second
        .get_object_mut(rock)
        .and_then(|object| object.physics_object.as_mut())
        .expect("the rock body");
    state.flags.waiting = !state.flags.waiting;
    assert_ne!(first.checksum(), second.checksum());
}
