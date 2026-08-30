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
const POWER_TYPE: u32 = 3;

#[test]
fn paid_two_input_cast_drops_fused_clusters_that_damage_and_nudge_once() {
    let database = database();
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("carpet_source".to_owned(), carpet_tactics())]);
    let mut world = paid_world(&database);
    let target_id = add_physical_target(&mut world, Vec3::ZERO);
    let used_power = world.subscribe_general_event(GeneralEventType::UsedPower, Some(1), false);
    let execution_id = world
        .invoke_carpet_bombing_power(
            &database,
            invocation(false, PowerUserId::new(1, POWER_TYPE, 17)),
        )
        .unwrap();

    let execution = &world.active_carpet_bombing_powers()[0];
    assert_eq!(execution.id(), execution_id);
    assert_eq!(execution.phase(), CarpetBombingPhase::WaitingForInputs);
    assert!(execution.bomber_object_id().is_none());
    assert_payment_state(&world, 700.0, 0);
    assert!(world.general_event_fired(used_power));

    assert!(world.submit_carpet_bombing_power_input(
        &database,
        execution_id,
        NativePowerInput::Position(Vec3::ZERO),
    ));
    assert_eq!(
        world.active_carpet_bombing_powers()[0].phase(),
        CarpetBombingPhase::WaitingForInputs
    );
    assert!(world.submit_carpet_bombing_power_input(
        &database,
        execution_id,
        NativePowerInput::Direction(Vec3::X),
    ));
    let execution = &world.active_carpet_bombing_powers()[0];
    assert_eq!(execution.phase(), CarpetBombingPhase::Active);
    assert_eq!(execution.start_direction(), Some(Vec3::X));
    let bomber_id = execution.bomber_object_id().expect("sim-owned bomber");
    assert_eq!(
        world.get_object(bomber_id).unwrap().proto_object_name,
        "carpet_bomber"
    );

    advance(&mut world, &database, &gameplay, 3);
    let execution = &world.active_carpet_bombing_powers()[0];
    assert_eq!(execution.bomb_clusters_dropped(), 2);
    assert_eq!(execution.pending_bombs().len(), 4);
    assert_eq!(
        execution
            .pending_bombs()
            .iter()
            .filter(|bomb| world.get_object(bomb.impact_object_id()).is_some())
            .count(),
        4
    );

    advance(&mut world, &database, &gameplay, 2);
    assert_eq!(
        world.active_carpet_bombing_powers()[0]
            .active_projectile_ids()
            .len(),
        4
    );
    let initial_health = world.get_unit(target_id).unwrap().hitpoints;
    advance_until(&mut world, &database, &gameplay, 10, |world| {
        world.active_carpet_bombing_powers().is_empty()
    });
    let target = world
        .get_unit(target_id)
        .expect("target survives test damage");
    assert!(target.hitpoints < initial_health);
    assert!(target.base.velocity.length_squared() > 0.0);
    assert!(world.get_object(bomber_id).is_none());
    assert_eq!(
        world
            .objects
            .iter()
            .filter(|(_, object)| object.proto_object_name == "carpet_explosion")
            .count(),
        4
    );
}

#[test]
fn invoke_power_two_routes_position_and_direction_by_retail_user_id() {
    let database = database();
    let mut world = test_world(&database);
    let executor = CommandExecutor::with_database(&database);
    let user_id = PowerUserId::new(1, POWER_TYPE, 91);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::Power(invoke_command(user_id)),
    );
    assert_eq!(world.active_carpet_bombing_powers().len(), 1);
    assert_eq!(
        world.active_carpet_bombing_powers()[0].power_user_id(),
        user_id
    );

    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Position,
            Vec3::ZERO,
        )),
    );
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            PowerUserId::new(1, POWER_TYPE, 92),
            PowerInputCommandType::Direction,
            Vec3::X,
        )),
    );
    assert_eq!(
        world.active_carpet_bombing_powers()[0].phase(),
        CarpetBombingPhase::WaitingForInputs
    );
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Direction,
            Vec3::X,
        )),
    );
    assert_eq!(
        world.active_carpet_bombing_powers()[0].phase(),
        CarpetBombingPhase::Active
    );
}

