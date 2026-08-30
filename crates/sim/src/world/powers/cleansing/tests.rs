use super::*;
use crate::command::Command;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    PowerCommand, PowerCommandType, PowerInputCommand, PowerInputCommandType, power_command_flags,
};
use crate::executor::CommandExecutor;
use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
use crate::player::PowerGrant;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, GameData, Power, ProtoObject, Squad as ProtoSquad};

const POWER_ID: i32 = 0;

#[test]
fn paid_beam_charges_immediate_ticks_moves_damages_and_stops_when_upkeep_fails() {
    let database = database();
    let gameplay = gameplay(&database);
    let mut world = test_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "owner_squad", -Vec3::X * 20.0);
    let target_id = spawn(&mut world, &database, 2, "target_squad", Vec3::ZERO);
    let target_unit_id = squad_leader(&world, target_id);
    world.get_player_mut(1).unwrap().set_resource(0, 100.0);
    assert!(world.grant_player_power(1, &database, power_grant(owner_id)));
    let used_power = world.subscribe_general_event(GeneralEventType::UsedPower, Some(1), false);
    let execution_id = world
        .invoke_cleansing_power(
            &database,
            invocation(
                owner_id,
                false,
                PowerUserId::new(1, CLEANSING_POWER_TYPE, 7),
            ),
        )
        .unwrap();
    let execution = &world.active_cleansing_powers()[0];
    let beam_id = execution.beam_object_id();
    assert_eq!(execution.id(), execution_id);
    assert_eq!(execution.beam_prototype(), "cleansing_beam");
    assert_eq!(execution.command_interval_ms(), 200);
    assert!(execution.requires_los());
    assert_eq!(world.get_squad(owner_id).unwrap().mode, SquadMode::Power);
    assert_close(world.get_player(1).unwrap().get_resource(0), 90.0);
    assert_eq!(finite_uses(&world), 0);
    assert!(world.general_event_fired(used_power));

    super::update(&mut world, 0.1, &database, Some(&gameplay));
    let execution = &world.active_cleansing_powers()[0];
    assert_close(execution.next_damage_time(), 0.2);
    assert_eq!(execution.active_projectile_ids().len(), 1);
    let projectile = world
        .get_projectile(execution.active_projectile_ids()[0])
        .unwrap();
    assert_eq!(projectile.source_id, squad_leader(&world, owner_id));
    assert_close(projectile.damage, 100.0);
    assert_close(world.get_player(1).unwrap().get_resource(0), 80.0);

    assert!(world.submit_cleansing_power_input(
        &database,
        execution_id,
        NativePowerInput::Position(Vec3::X * 8.0),
    ));
    let health_before = world.get_unit(target_unit_id).unwrap().hitpoints;
    world.update_entities_with_database_and_gameplay(0.1, &database, &gameplay);
    assert_close(world.get_object(beam_id).unwrap().base.position.x, 0.8);
    assert_close(world.get_player(1).unwrap().get_resource(0), 80.0);
    world.update_projectiles(0.05, Some(&database), Some(&gameplay));
    assert!(world.get_unit(target_unit_id).unwrap().hitpoints < health_before);

    world.get_player_mut(1).unwrap().set_resource(0, 5.0);
    world.update_entities_with_database_and_gameplay(0.01, &database, &gameplay);
    assert!(world.active_cleansing_powers().is_empty());
    assert!(world.get_object(beam_id).is_none());
    assert_eq!(world.get_squad(owner_id).unwrap().mode, SquadMode::Normal);
}

