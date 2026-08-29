use glam::Vec3;
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::{
    ChildObject, ChildObjectType, ChildObjects, ObjectCommand, PopulationAmount, ResourceCost,
    Socket,
};
use pipeline::database::hw1::{Database, GameData, ProtoObject, Vector3};
use sim::{
    BuildingCommand, ConstructionError, ConstructionKind, EntityId, MS_PER_TICK, Simulation, World,
    object_prototype_id, object_runtime_id, spawn_object_at,
};

const BUILDER: &str = "test_builder";
const DIRECT: &str = "test_direct_building";
const SOCKET_BUILDING: &str = "test_socket_building";
const MANUAL: &str = "test_manual_building";
const SOCKET: &str = "test_hardpoint_socket";

#[test]
fn direct_build_command_pays_constructs_and_defers_cap_addition() {
    let database = construction_database();
    let (mut world, builder_id) = construction_world(&database);
    let target_id = object_runtime_id(&database, DIRECT).unwrap();
    let mut clock = Simulation::new();

    enqueue(
        &mut clock,
        BuildingCommand::build(
            1,
            vec![builder_id],
            target_id,
            Vec3::new(12.0, 0.0, 8.0),
            1,
            EntityId::INVALID,
        ),
    );
    clock.tick_with_world_and_database(&mut world, &database);

    let building_id = constructed_by(&world, builder_id, DIRECT);
    let building = world.get_building(building_id).unwrap();
    assert!(!building.built);
    assert_eq!(building.base.position, Vec3::new(12.0, 0.0, 8.0));
    assert_close(world.get_player(1).unwrap().resources.get(0), 900.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 1.0);
    assert_close(world.get_player(1).unwrap().population[0].cap, 10.0);
    assert_close(
        world
            .construction_progress(1, building_id, ConstructionKind::Build, target_id)
            .unwrap()
            .unwrap()
            .current_points,
        0.05,
    );

    clock.tick_with_world_and_database(&mut world, &database);
    assert!(world.get_building(building_id).unwrap().built);
    assert_close(world.get_player(1).unwrap().population[0].cap, 12.0);
    assert_eq!(
        clock
            .tick_with_world_and_database(&mut world, &database)
            .len(),
        0
    );
}

#[test]
fn direct_build_cancel_refunds_and_removes_the_unfinished_target() {
    let database = construction_database();
    let (mut world, builder_id) = construction_world(&database);
    let target_id = object_runtime_id(&database, DIRECT).unwrap();
    let building_id = world
        .start_build(
            1,
            builder_id,
            &database,
            target_id,
            Vec3::X,
            EntityId::INVALID,
        )
        .unwrap();
    assert_close(world.get_player(1).unwrap().resources.get(0), 900.0);

    assert!(world.cancel_build(1, building_id, target_id).unwrap());
    assert!(world.get_building(building_id).is_none());
    assert_close(world.get_player(1).unwrap().resources.get(0), 1_000.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 0.0);
    assert_close(world.get_player(1).unwrap().population[0].cap, 10.0);
}

#[test]
fn build_other_uses_socket_transform_and_releases_future_pop_on_completion() {
    let database = construction_database();
    let (mut world, builder_id) = construction_world(&database);
    let target_id = object_runtime_id(&database, SOCKET_BUILDING).unwrap();
    let mut clock = Simulation::new();

    enqueue(
        &mut clock,
        BuildingCommand::build_other(1, vec![builder_id], target_id, 1),
    );
    clock.tick_with_world_and_database(&mut world, &database);

    let building_id = constructed_by(&world, builder_id, SOCKET_BUILDING);
    let building = world.get_building(building_id).unwrap();
    assert!(!building.built);
    assert!(
        building
            .base
            .position
            .abs_diff_eq(Vec3::new(4.0, 0.0, 6.0), 1.0e-6)
    );
    assert!(building.base.forward.abs_diff_eq(Vec3::X, 1.0e-6));
    assert_close(world.get_player(1).unwrap().resources.get(0), 950.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 1.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 1.0);

    clock.tick_with_world_and_database(&mut world, &database);
    clock.tick_with_world_and_database(&mut world, &database);
    assert!(world.get_building(building_id).unwrap().built);
    assert_close(world.get_player(1).unwrap().population[0].future, 1.0);
    assert_close(world.get_player(1).unwrap().population[0].cap, 13.0);

    clock.tick_with_world_and_database(&mut world, &database);
    assert_close(world.get_player(1).unwrap().population[0].future, 0.0);
    assert!(world.get_building(builder_id).unwrap().production.is_idle());
    assert!(matches!(
        world.queue_build_other(1, builder_id, &database, target_id),
        Err(ConstructionError::SocketUnavailable { .. })
    ));
}

