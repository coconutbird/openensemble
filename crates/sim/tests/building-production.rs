use glam::Vec3;
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::{
    ObjectCommand, PopulationAmount, ResourceCost, TrainLimit, TrainLimitType,
};
use pipeline::database::hw1::squads::{Cost as SquadCost, UnitEntry, UnitsWrapper};
use pipeline::database::hw1::techs::{EffectTarget, EffectsWrapper, TechEffect};
use pipeline::database::hw1::{Database, GameData, ProtoObject, Squad as ProtoSquad, Tech};
use sim::{
    BuildingCommand, EntityId, MS_PER_TICK, Simulation, TechStatus, TrainingError, TrainingKind,
    TrainingQueueResult, World, object_prototype_id, object_runtime_id, spawn_object_at,
    squad_prototype_id, squad_runtime_id, technology_prototype_id,
};

const BARRACKS: &str = "test_barracks";
const MARINE: &str = "test_marine";
const DRONE: &str = "test_drone";
const UPGRADE: &str = "test_upgrade";
const DISABLE_MARINES: &str = "disable_test_marines";

#[test]
fn train_squad_command_pays_reserves_completes_and_releases_population() {
    let database = production_database(false);
    let (mut world, barracks_id) = production_world(&database, 3.0, 1_000.0);
    let marine_id = squad_runtime_id(&database, MARINE).unwrap();
    let squads_before = world.squads.len();
    let mut clock = Simulation::new();

    enqueue_training(
        &mut clock,
        BuildingCommand::train_squads(1, vec![barracks_id], marine_id, 1),
    );
    clock.tick_with_world_and_database(&mut world, &database);
    assert_close(world.get_player(1).unwrap().resources.get(0), 900.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 1.0);
    let progress = world
        .training_progress(1, barracks_id, &database, TrainingKind::Squad, marine_id)
        .unwrap()
        .unwrap();
    assert!(!progress.queued);
    assert_close(progress.current_points, 0.0);

    clock.tick_with_world_and_database(&mut world, &database);
    assert_eq!(world.squads.len(), squads_before);
    clock.tick_with_world_and_database(&mut world, &database);
    assert_eq!(world.squads.len(), squads_before + 1);

    let squad_id = world
        .squads
        .iter()
        .find_map(|(id, squad)| (squad.trained_by == Some(barracks_id)).then_some(id))
        .expect("completed queue should create one linked squad");
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.proto_squad_name, MARINE);
    assert_eq!(squad.unit_ids.len(), 4);
    assert!(
        squad
            .unit_ids
            .iter()
            .all(|unit_id| world.get_unit(*unit_id).unwrap().is_garrisoned())
    );
    assert_eq!(trained_birth_count(&world, barracks_id), 1);
    assert_close(world.get_player(1).unwrap().population[0].future, 0.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 1.0);

    clock.tick_with_world_and_database(&mut world, &database);
    let squad = world.get_squad(squad_id).unwrap();
    assert!(squad.base.position.z > 0.0);
    assert!(
        squad
            .unit_ids
            .iter()
            .all(|unit_id| !world.get_unit(*unit_id).unwrap().is_garrisoned())
    );
    assert_eq!(trained_birth_count(&world, barracks_id), 0);

    world.remove_squad(squad_id).unwrap();
    assert_close(world.get_player(1).unwrap().population[0].count, 0.0);
}

#[test]
fn queue_accepts_a_prefix_and_cancellation_refunds_tail_then_current() {
    let database = production_database(false);
    let (mut world, barracks_id) = production_world(&database, 2.0, 250.0);
    let marine_id = squad_runtime_id(&database, MARINE).unwrap();
    let checksum_before = world.checksum();

    assert_eq!(
        world
            .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 3,)
            .unwrap(),
        TrainingQueueResult {
            accepted: 2,
            requested: 3,
        }
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 50.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 2.0);
    let update = world.update_production(0.05, &database);
    assert_eq!(update.completed_training, 0);

    assert_eq!(
        world
            .cancel_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 2,)
            .unwrap(),
        2
    );
    assert!(
        world
            .get_building(barracks_id)
            .unwrap()
            .production
            .is_idle()
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 250.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 0.0);
    assert_eq!(world.checksum(), checksum_before);
}

