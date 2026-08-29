use super::*;
use crate::command::Command;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    PowerCommand, PowerCommandType, PowerInputCommand, PowerInputCommandType, PowerUserId,
    power_command_flags,
};
use crate::executor::CommandExecutor;
use crate::player::PowerGrant;
use crate::world::{DisruptionPowerInvocation, GeneralEventType, NativePowerError};
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::PopulationAmount;
use pipeline::database::hw1::powers::{
    DataEntry, DataLevel, PowerAttributes, PowerCost, PowerPopulation,
};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Database, GameData, Power, ProtoObject, Squad as ProtoSquad, Tech};

const POWER_ID: i32 = 0;
const ODST_TECH: &str = "OdstTech";

#[test]
fn paid_drop_stays_hidden_until_strict_delay_then_finishes_shutdown() {
    let database = database();
    let mut world = paid_world(&database, 4.0, 2);
    let used_power = world.subscribe_general_event(GeneralEventType::UsedPower, Some(1), false);
    let execution_id = world
        .invoke_odst_power(&database, invocation(false, PowerUserId::new(1, 9, 41)))
        .unwrap();
    assert_player_payment_state(&world, 500.0, 10.0, 2);

    let target = Vec3::new(12.0, 3.0, 34.0);
    assert!(world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(target),
        false,
    ));
    assert_player_payment_state(&world, 400.0, 7.0, 1);
    assert!(world.general_event_fired(used_power));
    let drop = world.active_odst_powers()[0].active_drops()[0].clone();
    assert_eq!(drop.target_location(), target);
    assert_eq!(drop.projectile_id(), EntityId::INVALID);
    assert_hidden_squad(&world, drop.squad_id(), target);
    assert!(world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Shutdown,
        false,
    ));

    super::update(&mut world, 0.75, &database, None);
    let execution = &world.active_odst_powers()[0];
    assert!(execution.ready_for_shutdown());
    assert_eq!(execution.active_drops().len(), 1);
    let projectile_id = execution.active_drops()[0].projectile_id();
    let projectile = world.get_projectile(projectile_id).unwrap();
    assert_eq!(projectile.proto_object_name, "odst_drop_pod");
    assert_eq!(projectile.base.position, target + DROP_POD_OFFSET);
    assert_eq!(projectile.target_position, target);
    assert_hidden_squad(&world, drop.squad_id(), target);

    super::update(&mut world, 0.001, &database, None);
    assert!(world.active_odst_powers().is_empty());
    assert_revealed_squad(&world, drop.squad_id());
}

#[test]
fn confirm_rechecks_resources_actual_squad_population_and_legacy_pop_is_ignored() {
    let database = database();
    let mut world = test_world(&database, 2.0);
    assert!(world.grant_player_power(1, &database, power_grant(1)));
    let user_id = PowerUserId::new(1, 9, 7);
    assert_eq!(
        world.invoke_odst_power(&database, invocation(false, user_id)),
        Err(NativePowerError::MissingTechnology(ODST_TECH.to_owned()))
    );
    world.activate_technology(1, &database, ODST_TECH).unwrap();
    let execution_id = world
        .invoke_odst_power(&database, invocation(false, user_id))
        .unwrap();
    let target = Vec3::new(5.0, 0.0, 8.0);

    assert!(!world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(target),
        false,
    ));
    assert!(world.active_odst_powers()[0].active_drops().is_empty());
    set_resources(&mut world, 100.0, 3.0);
    assert!(
        world
            .get_player_mut(1)
            .unwrap()
            .set_population_limits(0, 1.0, 1.0)
    );
    assert!(!world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(target),
        false,
    ));
    assert_player_payment_state(&world, 100.0, 3.0, 1);

    assert!(
        world
            .get_player_mut(1)
            .unwrap()
            .set_population_limits(0, 2.0, 2.0)
    );
    assert!(world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(target),
        false,
    ));
    assert_player_payment_state(&world, 0.0, 0.0, 0);
    assert_close(
        world
            .get_player(1)
            .unwrap()
            .get_population(0)
            .unwrap()
            .count,
        2.0,
    );
}

#[test]
fn invoke_power_two_and_confirm_wire_vector_route_by_packed_user_id() {
    let database = database();
    let mut world = test_world(&database, 10.0);
    let executor = CommandExecutor::with_database(&database);
    let user_id = PowerUserId::new(1, 9, 19);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::Power(invoke_command(user_id, true)),
    );
    assert_eq!(world.active_odst_powers().len(), 1);
    assert_eq!(world.active_odst_powers()[0].power_user_id(), user_id);

    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(confirm_command(PowerUserId::new(1, 9, 20), Vec3::X)),
    );
    assert!(world.active_odst_powers()[0].active_drops().is_empty());
    let target = Vec3::new(17.0, 4.0, 29.0);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(confirm_command(user_id, target)),
    );

    let drop = &world.active_odst_powers()[0].active_drops()[0];
    assert_eq!(drop.target_location(), target);
    assert_eq!(
        world.get_squad(drop.squad_id()).unwrap().base.position,
        target
    );
}

