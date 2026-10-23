//! Checks the retail physics world of Blood Gulch: the bodies that `BObject_initFromProto` and
//! `BEntity_setupVehiclePhysics` make, the terrain body, a settled rock and the readback.

use glam::Vec3;
use physics::{BodyId, DEFAULT_CONVEX_RADIUS, MotionType, Shape};
use sim::{EntityId, GameplayCatalog, World, object_prototype_id, spawn_object_at};

const WARTHOG: &str = "unsc_veh_warthog_01";
/// A shipped proto object with the `rock` physics info (a 1 × 1 × 1 box of mass 20). The
/// macblast rocks are `ObjectClass Object`, and the sim gives those objects no body yet.
const ROCK: &str = "fld_egg_stalkEgg_01";
const TICK_SECONDS: f32 = 0.05;
/// The rock falls from this height above the terrain.
const DROP_HEIGHT: f32 = 6.0;
/// The ticks that the rock gets to fall and come to rest.
const SETTLE_TICKS: usize = 200;
/// The largest error of the rest gap against the convex radius.
const GAP_TOLERANCE: f32 = 0.01;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-physics-world -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_builds_the_retail_physics_world() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    world.create_pending_physics_objects();

    let warthog = player_warthog(world);
    assert_warthog_body(world, warthog);
    assert_fixed_buildings(world);
    assert_terrain_heights(world);
    assert_readback(world, database, gameplay, warthog);

    // The egg hatches when its body comes to rest (`fld_obj_eggPhysics.tactics`), so the check
    // runs at the first update that finds the body asleep.
    let rock = drop_rock(world, database, warthog);
    let settled = (0..SETTLE_TICKS).any(|_| {
        tick(world, database, gameplay);
        world.unit_body_is_active(rock) == Some(false)
    });
    assert!(settled, "the rock did not come to rest");
    assert_rock_rests_at_the_convex_radius(world, rock);
}

fn tick(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
) {
    world.update_entities_with_database_and_gameplay(TICK_SECONDS, database, gameplay);
}

fn player_warthog(world: &World) -> EntityId {
    world
        .units
        .iter()
        .find(|(_, unit)| {
            unit.base.player_id == 1 && unit.proto_object_name.eq_ignore_ascii_case(WARTHOG)
        })
        .map(|(id, _)| id)
        .expect("player 1 starts with a Warthog")
}

fn body_of(world: &World, unit_id: EntityId) -> BodyId {
    world.unit_body(unit_id).expect("the unit has a body")
}

/// `BEntity_setupVehiclePhysics`: a `BoxInertia` body with the shared 22-vertex hull, the
/// warthog inertia `diag(m, m, 0.4·m)` and the shared Vehicle filter.
fn assert_warthog_body(world: &World, warthog: EntityId) {
    let state = world
        .get_unit(warthog)
        .and_then(|unit| unit.physics_object.as_ref())
        .expect("the Warthog body");
    assert_eq!(state.info(), "warthog");
    assert!(state.has_vehicle_action());
    let body = state.body();
    let physics = world.physics();
    let havok = physics.world();
    assert_eq!(havok.motion_type(body), Some(MotionType::BoxInertia));
    let shape = havok.shape(body).expect("the Warthog shape");
    assert!(std::sync::Arc::ptr_eq(shape, physics.vehicle_hull()));
    let Shape::ConvexVertices { vertices, .. } = shape.as_ref() else {
        panic!("the Warthog shape is the vehicle hull, not {shape:?}");
    };
    assert_eq!(vertices.len(), 22);
    let mass = havok.mass(body).expect("the Warthog mass");
    assert!(mass >= 1.0);
    assert_eq!(
        havok.inertia_local(body),
        Some(Vec3::new(mass, mass, 0.4 * mass))
    );
    assert_eq!(havok.filter_info(body), Some(physics.vehicle_filter()));
}

/// `MotionType fixed`: every building body is fixed, has the building filter and no readback.
fn assert_fixed_buildings(world: &World) {
    let physics = world.physics();
    let mut buildings = 0;
    for (_, unit) in world.units.iter().filter(|(_, unit)| unit.is_building()) {
        let Some(state) = unit.physics_object.as_ref() else {
            continue;
        };
        buildings += 1;
        assert!(state.is_fixed(), "{} is not fixed", unit.proto_object_name);
        let body = state.body();
        assert_eq!(physics.world().motion_type(body), Some(MotionType::Fixed));
        assert_eq!(
            physics.world().filter_info(body),
            Some(physics.building_filter())
        );
    }
    assert!(buildings >= 2, "Blood Gulch starts with two bases");
}

