use super::*;
use crate::command::Command;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    PowerCommand, PowerCommandType, PowerInputCommand, PowerInputCommandType, power_command_flags,
};
use crate::executor::CommandExecutor;
use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
use crate::player::PowerGrant;
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, GameData, Power, ProtoObject};

const POWER_ID: i32 = 0;
const POWER_TYPE: u32 = 2;

#[test]
fn paid_cast_defers_payment_moves_beam_restarts_recharge_and_throws_debris() {
    let database = database();
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("orbital_source".to_owned(), tactics())]);
    let mut world = paid_world(&database);
    let target_id = add_physical_target(&mut world, Vec3::ZERO);
    let used_power = world.subscribe_general_event(GeneralEventType::UsedPower, Some(1), false);
    let execution_id = world
        .invoke_orbital_power(
            &database,
            invocation(false, PowerUserId::new(1, POWER_TYPE, 17)),
        )
        .unwrap();
    let execution = &world.active_orbital_powers()[0];
    let beam_id = execution.real_targeting_laser_id();
    assert_eq!(execution.id(), execution_id);
    assert_eq!(execution.shots_remaining(), 2);
    assert!(execution.requires_los());
    assert_eq!(
        world.get_object(beam_id).unwrap().proto_object_name,
        "target_beam"
    );
    assert_payment_state(&world, 1_000.0, 1);
    assert!(!world.general_event_fired(used_power));

    assert!(world.submit_orbital_power_input(
        &database,
        execution_id,
        NativePowerInput::Position(Vec3::X * 10.0),
    ));
    advance(&mut world, &database, &gameplay, 1);
    assert_close(world.get_object(beam_id).unwrap().base.position.x, 1.0);
    assert!(world.submit_orbital_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
    ));
    assert_payment_state(&world, 700.0, 0);
    assert!(world.general_event_fired(used_power));
    let first_recharge = power_item(&world).next_grant_time();
    assert_eq!(first_recharge, 1_100);

    advance(&mut world, &database, &gameplay, 1);
    let first_laser = world.active_orbital_powers()[0].pending_shots()[0].laser_object_id();
    assert_eq!(
        world.get_object(first_laser).unwrap().proto_object_name,
        "target_beam"
    );
    advance(&mut world, &database, &gameplay, 1);
    assert!(world.submit_orbital_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::X * 2.0),
    ));
    let execution = &world.active_orbital_powers()[0];
    assert_eq!(execution.shots_remaining(), 0);
    assert!(execution.real_targeting_laser_id().is_invalid());
    assert!(world.get_object(beam_id).is_none());
    assert_eq!(power_item(&world).next_grant_time(), 1_300);
    assert!(power_item(&world).next_grant_time() > first_recharge);

    let initial_health = world.get_unit(target_id).unwrap().hitpoints;
    advance_until(&mut world, &database, &gameplay, 12);
    assert!(world.get_unit(target_id).unwrap().hitpoints < initial_health);
    assert_eq!(count_objects(&world, "orbital_effect"), 2);
    let debris_ids = orbital_debris_ids(&world);
    assert!((26..=46).contains(&debris_ids.len()));
    assert!(debris_ids.iter().all(|id| {
        world
            .get_object(*id)
            .unwrap()
            .base
            .velocity
            .length_squared()
            > 0.0
    }));
    let debris_id = debris_ids[0];
    let old_position = world.get_object(debris_id).unwrap().base.position;
    advance(&mut world, &database, &gameplay, 1);
    assert_ne!(
        world.get_object(debris_id).unwrap().base.position,
        old_position
    );
}

#[test]
fn invoke_power_two_routes_confirm_position_and_shutdown_by_retail_user_id() {
    let database = database();
    let mut world = test_world(&database);
    let executor = CommandExecutor::with_database(&database);
    let user_id = PowerUserId::new(1, POWER_TYPE, 91);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::Power(invoke_command(user_id)),
    );
    let beam_id = world.active_orbital_powers()[0].real_targeting_laser_id();

    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            PowerUserId::new(1, POWER_TYPE, 92),
            PowerInputCommandType::Confirm,
            Vec3::ZERO,
        )),
    );
    assert_eq!(world.active_orbital_powers()[0].shots_remaining(), 2);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Position,
            Vec3::X * 4.0,
        )),
    );
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Confirm,
            Vec3::ZERO,
        )),
    );
    let execution = &world.active_orbital_powers()[0];
    assert_eq!(execution.desired_targeting_position(), Vec3::ZERO);
    assert_eq!(execution.pending_shots().len(), 1);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Shutdown,
            Vec3::ZERO,
        )),
    );
    let execution = &world.active_orbital_powers()[0];
    assert_eq!(execution.shots_remaining(), 0);
    assert_eq!(execution.pending_shots().len(), 1);
    assert!(world.get_object(beam_id).is_none());
    for _ in 0..3 {
        world.game_time_ms = world.game_time_ms.wrapping_add(100);
        world.update_entities_with_database(0.1, &database);
    }
    assert!(world.active_orbital_powers().is_empty());
}