#[test]
fn wire_position_and_shutdown_route_by_user_id_and_air_intersection_is_sim_owned() {
    let database = database();
    let gameplay = gameplay(&database);
    let mut world = test_world(&database);
    let owner_id = spawn(&mut world, &database, 1, "owner_squad", -Vec3::X * 20.0);
    let air_id = spawn(&mut world, &database, 2, "air_squad", Vec3::ZERO);
    make_aircraft_physical(&mut world, air_id);
    let executor = CommandExecutor::with_database(&database);
    let user_id = PowerUserId::new(1, CLEANSING_POWER_TYPE, 91);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::Power(invoke_command(owner_id, user_id)),
    );
    let beam_id = world.active_cleansing_powers()[0].beam_object_id();

    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            PowerUserId::new(1, CLEANSING_POWER_TYPE, 92),
            PowerInputCommandType::Position,
            Vec3::X * 4.0,
        )),
    );
    assert_eq!(
        world.active_cleansing_powers()[0].desired_beam_position(),
        Vec3::ZERO
    );
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Position,
            Vec3::X * 2.0,
        )),
    );
    super::update(&mut world, 0.1, &database, Some(&gameplay));
    let execution = &world.active_cleansing_powers()[0];
    assert_close(world.get_object(beam_id).unwrap().base.position.x, 0.8);
    let impact_id = execution.air_impact_object_id();
    assert_eq!(
        world.get_object(impact_id).unwrap().proto_object_name,
        "air_impact"
    );
    assert!(world.get_object(impact_id).unwrap().base.position.y > 0.0);

    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Shutdown,
            Vec3::ZERO,
        )),
    );
    assert!(world.active_cleansing_powers().is_empty());
    assert!(world.get_object(beam_id).is_none());
    assert!(world.get_object(impact_id).is_none());
    assert_eq!(world.get_squad(owner_id).unwrap().mode, SquadMode::Normal);
}

#[test]
fn profile_owner_and_packed_type_validation_match_the_native_contract() {
    let base_database = database();
    let mut world = test_world(&base_database);
    assert_eq!(
        world.invoke_cleansing_power(
            &base_database,
            invocation(EntityId::INVALID, true, PowerUserId::new(1, 2, 4)),
        ),
        Err(NativePowerError::InvalidData("PowerUserID"))
    );
    assert_eq!(
        world.invoke_cleansing_power(
            &base_database,
            invocation(EntityId::INVALID, false, PowerUserId::INVALID),
        ),
        Err(NativePowerError::InvalidTarget)
    );

    let mut zero_tick = database();
    set_base_value(&mut zero_tick, "TickLength", "0");
    assert_eq!(
        world.invoke_cleansing_power(
            &zero_tick,
            invocation(EntityId::INVALID, true, PowerUserId::INVALID),
        ),
        Err(NativePowerError::InvalidData("TickLength"))
    );
    let mut missing_beam = database();
    set_level_value(&mut missing_beam, "Beam", "missing_beam");
    assert_eq!(
        world.invoke_cleansing_power(
            &missing_beam,
            invocation(EntityId::INVALID, true, PowerUserId::INVALID),
        ),
        Err(NativePowerError::UnknownPrototype(
            "missing_beam".to_owned()
        ))
    );

    let id = world
        .invoke_cleansing_power(
            &base_database,
            invocation(EntityId::INVALID, true, PowerUserId::INVALID),
        )
        .unwrap();
    assert!(!world.active_cleansing_powers()[0].requires_los());
    assert!(world.submit_cleansing_power_input(&base_database, id, NativePowerInput::Shutdown,));
}

fn test_world(database: &Database) -> World {
    let mut world = World::with_seed(31);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world.configure_prototype_catalogs(database);
    world
}

fn gameplay(database: &Database) -> GameplayCatalog {
    GameplayCatalog::from_tactics(
        database,
        [("cleansing_source".to_owned(), cleansing_tactics())],
    )
}

fn spawn(
    world: &mut World,
    database: &Database,
    player_id: u8,
    prototype: &str,
    position: Vec3,
) -> EntityId {
    let prototype_id = squad_prototype_id(database, prototype).unwrap();
    spawn_squad_at(world, database, player_id, prototype_id, position, Vec3::Z).unwrap()
}

fn squad_leader(world: &World, squad_id: EntityId) -> EntityId {
    world.get_squad(squad_id).unwrap().unit_ids[0]
}

fn make_aircraft_physical(world: &mut World, squad_id: EntityId) {
    let unit_id = squad_leader(world, squad_id);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.flying = true;
    unit.base.set_position(Vec3::Y * 10.0);
    unit.physics = Some(PhysicsBody::ground_vehicle(
        PhysicsMaterial::default(),
        BoxCollider::new(Vec3::new(2.0, 2.0, 2.0), Vec3::ZERO),
        0.0,
        0.0,
        0.0,
        0.0,
    ));
}

