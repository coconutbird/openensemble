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
const UNIQUE_BASE: &str = "test_command_01";
const UNIQUE_COMMAND_TWO: &str = "test_command_02";
const UNIQUE_COMMAND_THREE: &str = "test_command_03";
const UNIQUE_UPGRADE_ONE: &str = "test_base_upgrade1";
const UNIQUE_UPGRADE_TWO: &str = "test_base_upgrade2";
const UNIQUE_ALTERNATE: &str = "test_base_alternate";
const COOP_LAB: &str = "test_coop_lab";
const COOP_REACTOR: &str = "test_coop_reactor";
const COOP_MARINE: &str = "test_coop_marine";
const COOP_BUILDING_TECH: &str = "test_coop_building_prereq";
const COOP_UNIT_TECH: &str = "test_coop_unit_prereq";

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

#[test]
fn unique_research_status_and_work_are_keyed_by_building() {
    let database = unique_research_database();
    let mut world = sim::World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().resources.amounts = [1_000.0, 0.0, 0.0, 0.0];
    let first = spawn_unique_base(&mut world, &database);
    let second = spawn_unique_base(&mut world, &database);
    let upgrade = technology_prototype_id(&database, UNIQUE_UPGRADE_ONE).unwrap();
    let alternate = technology_prototype_id(&database, UNIQUE_ALTERNATE).unwrap();

    assert_eq!(
        world.technology_status(1, &database, upgrade).unwrap(),
        TechStatus::Obtainable
    );
    assert_eq!(
        world
            .building_technology_status(1, first, &database, upgrade)
            .unwrap(),
        TechStatus::Available
    );
    assert_eq!(
        world.queue_research(1, first, &database, upgrade).unwrap(),
        ResearchQueueResult::Queued
    );
    assert_eq!(
        world.queue_research(1, second, &database, upgrade).unwrap(),
        ResearchQueueResult::Queued
    );
    assert!(matches!(
        world.queue_research(1, first, &database, alternate),
        Err(ResearchError::UniqueResearchInProgress(id)) if id == first
    ));
    assert_eq!(
        world
            .building_technology_status(1, first, &database, upgrade)
            .unwrap(),
        TechStatus::Researching
    );
    assert!(
        world
            .building_research_progress(1, first, &database, upgrade)
            .unwrap()
            .is_some()
    );
    assert!(
        world
            .research_progress(1, &database, upgrade)
            .unwrap()
            .is_none()
    );
    assert_resources(&world, [800.0, 0.0, 0.0, 0.0]);

    assert!(
        world
            .cancel_research(1, second, &database, upgrade)
            .unwrap()
    );
    assert_resources(&world, [900.0, 0.0, 0.0, 0.0]);
    assert_eq!(
        world
            .building_technology_status(1, second, &database, upgrade)
            .unwrap(),
        TechStatus::Available
    );
}

#[test]
fn unique_transform_changes_only_the_researching_instance_and_preserves_identity() {
    let database = unique_research_database();
    let mut world = sim::World::new();
    world.init_players(1);
    world.get_player_mut(1).unwrap().resources.amounts = [1_000.0, 0.0, 0.0, 0.0];
    let upgraded = spawn_unique_base(&mut world, &database);
    let untouched = spawn_unique_base(&mut world, &database);
    world.get_building_mut(upgraded).unwrap().hitpoints = 25.0;
    let upgrade_one = technology_prototype_id(&database, UNIQUE_UPGRADE_ONE).unwrap();
    let upgrade_two = technology_prototype_id(&database, UNIQUE_UPGRADE_TWO).unwrap();

    world
        .queue_research(1, upgraded, &database, upgrade_one)
        .unwrap();
    advance_research(&mut world, &database, 2.0);
    let building = world.get_building(upgraded).unwrap();
    assert_eq!(building.base.id, upgraded);
    assert_eq!(building.proto_object_name, UNIQUE_COMMAND_TWO);
    assert_eq!(building.logical_proto_object_name(), UNIQUE_COMMAND_TWO);
    assert_close(building.max_hitpoints, 200.0);
    assert_close(building.hitpoints, 50.0);
    assert!(building.unique_technology_is_active(upgrade_one));
    assert_eq!(
        world.get_building(untouched).unwrap().proto_object_name,
        UNIQUE_BASE
    );
    assert_eq!(
        world
            .building_technology_status(1, upgraded, &database, upgrade_two)
            .unwrap(),
        TechStatus::Available
    );

    world
        .queue_research(1, upgraded, &database, upgrade_two)
        .unwrap();
    advance_research(&mut world, &database, 2.0);
    let building = world.get_building(upgraded).unwrap();
    assert_eq!(building.proto_object_name, UNIQUE_COMMAND_THREE);
    assert!(building.unique_technology_is_active(upgrade_one));
    assert!(building.unique_technology_is_active(upgrade_two));
    assert_eq!(
        building.active_unique_technologies().collect::<Vec<_>>(),
        vec![upgrade_one, upgrade_two]
    );
}