#[test]
fn profile_and_packed_type_validation_match_the_native_contract() {
    let base_database = database();
    let mut world = test_world(&base_database);
    assert_eq!(
        world.invoke_orbital_power(&base_database, invocation(true, PowerUserId::new(1, 3, 4)),),
        Err(NativePowerError::InvalidData("PowerUserID"))
    );

    let mut zero_shots = database();
    set_level_value(&mut zero_shots, "NumShots", "0");
    assert_eq!(
        world.invoke_orbital_power(&zero_shots, invocation(true, PowerUserId::INVALID)),
        Err(NativePowerError::InvalidData("NumShots"))
    );

    let mut missing_rock = database();
    set_base_value(&mut missing_rock, "RockLarge", "missing_rock");
    assert_eq!(
        world.invoke_orbital_power(&missing_rock, invocation(true, PowerUserId::INVALID)),
        Err(NativePowerError::UnknownPrototype(
            "missing_rock".to_owned()
        ))
    );
}

fn advance(world: &mut World, database: &Database, gameplay: &GameplayCatalog, ticks: usize) {
    for _ in 0..ticks {
        world.game_time_ms = world.game_time_ms.wrapping_add(100);
        world.update_entities_with_database_and_gameplay(0.1, database, gameplay);
    }
}

fn advance_until(world: &mut World, database: &Database, gameplay: &GameplayCatalog, limit: usize) {
    for _ in 0..limit {
        if world.active_orbital_powers().is_empty() {
            return;
        }
        advance(world, database, gameplay, 1);
    }
    assert!(
        world.active_orbital_powers().is_empty(),
        "Orbital execution did not finish"
    );
}

fn paid_world(database: &Database) -> World {
    let mut world = test_world(database);
    world.get_player_mut(1).unwrap().set_resource(0, 1_000.0);
    assert!(world.grant_player_power(
        1,
        database,
        PowerGrant {
            proto_power_id: POWER_ID,
            squad_id: EntityId::INVALID,
            uses: 1,
            icon_location: -1,
            ignore_cost: false,
            ignore_tech_prerequisites: false,
            ignore_population: false,
        },
    ));
    world
}

fn test_world(database: &Database) -> World {
    let mut world = World::with_seed(73);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.configure_prototype_catalogs(database);
    world
}

fn add_physical_target(world: &mut World, position: Vec3) -> EntityId {
    let unit_id = world.create_unit_at(2, position);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.proto_object_id = 8;
    unit.proto_object_name = "target".to_owned();
    unit.set_max_hitpoints(5_000.0);
    unit.physics = Some(PhysicsBody::ground_vehicle(
        PhysicsMaterial {
            mass: 10.0,
            ..PhysicsMaterial::default()
        },
        BoxCollider::new(Vec3::splat(1.0), Vec3::Y),
        0.0,
        20.0,
        10.0,
        360.0,
    ));
    unit_id
}

fn power_item(world: &World) -> &crate::player::PowerEntryItem {
    &world
        .get_player(1)
        .unwrap()
        .power_entry(POWER_ID)
        .unwrap()
        .items()[0]
}

fn assert_payment_state(world: &World, supplies: f32, uses: i32) {
    assert_close(world.get_player(1).unwrap().get_resource(0), supplies);
    assert_eq!(
        world
            .get_player(1)
            .unwrap()
            .power_entry(POWER_ID)
            .unwrap()
            .finite_uses_remaining(),
        uses
    );
}

fn orbital_debris_ids(world: &World) -> Vec<EntityId> {
    world
        .objects
        .iter()
        .filter(|(_, object)| object.proto_object_name.starts_with("rock_"))
        .map(|(id, _)| id)
        .collect()
}

fn count_objects(world: &World, prototype: &str) -> usize {
    world
        .objects
        .iter()
        .filter(|(_, object)| object.proto_object_name == prototype)
        .count()
}