#[test]
fn canceling_active_build_other_refunds_and_frees_its_socket() {
    let database = construction_database();
    let (mut world, builder_id) = construction_world(&database);
    let target_id = object_runtime_id(&database, SOCKET_BUILDING).unwrap();

    world
        .queue_build_other(1, builder_id, &database, target_id)
        .unwrap();
    let update = world.update_production(0.05, &database);
    assert_eq!(update.completed_construction, 0);
    let building_id = constructed_by(&world, builder_id, SOCKET_BUILDING);

    assert!(world.cancel_build_other(1, builder_id, target_id).unwrap());
    assert!(world.get_building(building_id).is_none());
    assert_close(world.get_player(1).unwrap().resources.get(0), 1_000.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 0.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 0.0);
    world
        .queue_build_other(1, builder_id, &database, target_id)
        .expect("cancellation should free the virtual socket");
}

#[test]
fn manual_build_waits_for_external_build_points() {
    let database = construction_database();
    let (mut world, builder_id) = construction_world(&database);
    let target_id = object_runtime_id(&database, MANUAL).unwrap();
    let building_id = world
        .start_build(
            1,
            builder_id,
            &database,
            target_id,
            Vec3::ZERO,
            EntityId::INVALID,
        )
        .unwrap();

    let update = world.update_production(5.0, &database);
    assert_eq!(update.completed_construction, 0);
    assert!(!world.get_building(building_id).unwrap().built);
    assert!(world.add_build_points(1, building_id, 0.1));
    let update = world.update_production(0.05, &database);
    assert_eq!(update.completed_construction, 1);
    assert!(world.get_building(building_id).unwrap().built);
}

#[test]
fn destroyed_direct_construction_does_not_refund_its_cost() {
    let database = construction_database();
    let (mut world, builder_id) = construction_world(&database);
    let target_id = object_runtime_id(&database, DIRECT).unwrap();
    let building_id = world
        .start_build(
            1,
            builder_id,
            &database,
            target_id,
            Vec3::ZERO,
            EntityId::INVALID,
        )
        .unwrap();

    assert!(world.damage_unit(building_id, 10_000.0));
    world.update_entities(0.05);
    assert!(world.get_building(building_id).is_none());
    assert_close(world.get_player(1).unwrap().resources.get(0), 900.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 0.0);
}

#[test]
fn construction_progress_is_part_of_the_deterministic_checksum() {
    let database = construction_database();
    let (mut left, left_builder) = construction_world(&database);
    let (mut right, right_builder) = construction_world(&database);
    let target_id = object_runtime_id(&database, DIRECT).unwrap();

    let left_target = left
        .start_build(
            1,
            left_builder,
            &database,
            target_id,
            Vec3::X,
            EntityId::INVALID,
        )
        .unwrap();
    let right_target = right
        .start_build(
            1,
            right_builder,
            &database,
            target_id,
            Vec3::X,
            EntityId::INVALID,
        )
        .unwrap();
    assert_eq!(left_target, right_target);
    assert_eq!(left.checksum(), right.checksum());

    let before_progress = left.checksum();
    let _left_update = left.update_production(0.05, &database);
    assert_ne!(left.checksum(), before_progress);
    let _right_update = right.update_production(0.05, &database);
    assert_eq!(left.checksum(), right.checksum());
}

#[test]
fn player_object_forbids_gate_new_construction() {
    let database = construction_database();
    let (mut world, builder_id) = construction_world(&database);
    let runtime_id = object_runtime_id(&database, DIRECT).unwrap();
    let forbid_id = object_prototype_id(&database, DIRECT).unwrap();

    assert_eq!(
        world
            .get_player_mut(1)
            .unwrap()
            .set_object_forbidden(&database, forbid_id, true),
        Some(true)
    );
    assert!(matches!(
        world.start_build(
            1,
            builder_id,
            &database,
            runtime_id,
            Vec3::new(12.0, 0.0, 8.0),
            EntityId::INVALID,
        ),
        Err(ConstructionError::CommandUnavailable { .. })
    ));

    world
        .get_player_mut(1)
        .unwrap()
        .set_object_forbidden(&database, forbid_id, false);
    assert!(
        world
            .start_build(
                1,
                builder_id,
                &database,
                runtime_id,
                Vec3::new(12.0, 0.0, 8.0),
                EntityId::INVALID,
            )
            .is_ok()
    );
}

