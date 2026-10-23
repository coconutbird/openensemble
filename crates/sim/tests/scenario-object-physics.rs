//! Checks the bodies of the shipped class-0 objects with a `PhysicsInfo` on Blood Gulch: the
//! MAC blast rocks (`ObjectClass Object`).
//!
//! `BObject_initFromProto` (`0x14029C300`, 2008 `object.cpp:3656-3669`) checks no object class,
//! so a class-0 object of a proto object with a `PhysicsInfo` gets its body like a unit.
//! `BEntity_setupVehiclePhysics` (`0x1402055D0`) gives no vehicle action to a class other than
//! Unit.

use glam::Vec3;
use physics::{Layer, MotionType};
use sim::{EntityClass, EntityId, GameplayCatalog, World, object_prototype_id, spawn_object_at};

const ROCKS: [(&str, &str); 3] = [
    ("pow_gp_macblast_rocks_small", "smallRock"),
    ("pow_gp_macblast_rocks_medium", "mediumRock"),
    ("pow_gp_macblast_rocks_large", "largeRock"),
];
const TICK_SECONDS: f32 = 0.05;
/// The rocks fall from this height above the terrain.
const DROP_HEIGHT: f32 = 8.0;
/// The distance between the rocks.
const SPACING: f32 = 12.0;
/// The ticks that the rocks get to fall and come to rest.
const SETTLE_TICKS: usize = 400;
/// The ticks with an exact readback check.
const READBACK_TICKS: usize = 10;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-object-physics -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_gives_the_macblast_rocks_retail_bodies() {
    let checksums = [run(), run()];
    assert_eq!(checksums[0], checksums[1]);
}

/// Drops the three rocks, checks their bodies, the readback and the rest, and gives the
/// checksums of the run.
fn run() -> Vec<u32> {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    world.create_pending_physics_objects();

    let rocks = drop_rocks(world, database);
    let mut checksums = vec![world.checksum()];
    for _ in 0..READBACK_TICKS {
        assert_readback(world, database, gameplay, &rocks);
        checksums.push(world.checksum());
    }
    let settled = (0..SETTLE_TICKS).any(|_| {
        tick(world, database, gameplay);
        checksums.push(world.checksum());
        rocks
            .iter()
            .all(|&rock| body_is_active(world, rock) == Some(false))
    });
    assert!(settled, "the rocks did not come to rest");
    tick(world, database, gameplay);
    for &rock in &rocks {
        assert_rests_on_the_terrain(world, rock);
    }
    let body = world.physics_state(rocks[0]).expect("the rock body").body();
    assert!(world.remove_object(rocks[0]).is_some());
    assert_eq!(world.physics().world().position(body), None);
    checksums.push(world.checksum());
    checksums
}

fn tick(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    world.update_entities_with_database_and_gameplay(TICK_SECONDS, database, gameplay);
}

fn body_is_active(world: &World, rock: EntityId) -> Option<bool> {
    let body = world.physics_state(rock)?.body();
    world.physics().world().is_active(body)
}

