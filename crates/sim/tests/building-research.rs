use glam::Vec3;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::ObjectCommand;
use pipeline::database::hw1::techs::{
    EffectTarget, EffectsWrapper, PrereqsWrapper, TechCost, TechEffect, TechStatusEntry,
    TypeCountEntry,
};
use pipeline::database::hw1::{Database, GameData, ProtoObject, Tech};
use sim::packet::channel_packet_type;
use sim::{
    BuildingCommand, BuildingCommandType, ChannelPacketHeader, CommandDispatcher,
    DispatchedCommand, EntityId, MS_PER_TICK, ResearchError, ResearchQueueResult, Simulation,
    TechStatus, object_prototype_id, serialize_command, spawn_object_at, technology_prototype_id,
};

const BARRACKS: &str = "test_barracks";
const UPGRADE_ONE: &str = "test_marine_upgrade1";
const UPGRADE_TWO: &str = "test_marine_upgrade2";
const INSTANT_UPGRADE: &str = "test_instant_upgrade";
const BUILDING_PREREQ: &str = "test_building_prereq";
const OR_PREREQ: &str = "test_or_prereq";

#[test]
fn building_packet_dispatches_the_fixed_retail_payload() {
    let mut command = BuildingCommand::research(1, vec![EntityId::from_u32(0x1000_0001)], 42, -1);
    command.target_position = Vec3::new(4.0, 5.0, 6.0);
    command.socket_id = EntityId::from_u32(0x1000_0002);
    let header = ChannelPacketHeader {
        time_offset: 50,
        packet_id: 7,
        packet_type: channel_packet_type::COMMAND_BUILDING,
        channel: 0,
    };
    let mut bytes = Vec::new();
    header.serialize(&mut bytes).unwrap();
    serialize_command(&command.base, &mut bytes).unwrap();
    command.serialize_fields(&mut bytes).unwrap();

    let (decoded_header, decoded) = CommandDispatcher::dispatch(&bytes).unwrap();
    assert_eq!(
        decoded_header.packet_type,
        channel_packet_type::COMMAND_BUILDING
    );
    let DispatchedCommand::Building(decoded) = decoded else {
        panic!("building packet should dispatch as BuildingCommand");
    };
    assert_eq!(decoded.base.player_id, 1);
    assert_eq!(decoded.base.recipients, command.base.recipients);
    assert_eq!(decoded.building_type, BuildingCommandType::Research);
    assert_eq!(decoded.target_id, 42);
    assert_eq!(decoded.target_position, command.target_position);
    assert_eq!(decoded.count, -1);
    assert_eq!(decoded.socket_id, command.socket_id);
}