fn enqueue(clock: &mut Simulation, command: BuildingCommand) {
    clock
        .command_queue
        .enqueue_building(command, clock.game_time_ms + MS_PER_TICK, 1);
}

fn constructed_by(world: &World, builder_id: EntityId, name: &str) -> EntityId {
    world
        .units
        .iter()
        .find_map(|(id, unit)| {
            (unit.built_by == Some(builder_id) && unit.proto_object_name == name).then_some(id)
        })
        .expect("constructed building")
}

fn construction_world(database: &Database) -> (World, EntityId) {
    let mut world = World::new();
    world.init_players(1);
    let player = world.get_player_mut(1).unwrap();
    player.configure_population_slots(1);
    assert!(player.set_population_limits(0, 10.0, 20.0));
    player.resources.set(0, 1_000.0);
    let builder_id = spawn_object_at(
        &mut world,
        database,
        1,
        object_prototype_id(database, BUILDER).unwrap(),
        Vec3::ZERO,
        Vec3::Z,
    )
    .unwrap();
    (world, builder_id)
}

fn construction_database() -> Database {
    Database {
        objects: vec![
            ProtoObject {
                name: BUILDER.to_owned(),
                dbid: Some(100),
                object_class: Some("Building".to_owned()),
                commands: vec![
                    command("Build", DIRECT),
                    command("Build", MANUAL),
                    command("BuildOther", SOCKET_BUILDING),
                ],
                child_objects: Some(ChildObjects {
                    objects: vec![ChildObject {
                        proto_object: SOCKET.to_owned(),
                        child_type: Some(ChildObjectType::Socket),
                        offset: Some(Vector3 {
                            x: 3.0,
                            y: 0.0,
                            z: 4.0,
                        }),
                        ..ChildObject::default()
                    }],
                }),
                ..ProtoObject::default()
            },
            building(DIRECT, 101, 100.0, 0.1, 1.0, 2.0),
            ProtoObject {
                socket: Some(Socket {
                    object_type: "Hardpoint".to_owned(),
                    ..Socket::default()
                }),
                build_offset: Some(Vector3 {
                    x: 1.0,
                    y: 0.0,
                    z: 2.0,
                }),
                build_rotation: Some(90.0),
                ..building(SOCKET_BUILDING, 102, 50.0, 0.1, 1.0, 3.0)
            },
            ProtoObject {
                flags: vec!["ManualBuild".to_owned()],
                ..building(MANUAL, 103, 25.0, 0.1, 0.0, 0.0)
            },
            ProtoObject {
                name: SOCKET.to_owned(),
                dbid: Some(104),
                object_class: Some("Building".to_owned()),
                object_types: vec!["Hardpoint".to_owned()],
                flags: vec!["UseBuildRotation".to_owned()],
                ..ProtoObject::default()
            },
        ],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    deductable: Some(true),
                }],
            }),
            pops: Some(PopsWrapper {
                entries: vec!["Unit".to_owned()],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn building(
    name: &str,
    dbid: i32,
    cost: f32,
    build_points: f32,
    population: f32,
    cap_addition: f32,
) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Building".to_owned()),
        build_points: Some(build_points),
        costs: vec![ResourceCost {
            resource_type: "Supplies".to_owned(),
            amount: cost,
        }],
        population: positive_population(population),
        population_cap_additions: positive_population(cap_addition),
        ..ProtoObject::default()
    }
}

fn positive_population(amount: f32) -> Vec<PopulationAmount> {
    (amount > 0.0)
        .then_some(PopulationAmount {
            population_type: Some("Unit".to_owned()),
            amount,
        })
        .into_iter()
        .collect()
}

fn command(command_type: &str, target: &str) -> ObjectCommand {
    ObjectCommand {
        target: target.to_owned(),
        command_type: Some(command_type.to_owned()),
        ..ObjectCommand::default()
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0e-6,
        "{actual} != {expected}"
    );
}