#[test]
fn no_cost_bypasses_menu_requirements_but_profile_and_user_id_remain_strict() {
    let database = database();
    let mut world = test_world(&database, 0.0);
    let user_id = PowerUserId::new(1, 9, 3);
    let execution_id = world
        .invoke_odst_power(&database, invocation(true, user_id))
        .unwrap();
    assert!(world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert!(world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::X),
        false,
    ));
    assert_eq!(world.active_odst_powers()[0].active_drops().len(), 2);
    assert!(world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Shutdown,
        false,
    ));
    assert!(!world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::Z),
        true,
    ));

    assert_eq!(
        world.invoke_odst_power(&database, invocation(true, PowerUserId::new(1, 5, 3))),
        Err(NativePowerError::InvalidData("PowerUserID"))
    );
    let mut malformed = database;
    malformed.powers[0].attributes.as_mut().unwrap().data_levels[0]
        .entries
        .retain(|entry| entry.name != "Projectile");
    assert_eq!(
        world.invoke_odst_power(&malformed, invocation(true, PowerUserId::INVALID)),
        Err(NativePowerError::MissingData("Projectile"))
    );
}

#[test]
fn disruption_rejects_confirm_even_when_both_no_cost_paths_are_set() {
    let database = database();
    let mut world = test_world(&database, 0.0);
    let execution_id = world
        .invoke_odst_power(&database, invocation(true, PowerUserId::INVALID))
        .unwrap();
    world
        .invoke_disruption_power(
            &database,
            DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: 1,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .unwrap();
    for _ in 0..10 {
        world.update_entities_with_database(0.05, &database);
        if world.active_disruption_powers()[0].is_active() {
            break;
        }
    }
    assert!(world.active_disruption_powers()[0].is_active());
    assert!(!world.submit_odst_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        true,
    ));
    assert!(world.active_odst_powers()[0].active_drops().is_empty());
}

fn assert_hidden_squad(world: &World, squad_id: EntityId, target: Vec3) {
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.proto_squad_name, ODST_SQUAD);
    assert_eq!(squad.base.position, target);
    assert_eq!(world.entity_is_selectable(squad_id), Some(false));
    assert_eq!(squad.unit_ids.len(), 2);
    for unit_id in &squad.unit_ids {
        assert_eq!(world.entity_is_render_enabled(*unit_id), Some(false));
        assert!(world.get_unit(*unit_id).unwrap().is_invulnerable());
    }
}

fn assert_revealed_squad(world: &World, squad_id: EntityId) {
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(world.entity_is_selectable(squad_id), Some(true));
    for unit_id in &squad.unit_ids {
        assert_eq!(world.entity_is_render_enabled(*unit_id), Some(true));
        assert!(!world.get_unit(*unit_id).unwrap().is_invulnerable());
    }
}

fn assert_player_payment_state(world: &World, supplies: f32, power: f32, uses: i32) {
    let player = world.get_player(1).unwrap();
    assert_close(player.get_resource(0), supplies);
    assert_close(player.get_resource(1), power);
    assert_eq!(
        player
            .power_entry(POWER_ID)
            .unwrap()
            .finite_uses_remaining(),
        uses
    );
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}

fn paid_world(database: &Database, population_cap: f32, uses: i32) -> World {
    let mut world = test_world(database, population_cap);
    set_resources(&mut world, 500.0, 10.0);
    world.activate_technology(1, database, ODST_TECH).unwrap();
    assert!(world.grant_player_power(1, database, power_grant(uses)));
    world
}

fn test_world(database: &Database, population_cap: f32) -> World {
    let mut world = World::with_seed(29);
    world.init_players(1);
    world.configure_prototype_catalogs(database);
    let player = world.get_player_mut(1).unwrap();
    player.configure_population_slots(1);
    assert!(player.set_population_limits(0, population_cap, population_cap));
    world
}

fn set_resources(world: &mut World, supplies: f32, power: f32) {
    let player = world.get_player_mut(1).unwrap();
    player.set_resource(0, supplies);
    player.set_resource(1, power);
}

fn power_grant(uses: i32) -> PowerGrant {
    PowerGrant {
        proto_power_id: POWER_ID,
        squad_id: EntityId::INVALID,
        uses,
        icon_location: -1,
        ignore_cost: false,
        ignore_tech_prerequisites: false,
        ignore_population: false,
    }
}