#[test]
fn research_and_training_use_one_worker_and_building_loss_refunds_both() {
    let database = production_database(false);
    let (mut world, barracks_id) = production_world(&database, 3.0, 500.0);
    let marine_id = squad_runtime_id(&database, MARINE).unwrap();
    let upgrade_id = technology_prototype_id(&database, UPGRADE).unwrap();

    world
        .queue_research(1, barracks_id, &database, upgrade_id)
        .unwrap();
    world
        .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 1)
        .unwrap();
    assert_close(world.get_player(1).unwrap().resources.get(0), 350.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 1.0);

    let _update = world.update_production(0.05, &database);
    assert!(
        world
            .get_building(barracks_id)
            .unwrap()
            .production
            .current_research()
            .is_some()
    );
    assert_eq!(
        world.technology_status(1, &database, upgrade_id).unwrap(),
        TechStatus::Researching
    );
    let _update = world.update_production(0.05, &database);
    assert_eq!(
        world.technology_status(1, &database, upgrade_id).unwrap(),
        TechStatus::Active
    );
    let _update = world.update_production(0.05, &database);
    assert!(
        world
            .get_building(barracks_id)
            .unwrap()
            .production
            .current_training()
            .is_some()
    );

    world.remove_unit(barracks_id).unwrap();
    assert_close(world.get_player(1).unwrap().resources.get(0), 450.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 0.0);
}

#[test]
fn train_unit_command_uses_runtime_object_ids() {
    let database = production_database(false);
    let (mut world, barracks_id) = production_world(&database, 3.0, 100.0);
    let drone_id = object_runtime_id(&database, DRONE).unwrap();
    let units_before = world.units.len();
    let mut clock = Simulation::new();

    enqueue_training(
        &mut clock,
        BuildingCommand::train_units(1, vec![barracks_id], drone_id, 1),
    );
    clock.tick_with_world_and_database(&mut world, &database);
    clock.tick_with_world_and_database(&mut world, &database);
    assert_eq!(world.units.len(), units_before + 1);
    let drone_id = world
        .units
        .iter()
        .find_map(|(id, unit)| (unit.trained_by == Some(barracks_id)).then_some(id))
        .expect("TrainUnit should create a linked standalone object");
    let drone = world.get_unit(drone_id).unwrap();
    assert_eq!(drone.proto_object_name, DRONE);
    let parent_squad = drone
        .squad_id
        .expect("retail assigns trained objects a parent squad");
    assert_eq!(world.get_squad(parent_squad).unwrap().unit_ids, [drone_id]);
    assert!(drone.is_garrisoned());
    assert_eq!(trained_birth_count(&world, barracks_id), 1);
    assert_close(world.get_player(1).unwrap().resources.get(0), 75.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 1.0);

    clock.tick_with_world_and_database(&mut world, &database);
    assert!(!world.get_unit(drone_id).unwrap().is_garrisoned());
    assert_eq!(trained_birth_count(&world, barracks_id), 0);
}

#[test]
fn train_unit_batch_escalates_against_future_counts_and_refunds_exact_prices() {
    let mut database = production_database(false);
    let drone = database
        .objects
        .iter_mut()
        .find(|object| object.name == DRONE)
        .unwrap();
    drone.cost_escalation = Some(10.0);
    drone.cost_escalation_object = vec![DRONE.to_owned()];
    drone.flags.push("LinearCostEscalation".to_owned());
    let (mut world, barracks_id) = production_world(&database, 10.0, 200.0);
    let drone_id = object_runtime_id(&database, DRONE).unwrap();

    assert_close(
        world.object_cost(&database, 1, drone_id).unwrap().get(0),
        25.0,
    );
    assert_eq!(
        world
            .queue_training(1, barracks_id, &database, TrainingKind::Unit, drone_id, 3)
            .unwrap(),
        TrainingQueueResult {
            accepted: 3,
            requested: 3,
        }
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 95.0);
    assert_close(
        world.object_cost(&database, 1, drone_id).unwrap().get(0),
        55.0,
    );

    assert_eq!(
        world
            .cancel_training(1, barracks_id, &database, TrainingKind::Unit, drone_id, 3)
            .unwrap(),
        3
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 200.0);
    assert_close(
        world.object_cost(&database, 1, drone_id).unwrap().get(0),
        25.0,
    );
}