/// The terrain body reads the same heights as the sim terrain queries.
fn assert_terrain_heights(world: &World) {
    let physics = world.physics();
    let terrain = physics.terrain_body().expect("the terrain body");
    let havok = physics.world();
    assert_eq!(havok.motion_type(terrain), Some(MotionType::Fixed));
    let field = havok.heightfield().expect("the terrain heightfield");
    assert_eq!(havok.position(terrain), Some(field.body_position()));
    let side = field.vertices_per_side();
    let scale = field.tile_scale();
    let mut samples = 0;
    for z in (0..side).step_by(37) {
        for x in (0..side).step_by(41) {
            let point = Vec3::new(index(x) * scale, 0.0, index(z) * scale);
            let sim = world
                .terrain_height(point, true)
                .expect("the sim terrain height");
            let vertex = field.sim_height(x, z).expect("a grid vertex");
            let (surface, _) = field
                .surface_at(point.x, point.z)
                .expect("a point on the heightfield");
            assert_eq!(vertex.to_bits(), sim.to_bits(), "vertex ({x}, {z})");
            assert!(
                (surface - sim).abs() < 1e-4,
                "vertex ({x}, {z}): physics {surface}, sim {sim}"
            );
            samples += 1;
        }
    }
    assert!(samples > 16);
}

fn index(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("the grid index fits u16"))
}

/// The first point on a ring search around `center` where the terrain 6 units around is
/// level with it, at the terrain height.
fn flat_point_near(world: &World, center: Vec3) -> Option<Vec3> {
    let height = |point: Vec3| world.terrain_height(point, true);
    (1..12_u8)
        .flat_map(|ring| {
            let distance = 8.0 * f32::from(ring);
            [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z].map(|axis| center + axis * distance)
        })
        .find_map(|point| {
            let ground = height(point)?;
            [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
                .iter()
                .all(|&axis| {
                    height(point + axis * 6.0).is_some_and(|near| (near - ground).abs() < 1e-3)
                })
                .then_some(Vec3::new(point.x, ground, point.z))
        })
}
/// Drops the rock on flat ground near the Warthog, in the player 1 base.
fn drop_rock(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    warthog: EntityId,
) -> EntityId {
    let start = world.get_unit(warthog).expect("the Warthog").base.position;
    let mut position = flat_point_near(world, start).expect("flat ground near the Warthog");
    position.y += DROP_HEIGHT;
    let rock = spawn_object_at(
        world,
        database,
        0,
        object_prototype_id(database, ROCK).expect("the shipped rock"),
        position,
        Vec3::Z,
    )
    .expect("the rock spawns");
    world.create_pending_physics_objects();
    let body = body_of(world, rock);
    assert_eq!(
        world.physics().world().motion_type(body),
        Some(MotionType::BoxInertia)
    );
    rock
}

/// A body at rest on the terrain keeps the convex radius between its core shape and the
/// heightfield.
fn assert_rock_rests_at_the_convex_radius(world: &World, rock: EntityId) {
    let body = body_of(world, rock);
    let havok = world.physics().world();
    let velocity = havok.linear_velocity(body).expect("the rock velocity");
    assert!(
        velocity.length() < 0.05,
        "the rock still moves at {velocity}"
    );
    let transform = havok.transform(body).expect("the rock transform");
    let (points, radius) = match havok.shape(body).expect("the rock shape").as_ref() {
        Shape::Box {
            half_extents,
            radius,
        } => (box_corners(*half_extents), *radius),
        Shape::ConvexVertices {
            vertices, radius, ..
        } => (vertices.clone(), *radius),
        other => panic!("the rock has no convex shape: {other:?}"),
    };
    assert_eq!(radius.to_bits(), DEFAULT_CONVEX_RADIUS.to_bits());
    let field = havok.heightfield().expect("the terrain heightfield");
    let gap = points
        .iter()
        .map(|&point| {
            let point = transform.transform_point(point);
            let (height, _) = field
                .surface_at(point.x, point.z)
                .expect("the rock is over the terrain");
            point.y - height
        })
        .fold(f32::MAX, f32::min);
    assert!(
        (gap - radius).abs() < GAP_TOLERANCE,
        "the rock rests {gap} above the terrain, the convex radius is {radius}"
    );
}

fn box_corners(half_extents: Vec3) -> Vec<Vec3> {
    (0..8)
        .map(|corner: u8| {
            let sign = |bit: u8| if corner & bit == 0 { -1.0 } else { 1.0 };
            half_extents * Vec3::new(sign(1), sign(2), sign(4))
        })
        .collect()
}

/// `BUnitActionPhysics_update`: the unit takes `body position − R·centerOffset` of its active
/// body before the step. The Warthog drops onto its suspension at the start, so its body is
/// active.
fn assert_readback(
    world: &mut World,
    database: &pipeline::database::hw1::Database,
    gameplay: &GameplayCatalog,
    warthog: EntityId,
) {
    let center_offset = world
        .get_unit(warthog)
        .and_then(|unit| unit.physics_object.as_ref())
        .expect("the Warthog body")
        .center_offset();
    for _ in 0..5 {
        assert_eq!(world.unit_body_is_active(warthog), Some(true));
        let position = world
            .unit_body_position(warthog)
            .expect("the body position");
        let rotation = world
            .unit_body_rotation(warthog)
            .expect("the body rotation");
        tick(world, database, gameplay);
        let unit = world.get_unit(warthog).expect("the Warthog");
        assert_eq!(unit.base.position, position - rotation * center_offset);
        assert_eq!(unit.base.forward, rotation * Vec3::Z);
        assert_eq!(unit.base.up, rotation * Vec3::Y);
    }
}