#[test]
fn cooperative_partner_buildings_but_not_units_satisfy_research_type_counts() {
    let database = coop_prerequisite_database();
    let mut world = sim::World::new();
    world.init_players(2);
    let _lab = spawn_named(&mut world, &database, 1, COOP_LAB);
    let _reactor = spawn_named(&mut world, &database, 2, COOP_REACTOR);
    let _marine = spawn_named(&mut world, &database, 2, COOP_MARINE);
    let building_tech = technology_prototype_id(&database, COOP_BUILDING_TECH).unwrap();
    let unit_tech = technology_prototype_id(&database, COOP_UNIT_TECH).unwrap();

    assert_eq!(
        world
            .technology_status(1, &database, building_tech)
            .unwrap(),
        TechStatus::Obtainable
    );
    world.get_player_mut(1).unwrap().set_coop_player_id(Some(2));
    assert_eq!(
        world
            .technology_status(1, &database, building_tech)
            .unwrap(),
        TechStatus::Available
    );
    assert_eq!(
        world.technology_status(1, &database, unit_tech).unwrap(),
        TechStatus::Obtainable
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

fn spawn_unique_base(world: &mut sim::World, database: &Database) -> EntityId {
    let prototype_id = object_prototype_id(database, UNIQUE_BASE).unwrap();
    spawn_object_at(world, database, 1, prototype_id, Vec3::ZERO, Vec3::Z).unwrap()
}

fn spawn_named(world: &mut sim::World, database: &Database, player_id: u8, name: &str) -> EntityId {
    let prototype_id = object_prototype_id(database, name).unwrap();
    spawn_object_at(
        world,
        database,
        player_id,
        prototype_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .unwrap()
}

fn advance_research(world: &mut sim::World, database: &Database, points: f32) {
    let promoted = world.update_production(0.01, database);
    assert_eq!(promoted.completed_research, 0);
    let completed = world.update_production(points, database);
    assert_eq!(completed.completed_research, 1);
}

fn coop_prerequisite_database() -> Database {
    let technologies = vec![
        technology(
            COOP_BUILDING_TECH,
            1.0,
            Vec::new(),
            Vec::new(),
            Some(type_count_prerequisite(COOP_REACTOR)),
            None,
        ),
        technology(
            COOP_UNIT_TECH,
            1.0,
            Vec::new(),
            Vec::new(),
            Some(type_count_prerequisite(COOP_MARINE)),
            None,
        ),
    ];
    Database {
        objects: vec![
            ProtoObject {
                name: COOP_LAB.to_owned(),
                object_class: Some("Building".to_owned()),
                commands: technologies
                    .iter()
                    .map(|technology| ObjectCommand {
                        target: technology.name.clone(),
                        command_type: Some("Research".to_owned()),
                        ..ObjectCommand::default()
                    })
                    .collect(),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: COOP_REACTOR.to_owned(),
                object_class: Some("Building".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: COOP_MARINE.to_owned(),
                object_class: Some("Unit".to_owned()),
                ..ProtoObject::default()
            },
        ],
        techs: technologies,
        ..Database::default()
    }
}

fn type_count_prerequisite(unit: &str) -> PrereqsWrapper {
    PrereqsWrapper {
        type_counts: vec![TypeCountEntry {
            unit: unit.to_owned(),
            operator: Some("gt".to_owned()),
            count: Some(0),
        }],
        ..PrereqsWrapper::default()
    }
}

fn unique_research_database() -> Database {
    Database {
        objects: vec![
            unique_command_proto(
                UNIQUE_BASE,
                501,
                100.0,
                &[UNIQUE_UPGRADE_ONE, UNIQUE_ALTERNATE],
            ),
            unique_command_proto(UNIQUE_COMMAND_TWO, 502, 200.0, &[UNIQUE_UPGRADE_TWO]),
            unique_command_proto(UNIQUE_COMMAND_THREE, 503, 300.0, &[]),
        ],
        techs: vec![
            unique_technology(UNIQUE_UPGRADE_ONE, UNIQUE_COMMAND_TWO, 100.0, None),
            unique_technology(
                UNIQUE_UPGRADE_TWO,
                UNIQUE_COMMAND_THREE,
                150.0,
                Some(active_prerequisite(UNIQUE_UPGRADE_ONE)),
            ),
            unique_technology(UNIQUE_ALTERNATE, UNIQUE_COMMAND_TWO, 50.0, None),
        ],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    deductable: Some(true),
                }],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn unique_command_proto(name: &str, dbid: i32, hitpoints: f32, techs: &[&str]) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Building".to_owned()),
        hitpoints: Some(hitpoints),
        commands: techs
            .iter()
            .map(|technology| ObjectCommand {
                target: (*technology).to_owned(),
                command_type: Some("Research".to_owned()),
                ..ObjectCommand::default()
            })
            .collect(),
        ..ProtoObject::default()
    }
}

fn unique_technology(
    name: &str,
    target: &str,
    supplies: f32,
    prereqs: Option<PrereqsWrapper>,
) -> Tech {
    technology(
        name,
        2.0,
        vec![cost("Supplies", supplies)],
        vec!["UniqueProtoUnitInstance".to_owned()],
        prereqs,
        Some(TechEffect {
            effect_type: "TransformUnit".to_owned(),
            value: Some(target.to_owned()),
            ..TechEffect::default()
        }),
    )
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