#[test]
fn exponential_object_cost_rounds_to_the_nearest_ten_after_live_counts() {
    let mut database = production_database(false);
    let drone = database
        .objects
        .iter_mut()
        .find(|object| object.name == DRONE)
        .unwrap();
    drone.costs[0].amount = 100.0;
    drone.cost_escalation = Some(1.15);
    let (mut world, _barracks_id) = production_world(&database, 10.0, 1_000.0);
    let runtime_id = object_runtime_id(&database, DRONE).unwrap();
    let prototype_id = object_prototype_id(&database, DRONE).unwrap();

    assert_close(
        world.object_cost(&database, 1, runtime_id).unwrap().get(0),
        100.0,
    );
    spawn_object_at(&mut world, &database, 1, prototype_id, Vec3::X, Vec3::Z).unwrap();
    assert_close(
        world.object_cost(&database, 1, runtime_id).unwrap().get(0),
        120.0,
    );
    spawn_object_at(&mut world, &database, 1, prototype_id, Vec3::NEG_X, Vec3::Z).unwrap();
    assert_close(
        world.object_cost(&database, 1, runtime_id).unwrap().get(0),
        130.0,
    );
}

#[test]
fn instant_training_spawns_now_and_locks_only_its_building_command() {
    let mut database = production_database(false);
    let marine = database
        .squads
        .iter_mut()
        .find(|squad| squad.name == MARINE)
        .unwrap();
    marine.build_points = Some(1.0);
    marine.flags.push("InstantTrainWithRecharge".to_owned());
    let (mut world, barracks_id) = production_world(&database, 3.0, 500.0);
    world
        .get_building_mut(barracks_id)
        .unwrap()
        .work_rate_scalar = 10.0;
    let marine_id = squad_runtime_id(&database, MARINE).unwrap();
    let squads_before = world.squads.len();

    assert_eq!(
        world
            .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 2)
            .unwrap(),
        TrainingQueueResult {
            accepted: 2,
            requested: 2,
        }
    );
    assert_eq!(world.squads.len(), squads_before + 2);
    assert_eq!(trained_birth_count(&world, barracks_id), 2);
    assert!(
        world
            .get_building(barracks_id)
            .unwrap()
            .production
            .is_idle()
    );
    assert!(
        world
            .training_progress(1, barracks_id, &database, TrainingKind::Squad, marine_id)
            .unwrap()
            .is_none()
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 300.0);
    assert_close(world.get_player(1).unwrap().population[0].future, 0.0);
    assert_close(world.get_player(1).unwrap().population[0].count, 2.0);
    assert_close(
        world
            .training_recharge(barracks_id, TrainingKind::Squad, marine_id)
            .unwrap()
            .time_remaining(),
        1.0,
    );

    assert_eq!(
        world
            .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 1)
            .unwrap(),
        TrainingQueueResult {
            accepted: 0,
            requested: 1,
        }
    );
    assert_close(world.get_player(1).unwrap().resources.get(0), 300.0);
    let checksum_before_recharge_tick = world.checksum();

    let update = world.update_production(0.4, &database);
    assert_eq!(update.completed_training, 0);
    assert_eq!(trained_birth_count(&world, barracks_id), 1);
    assert_ne!(world.checksum(), checksum_before_recharge_tick);
    assert_close(
        world
            .training_recharge(barracks_id, TrainingKind::Squad, marine_id)
            .unwrap()
            .time_remaining(),
        0.6,
    );
    let _expired = world.update_production(0.6, &database);
    assert_eq!(trained_birth_count(&world, barracks_id), 0);
    assert!(
        world
            .training_recharge(barracks_id, TrainingKind::Squad, marine_id)
            .is_none()
    );

    assert_eq!(
        world
            .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 1)
            .unwrap()
            .accepted,
        1
    );
    assert_eq!(world.squads.len(), squads_before + 3);
    assert_close(world.get_player(1).unwrap().resources.get(0), 200.0);
}