#[test]
fn research_commands_pay_progress_activate_cancel_and_refund() {
    let database = research_database();
    let mut world = sim::World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().resources.amounts = [1_000.0, 10.0, 0.0, 0.0];
    let first_barracks = spawn_barracks(&mut world, &database);
    let second_barracks = spawn_barracks(&mut world, &database);
    let upgrade_one = technology_prototype_id(&database, UPGRADE_ONE).unwrap();
    let upgrade_two = technology_prototype_id(&database, UPGRADE_TWO).unwrap();
    let mut simulation = Simulation::new();

    assert_eq!(
        world.technology_status(1, &database, upgrade_one).unwrap(),
        TechStatus::Available
    );
    assert_eq!(
        world.technology_status(1, &database, upgrade_two).unwrap(),
        TechStatus::Obtainable
    );

    enqueue_research(&mut simulation, first_barracks, upgrade_one);
    simulation.tick_with_world_and_database(&mut world, &database);
    assert_resources(&world, [800.0, 9.0, 0.0, 0.0]);
    assert_eq!(
        world.technology_status(1, &database, upgrade_one).unwrap(),
        TechStatus::Researching
    );
    let progress = world
        .research_progress(1, &database, upgrade_one)
        .unwrap()
        .unwrap();
    assert!(!progress.queued);
    assert_close(progress.current_points, 0.0);

    simulation.tick_with_world_and_database(&mut world, &database);
    simulation.tick_with_world_and_database(&mut world, &database);
    assert_eq!(
        world.technology_status(1, &database, upgrade_one).unwrap(),
        TechStatus::Active
    );
    assert_eq!(
        world.technology_status(1, &database, upgrade_two).unwrap(),
        TechStatus::Available
    );
    assert!(
        world
            .get_player(1)
            .unwrap()
            .technologies
            .is_active(UPGRADE_ONE)
    );
    assert_close(
        world.get_building(first_barracks).unwrap().max_hitpoints,
        150.0,
    );

    enqueue_research(&mut simulation, first_barracks, upgrade_two);
    simulation.tick_with_world_and_database(&mut world, &database);
    simulation.tick_with_world_and_database(&mut world, &database);
    assert_resources(&world, [400.0, 7.0, 0.0, 0.0]);
    let progress = world
        .research_progress(1, &database, upgrade_two)
        .unwrap()
        .unwrap();
    assert!(progress.current_points > 0.0);

    let cancel = BuildingCommand::research(1, vec![second_barracks], upgrade_two, -1);
    simulation
        .command_queue
        .enqueue_building(cancel, simulation.game_time_ms + MS_PER_TICK, 1);
    simulation.tick_with_world_and_database(&mut world, &database);
    assert_resources(&world, [800.0, 9.0, 0.0, 0.0]);
    assert_eq!(
        world.technology_status(1, &database, upgrade_two).unwrap(),
        TechStatus::Available
    );
    assert!(
        world
            .research_progress(1, &database, upgrade_two)
            .unwrap()
            .is_none()
    );

    assert_eq!(
        world
            .queue_research(1, first_barracks, &database, upgrade_two)
            .unwrap(),
        ResearchQueueResult::Queued
    );
    assert!(matches!(
        world.queue_research(1, second_barracks, &database, upgrade_two),
        Err(ResearchError::TechnologyUnavailable {
            status: TechStatus::Researching,
            ..
        })
    ));
    let _removed = world.remove_unit(first_barracks).unwrap();
    assert_resources(&world, [800.0, 9.0, 0.0, 0.0]);
    assert_eq!(
        world.technology_status(1, &database, upgrade_two).unwrap(),
        TechStatus::Available
    );
}

#[test]
fn instant_and_authored_prerequisite_semantics_are_authoritative() {
    let database = research_database();
    let mut world = sim::World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().resources.amounts = [100.0, 5.0, 0.0, 0.0];
    let barracks = spawn_barracks(&mut world, &database);
    let instant = technology_prototype_id(&database, INSTANT_UPGRADE).unwrap();
    let building_prereq = technology_prototype_id(&database, BUILDING_PREREQ).unwrap();
    let or_prereq = technology_prototype_id(&database, OR_PREREQ).unwrap();

    assert_eq!(
        world
            .technology_status(1, &database, building_prereq)
            .unwrap(),
        TechStatus::Available
    );
    assert_eq!(
        world.technology_status(1, &database, or_prereq).unwrap(),
        TechStatus::Available
    );
    assert_eq!(
        world
            .queue_research(1, barracks, &database, instant)
            .unwrap(),
        ResearchQueueResult::CompletedInstantly
    );
    assert_eq!(
        world.technology_status(1, &database, instant).unwrap(),
        TechStatus::Active
    );
    assert!(
        world
            .research_progress(1, &database, instant)
            .unwrap()
            .is_none()
    );
    assert_resources(&world, [50.0, 5.0, 0.0, 0.0]);
}

#[test]
fn player_technology_forbids_gate_new_research() {
    let database = research_database();
    let mut world = sim::World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().resources.amounts = [1_000.0, 10.0, 0.0, 0.0];
    let barracks_id = spawn_barracks(&mut world, &database);
    let technology_id = technology_prototype_id(&database, UPGRADE_ONE).unwrap();

    assert_eq!(
        world
            .get_player_mut(1)
            .unwrap()
            .set_technology_forbidden(&database, technology_id, true),
        Some(true)
    );
    assert_eq!(
        world
            .technology_status(1, &database, technology_id)
            .unwrap(),
        TechStatus::Obtainable
    );
    assert!(matches!(
        world.queue_research(1, barracks_id, &database, technology_id),
        Err(ResearchError::TechnologyUnavailable {
            status: TechStatus::Obtainable,
            ..
        })
    ));

    world
        .get_player_mut(1)
        .unwrap()
        .set_technology_forbidden(&database, technology_id, false);
    assert_eq!(
        world
            .queue_research(1, barracks_id, &database, technology_id)
            .unwrap(),
        ResearchQueueResult::Queued
    );
}

fn assert_resources(world: &sim::World, expected: [f32; 4]) {
    let actual = world.get_player(1).unwrap().resources.amounts;
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_close(actual, expected);
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0e-6,
        "{actual} != {expected}"
    );
}