fn invocation(
    squad_id: EntityId,
    ignore_requirements: bool,
    power_user_id: PowerUserId,
) -> CleansingPowerInvocation {
    CleansingPowerInvocation {
        player_id: 1,
        proto_power_id: POWER_ID,
        power_level: 0,
        squad_id,
        target_location: Vec3::ZERO,
        ignore_requirements,
        power_user_id,
    }
}

fn power_grant(squad_id: EntityId) -> PowerGrant {
    PowerGrant {
        proto_power_id: POWER_ID,
        squad_id,
        uses: 1,
        icon_location: -1,
        ignore_cost: false,
        ignore_tech_prerequisites: false,
        ignore_population: false,
    }
}

fn finite_uses(world: &World) -> i32 {
    world
        .get_player(1)
        .unwrap()
        .power_entry(POWER_ID)
        .unwrap()
        .finite_uses_remaining()
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

fn invoke_command(squad_id: EntityId, user_id: PowerUserId) -> PowerCommand {
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
        squad_id,
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
            visual("cleansing_beam", 0),
            projectile("cleansing_source", 1, Some("cleansing_source.tactics")),
            projectile("damage_projectile", 2, None),
            visual("air_impact", 3),
            unit("owner", 4, false, 500.0),
            unit("target", 5, false, 500.0),
            unit("air_target", 6, true, 500.0),
        ],
        squads: vec![
            squad("owner_squad", 10, "owner"),
            squad("target_squad", 11, "target"),
            squad("air_squad", 12, "air_target"),
        ],
        powers: vec![cleansing_power()],
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

fn cleansing_power() -> Power {
    Power {
        name: "TestCleansing".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Cleansing".to_owned()),
            cost: Some(PowerCost {
                supplies: Some(10.0),
                ..PowerCost::default()
            }),
            base_data_level: Some(DataLevel {
                entries: vec![
                    data("protoobject", "Projectile", "cleansing_source"),
                    float_data("MinBeamDistance", 0.0),
                    float_data("MaxBeamDistance", 70.0),
                    float_data("TickLength", 0.2),
                    float_data("SuppliesPerTick", 10.0),
                    float_data("CommandInterval", 0.2),
                    float_data("MaxBeamSpeed", 8.0),
                    data("bool", "RequiresLOS", "true"),
                ],
                ..DataLevel::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("protoobject", "Beam", "cleansing_beam"),
                    data("protoobject", "AirImpactObject", "air_impact"),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
    }
}

fn cleansing_tactics() -> TacticData {
    TacticData {
        weapons: vec![Weapon {
            name: "Cleansing".to_owned(),
            damage_per_second: Some(100.0),
            weapon_type: Some("LeaderPower".to_owned()),
            projectile: Some("damage_projectile".to_owned()),
            aoe_radius: Some(8.0),
            ..Weapon::default()
        }],
        actions: vec![Action {
            name: "CleansingAttackAction".to_owned(),
            weapon: Some("Cleansing".to_owned()),
            default: Some(true),
            ..Action::default()
        }],
        ..TacticData::default()
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

fn visual(name: &str, dbid: i32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        ..ProtoObject::default()
    }
}

fn unit(name: &str, dbid: i32, flying: bool, hitpoints: f32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Unit".to_owned()),
        movement_type: Some(if flying { "Air" } else { "Land" }.to_owned()),
        hitpoints: Some(hitpoints),
        obstruction_radius_x: Some(2.0),
        obstruction_radius_y: Some(2.0),
        obstruction_radius_z: Some(2.0),
        ..ProtoObject::default()
    }
}

fn squad(name: &str, dbid: i32, member: &str) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        dbid: Some(dbid),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: member.to_owned(),
                count: 1,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
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

fn set_level_value(database: &mut Database, name: &str, value: &str) {
    let entry = database.powers[0].attributes.as_mut().unwrap().data_levels[0]
        .entries
        .iter_mut()
        .find(|entry| entry.name == name)
        .unwrap();
    entry.value = value.to_owned();
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