#[test]
fn command_enable_and_train_limits_gate_authoritative_acceptance() {
    let database = production_database(true);
    let (mut world, barracks_id) = production_world(&database, 3.0, 500.0);
    let marine_id = squad_runtime_id(&database, MARINE).unwrap();

    let result = world
        .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 2)
        .unwrap();
    assert_eq!(result.accepted, 1);
    let _update = world.update_production(0.05, &database);
    let _update = world.update_production(0.10, &database);
    assert_eq!(
        world
            .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 1,)
            .unwrap()
            .accepted,
        0
    );

    let trained_id = world
        .squads
        .iter()
        .find_map(|(id, squad)| (squad.trained_by == Some(barracks_id)).then_some(id))
        .unwrap();
    world.remove_squad(trained_id).unwrap();
    assert_eq!(
        world
            .queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 1,)
            .unwrap()
            .accepted,
        1
    );
    world
        .cancel_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 1)
        .unwrap();

    world
        .activate_technology(1, &database, DISABLE_MARINES)
        .unwrap();
    assert!(matches!(
        world.queue_training(1, barracks_id, &database, TrainingKind::Squad, marine_id, 1,),
        Err(TrainingError::CommandUnavailable { .. })
    ));
}

#[test]
fn player_forbids_gate_new_squad_and_unit_training() {
    let database = production_database(false);
    let (mut world, barracks_id) = production_world(&database, 3.0, 500.0);
    let marine_runtime_id = squad_runtime_id(&database, MARINE).unwrap();
    let marine_forbid_id = squad_prototype_id(&database, MARINE).unwrap();
    let drone_runtime_id = object_runtime_id(&database, DRONE).unwrap();
    let drone_forbid_id = object_prototype_id(&database, DRONE).unwrap();
    let initial_checksum = world.checksum();

    assert_eq!(
        world
            .get_player_mut(1)
            .unwrap()
            .set_squad_forbidden(&database, marine_forbid_id, true),
        Some(true)
    );
    assert_ne!(world.checksum(), initial_checksum);
    assert!(matches!(
        world.queue_training(
            1,
            barracks_id,
            &database,
            TrainingKind::Squad,
            marine_runtime_id,
            1,
        ),
        Err(TrainingError::CommandUnavailable { .. })
    ));

    world
        .get_player_mut(1)
        .unwrap()
        .set_squad_forbidden(&database, marine_forbid_id, false);
    assert_eq!(
        world
            .queue_training(
                1,
                barracks_id,
                &database,
                TrainingKind::Squad,
                marine_runtime_id,
                1,
            )
            .unwrap()
            .accepted,
        1
    );
    world
        .cancel_training(
            1,
            barracks_id,
            &database,
            TrainingKind::Squad,
            marine_runtime_id,
            1,
        )
        .unwrap();

    world
        .get_player_mut(1)
        .unwrap()
        .set_object_forbidden(&database, drone_forbid_id, true);
    assert!(matches!(
        world.queue_training(
            1,
            barracks_id,
            &database,
            TrainingKind::Unit,
            drone_runtime_id,
            1,
        ),
        Err(TrainingError::CommandUnavailable { .. })
    ));
}

fn enqueue_training(clock: &mut Simulation, command: BuildingCommand) {
    clock
        .command_queue
        .enqueue_building(command, clock.game_time_ms + MS_PER_TICK, 1);
}

fn trained_birth_count(world: &World, building_id: EntityId) -> usize {
    world
        .get_building(building_id)
        .unwrap()
        .production
        .trained_squad_births()
        .count()
}