#[test]
fn profile_and_packed_type_validation_match_the_native_contract() {
    let base_database = database();
    let mut world = test_world(&base_database);
    assert_eq!(
        world.invoke_carpet_bombing_power(
            &base_database,
            invocation(true, PowerUserId::new(1, 2, 4)),
        ),
        Err(NativePowerError::InvalidData("PowerUserID"))
    );

    let mut malformed = database();
    set_level_value(&mut malformed, "BombSpacing", "4");
    assert_eq!(
        world.invoke_carpet_bombing_power(&malformed, invocation(true, PowerUserId::INVALID),),
        Err(NativePowerError::InvalidData("BombSpacing"))
    );

    let mut zero_bombs = database();
    set_level_value(&mut zero_bombs, "MaxBombs", "-1");
    let id = world
        .invoke_carpet_bombing_power(&zero_bombs, invocation(true, PowerUserId::INVALID))
        .unwrap();
    assert!(world.active_carpet_bombing_powers()[0].requires_los);
    assert!(world.submit_carpet_bombing_power_input(
        &zero_bombs,
        id,
        NativePowerInput::Position(Vec3::ZERO),
    ));
    assert!(world.submit_carpet_bombing_power_input(
        &zero_bombs,
        id,
        NativePowerInput::Direction(Vec3::X),
    ));
    world.update_entities_with_database(0.1, &zero_bombs);
    assert!(world.active_carpet_bombing_powers().is_empty());
}

fn advance(world: &mut World, database: &Database, gameplay: &GameplayCatalog, ticks: usize) {
    for _ in 0..ticks {
        world.game_time_ms = world.game_time_ms.wrapping_add(100);
        world.update_entities_with_database_and_gameplay(0.1, database, gameplay);
    }
}

fn advance_until(
    world: &mut World,
    database: &Database,
    gameplay: &GameplayCatalog,
    maximum_ticks: usize,
    complete: impl Fn(&World) -> bool,
) {
    for _ in 0..maximum_ticks {
        if complete(world) {
            return;
        }
        advance(world, database, gameplay, 1);
    }
    assert!(complete(world), "Carpet Bombing execution did not finish");
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
    unit.proto_object_id = 6;
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

fn assert_payment_state(world: &World, supplies: f32, uses: i32) {
    let player = world.get_player(1).unwrap();
    assert_close(player.get_resource(0), supplies);
    assert_eq!(
        player
            .power_entry(POWER_ID)
            .unwrap()
            .finite_uses_remaining(),
        uses
    );
}

fn invocation(
    ignore_requirements: bool,
    power_user_id: PowerUserId,
) -> CarpetBombingPowerInvocation {
    CarpetBombingPowerInvocation {
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
        power_level: 0,
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
            projectile("carpet_source", 0, Some("carpet_source.tactics")),
            projectile("damage_projectile", 1, None),
            visual("impact", 2, Some(5.0)),
            visual("carpet_explosion", 3, Some(5.0)),
            visual("carpet_bomber", 4, None),
            ProtoObject {
                name: "sys_revealer".to_owned(),
                dbid: Some(5),
                los: Some(1.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "target".to_owned(),
                dbid: Some(6),
                object_class: Some("Unit".to_owned()),
                hitpoints: Some(5_000.0),
                obstruction_radius_x: Some(1.0),
                obstruction_radius_y: Some(1.0),
                obstruction_radius_z: Some(1.0),
                ..ProtoObject::default()
            },
        ],
        powers: vec![carpet_power()],
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
        lifespan: Some(1.0),
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

fn carpet_power() -> Power {
    Power {
        name: "TestCarpetBombing".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("CarpetBombing".to_owned()),
            cost: Some(PowerCost {
                supplies: Some(300.0),
                ..PowerCost::default()
            }),
            base_data_level: Some(DataLevel {
                entries: vec![
                    data("protoobject", "Projectile", "carpet_source"),
                    data("protoobject", "Impact", "impact"),
                    data("protoobject", "Explosion", "carpet_explosion"),
                    data("protoobject", "Bomber", "carpet_bomber"),
                    float_data("InitialDelay", 0.2),
                    float_data("FuseTime", 0.2),
                    float_data("BomberFlyinDistance", 20.0),
                    float_data("BomberFlyinHeight", 10.0),
                    float_data("BomberBombHeight", 5.0),
                    float_data("BomberSpeed", 40.0),
                    float_data("NudgeMultiplier", 1.5),
                ],
                ..DataLevel::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    int_data("MaxBombs", 2),
                    float_data("MaxBombOffset", 4.0),
                    float_data("BombSpacing", 1.0),
                    float_data("LengthMultiplier", 10.0),
                    float_data("WedgeLengthMultiplier", 2.0),
                    float_data("WedgeMinOffset", 2.0),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn carpet_tactics() -> TacticData {
    TacticData {
        weapons: vec![Weapon {
            name: "CarpetBomb".to_owned(),
            damage_per_second: Some(400.0),
            weapon_type: Some("LeaderPower".to_owned()),
            projectile: Some("damage_projectile".to_owned()),
            aoe_radius: Some(10.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "CarpetBombingAttackAction".to_owned(),
            weapon: Some("CarpetBomb".to_owned()),
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