/// The first point on a ring search around the map centre where the terrain 20 units around
/// is level with it, at the terrain height.
fn flat_point(world: &World) -> Vec3 {
    let bounds = world.terrain_bounds().expect("scenario terrain bounds");
    let center = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    let height = |point: Vec3| world.terrain_height(point, true);
    (0..24_u8)
        .flat_map(|ring| {
            let distance = 8.0 * f32::from(ring);
            [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z].map(|axis| center + axis * distance)
        })
        .find_map(|point| {
            let ground = height(point)?;
            [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
                .iter()
                .all(|&axis| {
                    height(point + axis * 20.0).is_some_and(|near| (near - ground).abs() < 1e-3)
                })
                .then_some(Vec3::new(point.x, ground, point.z))
        })
        .expect("flat ground near the map centre")
}

/// Spawns the three rocks in a row above flat ground and checks their new bodies.
fn drop_rocks(world: &mut World, database: &pipeline::database::hw1::Database) -> Vec<EntityId> {
    let ground = flat_point(world);
    let rocks = ROCKS
        .iter()
        .zip([-SPACING, 0.0, SPACING])
        .map(|(&(proto, _), offset)| {
            let position = ground + Vec3::new(offset, DROP_HEIGHT, 0.0);
            spawn_object_at(
                world,
                database,
                0,
                object_prototype_id(database, proto).expect("the shipped rock"),
                position,
                Vec3::Z,
            )
            .expect("the rock spawns")
        })
        .collect::<Vec<_>>();
    for &rock in &rocks {
        assert_eq!(rock.class(), Some(EntityClass::Object));
        assert!(
            world.physics_state(rock).is_none(),
            "the body waits for the update"
        );
    }
    world.create_pending_physics_objects();
    for (&rock, &(_, info)) in rocks.iter().zip(&ROCKS) {
        assert_rock_body(world, rock, info);
    }
    rocks
}

/// A `BoxInertia` body of the rock info with a new Objects filter and no vehicle action.
fn assert_rock_body(world: &World, rock: EntityId, info: &str) {
    let state = world.physics_state(rock).expect("the rock has a body");
    assert!(
        state.info().eq_ignore_ascii_case(info),
        "{} != {info}",
        state.info()
    );
    assert!(!state.is_fixed());
    assert!(!state.has_vehicle_action());
    let body = state.body();
    let havok = world.physics().world();
    assert_eq!(havok.motion_type(body), Some(MotionType::BoxInertia));
    assert_eq!(
        havok.filter_info(body).map(physics::FilterInfo::layer),
        Some(Layer::Objects as u8)
    );
    assert!(havok.mass(body).is_some_and(|mass| mass >= 1.0));
    let object = world.get_object(rock).expect("the rock");
    let rotation = havok.rotation(body).expect("the body rotation");
    assert_eq!(
        havok.position(body),
        Some(object.base.position + rotation * state.center_offset())
    );
}

/// `BUnitActionPhysics_update` in `BObject::update`: each rock takes `body position −
/// R·centerOffset`, the body axes and the body velocity before the step.
fn assert_readback(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    rocks: &[EntityId],
) {
    let poses = rocks
        .iter()
        .map(|&rock| {
            let state = world.physics_state(rock).expect("the rock body");
            let havok = world.physics().world();
            let body = state.body();
            assert_eq!(havok.is_active(body), Some(true));
            (
                havok.position(body).expect("the body position"),
                havok.rotation(body).expect("the body rotation"),
                havok.linear_velocity(body).expect("the body velocity"),
                state.center_offset(),
            )
        })
        .collect::<Vec<_>>();
    tick(world, database, gameplay);
    for (&rock, &(position, rotation, velocity, center_offset)) in rocks.iter().zip(&poses) {
        let object = world.get_object(rock).expect("the rock");
        assert_eq!(object.base.position, position - rotation * center_offset);
        assert_eq!(object.base.forward, rotation * Vec3::Z);
        assert_eq!(object.base.up, rotation * Vec3::Y);
        assert_eq!(object.base.velocity, velocity);
    }
}

/// A rock at rest lies on the terrain: its body is below the drop height and its object
/// stays within its half size of the terrain.
fn assert_rests_on_the_terrain(world: &World, rock: EntityId) {
    let state = world.physics_state(rock).expect("the rock body");
    let object = world.get_object(rock).expect("the rock");
    let ground = world
        .terrain_height(object.base.position, true)
        .expect("the rock is over the terrain");
    let reach = state.half_extents().length() + state.center_offset().length();
    assert!(
        (object.base.position.y - ground).abs() <= reach,
        "the rock rests at {}, the terrain is at {ground}, the reach is {reach}",
        object.base.position.y
    );
    assert!(
        object.base.velocity.length() < 0.05,
        "the rock still moves at {}",
        object.base.velocity
    );
}