fn invocation(ignore_requirements: bool, power_user_id: PowerUserId) -> OdstPowerInvocation {
    OdstPowerInvocation {
        player_id: 1,
        proto_power_id: POWER_ID,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
        ignore_requirements,
        power_user_id,
    }
}

fn execute_command(executor: &CommandExecutor<'_>, world: &mut World, command: QueuedCommand) {
    executor.execute(
        world,
        &CommandEntry {
            command,
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn invoke_command(user_id: PowerUserId, no_cost: bool) -> PowerCommand {
    let mut base = Command {
        player_id: 1,
        ..Command::default()
    };
    base.set_flag(power_command_flags::NO_COST, no_cost);
    PowerCommand {
        base,
        power_type: PowerCommandType::InvokePower2,
        proto_power_id: POWER_ID,
        power_level: 0,
        target_location: Vec3::ZERO.extend(0.0),
        squad_id: EntityId::INVALID,
        power_user_id: user_id.raw().cast_signed(),
        ..PowerCommand::default()
    }
}

fn confirm_command(user_id: PowerUserId, target: Vec3) -> PowerInputCommand {
    PowerInputCommand {
        base: Command {
            player_id: 1,
            ..Command::default()
        },
        input_type: PowerInputCommandType::Confirm,
        vector: target.extend(0.0),
        power_user_id: user_id,
    }
}

fn database() -> Database {
    Database {
        objects: vec![
            drop_pod(),
            odst_member(),
            visual("disruption_field"),
            visual("disruption_pulse"),
            visual("disruption_strike"),
            visual("disruption_bomber"),
        ],
        squads: vec![odst_squad()],
        powers: vec![odst_power(), disruption_power()],
        techs: vec![Tech {
            name: ODST_TECH.to_owned(),
            ..Tech::default()
        }],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![resource("Supplies"), resource("Power")],
            }),
            pops: Some(PopsWrapper {
                entries: vec!["Unit".to_owned()],
            }),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn drop_pod() -> ProtoObject {
    ProtoObject {
        name: "odst_drop_pod".to_owned(),
        dbid: Some(10),
        object_class: Some("Projectile".to_owned()),
        flags: vec!["IsAffectedByGravity".to_owned()],
        velocity: Some(125.0),
        max_projectile_height: Some(35.0),
        lifespan: Some(2.0),
        ..ProtoObject::default()
    }
}

fn odst_member() -> ProtoObject {
    ProtoObject {
        name: "odst_member".to_owned(),
        dbid: Some(20),
        object_class: Some("Unit".to_owned()),
        hitpoints: Some(100.0),
        population: vec![PopulationAmount {
            population_type: Some("Unit".to_owned()),
            amount: 1.0,
        }],
        ..ProtoObject::default()
    }
}

fn odst_squad() -> ProtoSquad {
    ProtoSquad {
        name: ODST_SQUAD.to_owned(),
        dbid: Some(30),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: "odst_member".to_owned(),
                count: 2,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn odst_power() -> Power {
    Power {
        name: "TestOdstDrop".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("ODST".to_owned()),
            auto_recharge: Some(10_000),
            cost: Some(PowerCost {
                supplies: Some(100.0),
                power: Some(3.0),
                ..PowerCost::default()
            }),
            tech_prerequisites: vec![ODST_TECH.to_owned()],
            population: vec![PowerPopulation {
                population_type: None,
                amount: 99.0,
            }],
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("float", "SquadSpawnDelay", "0.75"),
                    data("protoobject", "Projectile", "odst_drop_pod"),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn disruption_power() -> Power {
    Power {
        name: "TestDisruption".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Disruption".to_owned()),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("protoobject", "DisruptionObject", "disruption_field"),
                    data("protoobject", "PulseObject", "disruption_pulse"),
                    data("protoobject", "StrikeObject", "disruption_strike"),
                    data("sound", "PulseSound", "pulse"),
                    data("float", "PulseSpacing", "1"),
                    data("float", "DisruptionRadius", "10"),
                    data("float", "DisruptionTimeSec", "5"),
                    data("float", "DisruptionStartTime", "0.1"),
                    data("protoobject", "Bomber", "disruption_bomber"),
                    data("float", "BomberBombTime", "0.05"),
                    data("float", "BomberFlyinDistance", "10"),
                    data("float", "BomberFlyinHeight", "3"),
                    data("float", "BomberBombHeight", "1"),
                    data("float", "BomberSpeed", "5"),
                    data("float", "BomberFlyOutTime", "1"),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn visual(name: &str) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        ..ProtoObject::default()
    }
}

fn resource(name: &str) -> ResourceDef {
    ResourceDef {
        name: name.to_owned(),
        ..ResourceDef::default()
    }
}

fn data(data_type: &str, name: &str, value: &str) -> DataEntry {
    DataEntry {
        data_type: data_type.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
    }
}