fn enqueue_research(simulation: &mut Simulation, building_id: EntityId, technology_id: i32) {
    let exec_time = simulation.game_time_ms + MS_PER_TICK;
    simulation.command_queue.enqueue_building(
        BuildingCommand::research(1, vec![building_id], technology_id, 1),
        exec_time,
        1,
    );
}

fn spawn_barracks(world: &mut sim::World, database: &Database) -> EntityId {
    let prototype_id = object_prototype_id(database, BARRACKS).unwrap();
    spawn_object_at(world, database, 1, prototype_id, Vec3::ZERO, Vec3::Z).unwrap()
}

fn research_database() -> Database {
    let technologies = vec![
        technology(
            UPGRADE_ONE,
            0.1,
            vec![cost("Supplies", 200.0), cost("Power", 1.0)],
            Vec::new(),
            None,
            Some(hitpoint_effect(50.0)),
        ),
        technology(
            UPGRADE_TWO,
            0.2,
            vec![cost("Supplies", 400.0), cost("Power", 2.0)],
            Vec::new(),
            Some(active_prerequisite(UPGRADE_ONE)),
            None,
        ),
        technology(
            INSTANT_UPGRADE,
            99.0,
            vec![cost("Supplies", 50.0)],
            vec!["Instant".to_owned()],
            None,
            None,
        ),
        technology(
            BUILDING_PREREQ,
            1.0,
            Vec::new(),
            Vec::new(),
            Some(building_count_prerequisite()),
            None,
        ),
        technology(
            OR_PREREQ,
            1.0,
            Vec::new(),
            vec!["OrPrereqs".to_owned()],
            Some(or_prerequisites()),
            None,
        ),
    ];
    let commands = technologies
        .iter()
        .map(|technology| ObjectCommand {
            target: technology.name.clone(),
            command_type: None,
            ..ObjectCommand::default()
        })
        .collect();
    Database {
        objects: vec![ProtoObject {
            name: BARRACKS.to_owned(),
            object_class: Some("Building".to_owned()),
            hitpoints: Some(100.0),
            commands,
            ..ProtoObject::default()
        }],
        techs: technologies,
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![
                    ResourceDef {
                        name: "Supplies".to_owned(),
                        deductable: Some(true),
                    },
                    ResourceDef {
                        name: "Power".to_owned(),
                        deductable: Some(false),
                    },
                ],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn technology(
    name: &str,
    research_points: f32,
    costs: Vec<TechCost>,
    flags: Vec<String>,
    prereqs: Option<PrereqsWrapper>,
    effect: Option<TechEffect>,
) -> Tech {
    Tech {
        name: name.to_owned(),
        research_points: Some(research_points),
        status: Some("OBTAINABLE".to_owned()),
        flags,
        effects: effect.map(|effect| EffectsWrapper {
            entries: vec![effect],
        }),
        prereqs,
        costs,
        ..Tech::default()
    }
}

fn cost(resource_type: &str, amount: f32) -> TechCost {
    TechCost {
        resource_type: resource_type.to_owned(),
        amount,
    }
}

fn active_prerequisite(name: &str) -> PrereqsWrapper {
    PrereqsWrapper {
        entries: vec![TechStatusEntry {
            status: "Active".to_owned(),
            text: Some(name.to_owned()),
            ..TechStatusEntry::default()
        }],
        ..PrereqsWrapper::default()
    }
}

fn building_count_prerequisite() -> PrereqsWrapper {
    PrereqsWrapper {
        type_counts: vec![TypeCountEntry {
            unit: BARRACKS.to_owned(),
            operator: Some("gt".to_owned()),
            count: Some(0),
        }],
        ..PrereqsWrapper::default()
    }
}

fn or_prerequisites() -> PrereqsWrapper {
    PrereqsWrapper {
        entries: active_prerequisite("not_active").entries,
        type_counts: building_count_prerequisite().type_counts,
    }
}

fn hitpoint_effect(amount: f32) -> TechEffect {
    TechEffect {
        effect_type: "Data".to_owned(),
        amount: Some(amount),
        subtype: Some("Hitpoints".to_owned()),
        relativity: Some("Absolute".to_owned()),
        target: Some(EffectTarget {
            target_type: Some("ProtoUnit".to_owned()),
            value: Some(BARRACKS.to_owned()),
        }),
        ..TechEffect::default()
    }
}