fn invocation(ignore_requirements: bool, power_user_id: PowerUserId) -> OrbitalPowerInvocation {
    OrbitalPowerInvocation {
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

fn invoke_command(user_id: PowerUserId) -> PowerCommand {
    let mut base = Command {
        player_id: 1,
        ..Command::default()
    };
    base.set_flag(power_command_flags::NO_COST, true);
    PowerCommand {
        base,
        power_type: PowerCommandType::InvokePower2,
        proto_power_id: POWER_ID,
        target_location: Vec3::ZERO.extend(0.0),
        squad_id: EntityId::INVALID,
        power_user_id: user_id.raw().cast_signed(),
        ..PowerCommand::default()
    }
}

fn input_command(
    user_id: PowerUserId,
    input_type: PowerInputCommandType,
    vector: Vec3,
) -> PowerInputCommand {
    PowerInputCommand {
        base: Command {
            player_id: 1,
            ..Command::default()
        },
        input_type,
        vector: vector.extend(0.0),
        power_user_id: user_id,
    }
}

fn database() -> Database {
    Database {
        objects: vec![
            projectile("orbital_source", 0, Some("orbital_source.tactics")),
            projectile("damage_projectile", 1, None),
            target_beam(),
            visual("orbital_effect", 3, Some(5.0)),
            visual("rock_small", 4, None),
            visual("rock_medium", 5, None),
            visual("rock_large", 6, None),
            ProtoObject {
                name: "sys_revealer".to_owned(),
                dbid: Some(7),
                los: Some(1.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "target".to_owned(),
                dbid: Some(8),
                object_class: Some("Unit".to_owned()),
                hitpoints: Some(5_000.0),
                obstruction_radius_x: Some(1.0),
                obstruction_radius_y: Some(1.0),
                obstruction_radius_z: Some(1.0),
                ..ProtoObject::default()
            },
        ],
        powers: vec![orbital_power()],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    ..ResourceDef::default()
                }],
            }),
            pops: Some(PopsWrapper::default()),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn projectile(name: &str, dbid: i32, tactics: Option<&str>) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Projectile".to_owned()),
        tactics: tactics.map(str::to_owned),
        velocity: Some(50.0),
        lifespan: Some(2.0),
        ..ProtoObject::default()
    }
}

fn target_beam() -> ProtoObject {
    ProtoObject {
        name: "target_beam".to_owned(),
        dbid: Some(2),
        max_velocity: Some(10.0),
        ..ProtoObject::default()
    }
}

fn visual(name: &str, dbid: i32, lifespan: Option<f32>) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        lifespan,
        ..ProtoObject::default()
    }
}

fn orbital_power() -> Power {
    Power {
        name: "TestOrbital".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Orbital".to_owned()),
            auto_recharge: Some(1_000),
            cost: Some(PowerCost {
                supplies: Some(300.0),
                ..PowerCost::default()
            }),
            base_data_level: Some(DataLevel {
                entries: vec![
                    data("protoobject", "TargetBeam", "target_beam"),
                    data("protoobject", "Projectile", "orbital_source"),
                    data("protoobject", "Effect", "orbital_effect"),
                    data("protoobject", "RockSmall", "rock_small"),
                    data("protoobject", "RockMedium", "rock_medium"),
                    data("protoobject", "RockLarge", "rock_large"),
                    float_data("TargetingDelay", 0.2),
                    float_data("AutoShotDelay", 0.5),
                    float_data("AutoShotInnerRadius", 1.0),
                    float_data("AutoShotOuterRadius", 3.0),
                    float_data("XOffset", 0.0),
                    float_data("YOffset", 5.0),
                    float_data("ZOffset", 0.0),
                    data("bool", "RequiresLOS", "true"),
                ],
                ..DataLevel::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![int_data("NumShots", 2)],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn tactics() -> TacticData {
    TacticData {
        weapons: vec![Weapon {
            name: "OrbitalAttack".to_owned(),
            damage_per_second: Some(400.0),
            weapon_type: Some("LeaderPower".to_owned()),
            projectile: Some("damage_projectile".to_owned()),
            aoe_radius: Some(10.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "OrbitalAttackAction".to_owned(),
            weapon: Some("OrbitalAttack".to_owned()),
            default: Some(true),
            ..Action::default()
        }],
        ..TacticData::default()
    }
}

fn set_level_value(database: &mut Database, name: &str, value: &str) {
    let entry = database.powers[0].attributes.as_mut().unwrap().data_levels[0]
        .entries
        .iter_mut()
        .find(|entry| entry.name == name)
        .unwrap();
    entry.value = value.to_owned();
}

fn set_base_value(database: &mut Database, name: &str, value: &str) {
    let entry = database.powers[0]
        .attributes
        .as_mut()
        .unwrap()
        .base_data_level
        .as_mut()
        .unwrap()
        .entries
        .iter_mut()
        .find(|entry| entry.name == name)
        .unwrap();
    entry.value = value.to_owned();
}

fn int_data(name: &str, value: i32) -> DataEntry {
    data("int", name, &value.to_string())
}

fn float_data(name: &str, value: f32) -> DataEntry {
    data("float", name, &value.to_string())
}

fn data(data_type: &str, name: &str, value: &str) -> DataEntry {
    DataEntry {
        data_type: data_type.to_owned(),
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}