fn production_world(database: &Database, cap: f32, supplies: f32) -> (World, EntityId) {
    let mut world = World::new();
    world.init_players(1);
    let player = world.get_player_mut(1).unwrap();
    player.configure_population_slots(1);
    assert!(player.set_population_limits(0, cap, cap));
    player.resources.set(0, supplies);
    let barracks_proto_id = object_prototype_id(database, BARRACKS).unwrap();
    let barracks_id = spawn_object_at(
        &mut world,
        database,
        1,
        barracks_proto_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .unwrap();
    (world, barracks_id)
}

fn production_database(with_train_limit: bool) -> Database {
    Database {
        objects: production_objects(with_train_limit),
        squads: vec![marine_squad()],
        techs: production_techs(),
        game_data: Some(production_game_data()),
        ..Database::default()
    }
}

fn production_objects(with_train_limit: bool) -> Vec<ProtoObject> {
    let mut train_limits = Vec::new();
    if with_train_limit {
        train_limits.push(TrainLimit {
            target: MARINE.to_owned(),
            limit_type: Some(TrainLimitType::Squad),
            count: Some(1),
            bucket: None,
        });
    }
    vec![
        ProtoObject {
            name: BARRACKS.to_owned(),
            dbid: Some(100),
            object_class: Some("Building".to_owned()),
            obstruction_radius_x: Some(4.0),
            obstruction_radius_z: Some(5.0),
            commands: vec![
                ObjectCommand {
                    target: MARINE.to_owned(),
                    command_type: None,
                    ..ObjectCommand::default()
                },
                ObjectCommand {
                    target: DRONE.to_owned(),
                    command_type: Some("TrainUnit".to_owned()),
                    ..ObjectCommand::default()
                },
                ObjectCommand {
                    target: UPGRADE.to_owned(),
                    command_type: Some("Research".to_owned()),
                    ..ObjectCommand::default()
                },
            ],
            train_limits,
            ..ProtoObject::default()
        },
        ProtoObject {
            name: MARINE.to_owned(),
            dbid: Some(101),
            object_class: Some("Unit".to_owned()),
            population: vec![population(0.25)],
            ..ProtoObject::default()
        },
        ProtoObject {
            name: DRONE.to_owned(),
            dbid: Some(102),
            object_class: Some("Unit".to_owned()),
            build_points: Some(0.05),
            costs: vec![object_cost(25.0)],
            population: vec![population(1.0)],
            ..ProtoObject::default()
        },
    ]
}

fn marine_squad() -> ProtoSquad {
    ProtoSquad {
        name: MARINE.to_owned(),
        dbid: Some(200),
        build_points: Some(0.1),
        costs: vec![SquadCost {
            resource_type: "Supplies".to_owned(),
            amount: 100.0,
        }],
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: MARINE.to_owned(),
                count: 4,
                role: None,
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn production_techs() -> Vec<Tech> {
    vec![
        Tech {
            name: UPGRADE.to_owned(),
            research_points: Some(0.05),
            status: Some("OBTAINABLE".to_owned()),
            costs: vec![pipeline::database::hw1::techs::TechCost {
                resource_type: "Supplies".to_owned(),
                amount: 50.0,
            }],
            ..Tech::default()
        },
        Tech {
            name: DISABLE_MARINES.to_owned(),
            research_points: Some(0.0),
            effects: Some(EffectsWrapper {
                entries: vec![TechEffect {
                    effect_type: "Data".to_owned(),
                    amount: Some(0.0),
                    subtype: Some("CommandEnable".to_owned()),
                    relativity: Some("Absolute".to_owned()),
                    command_data: Some(MARINE.to_owned()),
                    command_type: Some("TrainSquad".to_owned()),
                    target: Some(EffectTarget {
                        target_type: Some("ProtoUnit".to_owned()),
                        value: Some(BARRACKS.to_owned()),
                    }),
                    ..TechEffect::default()
                }],
            }),
            ..Tech::default()
        },
    ]
}

fn production_game_data() -> GameData {
    GameData {
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
    }
}

fn object_cost(amount: f32) -> ResourceCost {
    ResourceCost {
        resource_type: "Supplies".to_owned(),
        amount,
    }
}

fn population(amount: f32) -> PopulationAmount {
    PopulationAmount {
        population_type: Some("Unit".to_owned()),
        amount,
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0e-6,
        "{actual} != {expected}"
    );
}
