use super::*;
use crate::command::Command;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::{
    PowerCommand, PowerCommandType, PowerInputCommand, PowerInputCommandType, PowerUserId,
    power_command_flags,
};
use crate::executor::CommandExecutor;
use crate::player::PowerGrant;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use crate::{PowerTransportPhase, Unit};
use pipeline::database::hw1::gamedata::{PopsWrapper, ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::PopulationAmount;
use pipeline::database::hw1::powers::{DataEntry, DataLevel, PowerAttributes, PowerCost};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::{Civ, Database, GameData, Power, ProtoObject, Squad as ProtoSquad};

const POWER_ID: i32 = 0;
const TRANSPORT_TYPE: u32 = 8;

#[test]
fn paid_transport_selects_by_type_and_moves_passengers_through_carrier_state() {
    let database = database();
    let mut world = paid_world(&database);
    let infantry_near = spawn(&mut world, &database, "infantry_squad", Vec3::ZERO);
    let vehicle = spawn(
        &mut world,
        &database,
        "vehicle_squad",
        Vec3::new(1.0, 0.0, 0.0),
    );
    let infantry_second = spawn(
        &mut world,
        &database,
        "infantry_squad",
        Vec3::new(2.0, 0.0, 0.0),
    );
    let _infantry_over_limit = spawn(
        &mut world,
        &database,
        "infantry_squad",
        Vec3::new(3.0, 0.0, 0.0),
    );
    let _outside_radius = spawn(
        &mut world,
        &database,
        "vehicle_squad",
        Vec3::new(50.0, 0.0, 0.0),
    );
    let used_power = world.subscribe_general_event(GeneralEventType::UsedPower, Some(1), false);
    let execution_id = world
        .invoke_transport_power(&database, invocation(false, PowerUserId::new(1, 8, 41)))
        .unwrap();
    assert_payment_state(&world, 500.0, 2);

    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    let execution = &world.active_transport_powers()[0];
    assert_eq!(execution.pickup_location(), Some(Vec3::ZERO));
    assert_eq!(
        execution.selected_squad_ids(),
        &[infantry_near, vehicle, infantry_second]
    );
    assert_payment_state(&world, 500.0, 2);

    assert!(!world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::new(20.0, 0.0, 0.0)),
        false,
    ));
    assert_eq!(world.active_transport_powers().len(), 1);
    assert_payment_state(&world, 500.0, 2);

    let dropoff = Vec3::new(60.0, 0.0, 0.0);
    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(dropoff),
        false,
    ));
    assert!(world.active_transport_powers().is_empty());
    assert!(world.general_event_fired(used_power));
    assert_payment_state(&world, 300.0, 1);

    let carrier_ids = power_carriers(&world);
    assert_eq!(carrier_ids.len(), 2);
    advance_until(&mut world, &database, 180, |world| {
        carrier_ids.iter().all(|carrier_id| {
            world.get_squad(*carrier_id).is_some_and(|carrier| {
                carrier
                    .power_transport()
                    .is_some_and(|action| action.phase() == PowerTransportPhase::Transporting)
            })
        })
    });
    for passenger_id in [infantry_near, vehicle, infantry_second] {
        assert_passenger_garrisoned(&world, passenger_id);
    }

    advance_until(&mut world, &database, 600, |world| {
        carrier_ids.iter().all(|carrier_id| {
            world.get_squad(*carrier_id).is_some_and(|carrier| {
                carrier
                    .power_transport()
                    .is_some_and(|action| action.phase() == PowerTransportPhase::Outgoing)
            })
        })
    });
    for passenger_id in [infantry_near, vehicle, infantry_second] {
        assert_passenger_released_near(&world, passenger_id, dropoff);
    }

    advance_until(&mut world, &database, 180, |world| {
        carrier_ids
            .iter()
            .all(|carrier_id| world.get_squad(*carrier_id).is_none())
    });
}

#[test]
fn shutdown_rolls_back_pickup_before_destroying_the_targeting_session() {
    let database = database();
    let mut world = test_world(&database);
    let passenger = spawn(&mut world, &database, "infantry_squad", Vec3::ZERO);
    let execution_id = world
        .invoke_transport_power(&database, invocation(true, PowerUserId::INVALID))
        .unwrap();
    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert_eq!(
        world.active_transport_powers()[0].selected_squad_ids(),
        &[passenger]
    );

    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Shutdown,
        false,
    ));
    let execution = &world.active_transport_powers()[0];
    assert_eq!(execution.pickup_location(), None);
    assert!(execution.selected_squad_ids().is_empty());

    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Shutdown,
        false,
    ));
    assert!(world.active_transport_powers().is_empty());
}

#[test]
fn commanding_a_reserved_squad_cancels_its_incoming_pickup() {
    let database = database();
    let mut world = test_world(&database);
    let passenger = spawn(&mut world, &database, "infantry_squad", Vec3::ZERO);
    let execution_id = world
        .invoke_transport_power(&database, invocation(true, PowerUserId::INVALID))
        .unwrap();
    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::X * 60.0),
        false,
    ));
    let carrier_id = power_carriers(&world)[0];
    assert_eq!(
        world
            .get_squad(carrier_id)
            .unwrap()
            .power_transport()
            .unwrap()
            .passenger_squad_ids(),
        &[passenger]
    );

    assert!(world.issue_squad_move_order_to_position(1, passenger, Vec3::Z * 10.0, false, false,));
    assert!(
        world
            .get_squad(carrier_id)
            .unwrap()
            .power_transport()
            .unwrap()
            .passenger_squad_ids()
            .is_empty()
    );
    advance_until(&mut world, &database, 360, |world| {
        world.get_squad(carrier_id).is_none()
    });
    assert!(!world.get_squad(passenger).unwrap().garrison.is_garrisoned());
}

#[test]
fn invoke_power_two_and_confirm_vectors_route_by_transport_user_id() {
    let database = database();
    let mut world = test_world(&database);
    let _passenger = spawn(&mut world, &database, "infantry_squad", Vec3::ZERO);
    let executor = CommandExecutor::with_database(&database);
    let user_id = PowerUserId::new(1, TRANSPORT_TYPE, 19);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::Power(invoke_command(user_id)),
    );
    assert_eq!(world.active_transport_powers().len(), 1);
    assert_eq!(world.active_transport_powers()[0].power_user_id(), user_id);

    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            PowerUserId::new(1, TRANSPORT_TYPE, 20),
            PowerInputCommandType::Confirm,
            Vec3::ZERO,
        )),
    );
    assert_eq!(world.active_transport_powers()[0].pickup_location(), None);
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Confirm,
            Vec3::ZERO,
        )),
    );
    assert_eq!(
        world.active_transport_powers()[0].pickup_location(),
        Some(Vec3::ZERO)
    );
    execute_command(
        &executor,
        &mut world,
        QueuedCommand::PowerInput(input_command(
            user_id,
            PowerInputCommandType::Confirm,
            Vec3::new(60.0, 0.0, 0.0),
        )),
    );
    assert!(world.active_transport_powers().is_empty());
    assert_eq!(power_carriers(&world).len(), 1);
}

#[test]
fn profile_and_packed_transport_type_stay_strict_even_without_costs() {
    let database = database();
    let mut world = test_world(&database);
    assert_eq!(
        world.invoke_transport_power(&database, invocation(true, PowerUserId::new(1, 9, 3)),),
        Err(NativePowerError::InvalidData("PowerUserID"))
    );

    let mut malformed = database;
    malformed.civs[0].transport = None;
    assert_eq!(
        world.invoke_transport_power(&malformed, invocation(true, PowerUserId::INVALID),),
        Err(NativePowerError::MissingData("TransportPrototype"))
    );
}

#[test]
fn missing_game_data_transport_values_use_retail_constructor_defaults() {
    let mut database = database();
    let game_data = database.game_data.as_mut().unwrap();
    game_data.transport_max = None;
    game_data.transport_incoming_height = None;
    game_data.transport_incoming_offset = None;
    game_data.transport_outgoing_height = None;
    game_data.transport_outgoing_offset = None;
    game_data.transport_pickup_height = None;
    game_data.transport_dropoff_height = None;
    let mut world = test_world(&database);
    world
        .invoke_transport_power(&database, invocation(true, PowerUserId::INVALID))
        .unwrap();
    let execution = &world.active_transport_powers()[0];
    assert_eq!(execution.maximum_transports, 3);
    assert_close(execution.incoming_height, 60.0);
    assert_close(execution.incoming_offset, 40.0);
    assert_close(execution.outgoing_height, 60.0);
    assert_close(execution.outgoing_offset, 40.0);
    assert_close(execution.pickup_height, 12.0);
    assert_close(execution.dropoff_height, 12.0);
}

#[test]
fn carrier_count_rounds_population_but_assignment_uses_actual_capacity() {
    let mut database = database();
    database
        .objects
        .iter_mut()
        .find(|prototype| prototype.name == "infantry")
        .unwrap()
        .population[0]
        .amount = 1.4;
    let mut world = test_world(&database);
    let first = spawn(&mut world, &database, "infantry_squad", Vec3::ZERO);
    let second = spawn(&mut world, &database, "infantry_squad", Vec3::X);
    let execution_id = world
        .invoke_transport_power(&database, invocation(true, PowerUserId::INVALID))
        .unwrap();
    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert!(world.submit_transport_power_input(
        &database,
        execution_id,
        NativePowerInput::Confirm(Vec3::X * 60.0),
        false,
    ));

    let carriers = power_carriers(&world);
    assert_eq!(carriers.len(), 1);
    let passengers = world
        .get_squad(carriers[0])
        .unwrap()
        .power_transport()
        .unwrap()
        .passenger_squad_ids();
    assert_eq!(passengers.len(), 1);
    assert!(passengers[0] == first || passengers[0] == second);
}

fn advance_until(
    world: &mut World,
    database: &Database,
    maximum_ticks: usize,
    complete: impl Fn(&World) -> bool,
) {
    for _ in 0..maximum_ticks {
        if complete(world) {
            return;
        }
        world.update_entities_with_database(0.1, database);
    }
    assert!(complete(world), "transport state did not converge");
}

fn assert_passenger_garrisoned(world: &World, squad_id: EntityId) {
    let squad = world.get_squad(squad_id).unwrap();
    assert!(squad.garrison.is_garrisoned());
    assert!(
        squad
            .unit_ids
            .iter()
            .all(|unit_id| world.get_unit(*unit_id).is_some_and(Unit::is_garrisoned))
    );
}

fn assert_passenger_released_near(world: &World, squad_id: EntityId, expected: Vec3) {
    let squad = world.get_squad(squad_id).unwrap();
    assert!(!squad.garrison.is_garrisoned());
    assert!(squad.unit_ids.iter().all(|unit_id| {
        world
            .get_unit(*unit_id)
            .is_some_and(|unit| !unit.is_garrisoned())
    }));
    let delta = squad.base.position - expected;
    assert!(delta.x.abs() < 0.001);
    assert!(delta.z.abs() <= 8.001);
}

fn power_carriers(world: &World) -> Vec<EntityId> {
    world
        .squads
        .iter()
        .filter_map(|(squad_id, squad)| squad.power_transport().is_some().then_some(squad_id))
        .collect()
}

fn paid_world(database: &Database) -> World {
    let mut world = test_world(database);
    world.get_player_mut(1).unwrap().set_resource(0, 500.0);
    assert!(world.grant_player_power(
        1,
        database,
        PowerGrant {
            proto_power_id: POWER_ID,
            squad_id: EntityId::INVALID,
            uses: 2,
            icon_location: -1,
            ignore_cost: false,
            ignore_tech_prerequisites: false,
            ignore_population: false,
        },
    ));
    world
}

fn test_world(database: &Database) -> World {
    let mut world = World::with_seed(53);
    world.init_players(1);
    world.configure_prototype_catalogs(database);
    let player = world.get_player_mut(1).unwrap();
    player.civ_id = 0;
    player.configure_population_slots(1);
    assert!(player.set_population_limits(0, 50.0, 50.0));
    world
}

fn spawn(world: &mut World, database: &Database, prototype: &str, position: Vec3) -> EntityId {
    let prototype_id = squad_prototype_id(database, prototype).unwrap();
    spawn_squad_at(world, database, 1, prototype_id, position, Vec3::Z).unwrap()
}

fn invocation(ignore_requirements: bool, power_user_id: PowerUserId) -> TransportPowerInvocation {
    TransportPowerInvocation {
        player_id: 1,
        proto_power_id: POWER_ID,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
        ignore_requirements,
        power_user_id,
    }
}

fn assert_payment_state(world: &World, supplies: f32, uses: i32) {
    let player = world.get_player(1).unwrap();
    assert!((player.get_resource(0) - supplies).abs() < 0.000_1);
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
    target: Vec3,
) -> PowerInputCommand {
    PowerInputCommand {
        base: Command {
            player_id: 1,
            ..Command::default()
        },
        input_type,
        vector: target.extend(0.0),
        power_user_id: user_id,
    }
}

fn database() -> Database {
    Database {
        objects: vec![carrier(), infantry(), vehicle()],
        squads: vec![
            squad("infantry_squad", 100, "infantry"),
            squad("vehicle_squad", 101, "vehicle"),
        ],
        powers: vec![transport_power()],
        civs: vec![Civ {
            name: "UNSC".to_owned(),
            transport: Some("pelican".to_owned()),
            ..Civ::default()
        }],
        game_data: Some(game_data()),
        ..Database::default()
    }
}

fn game_data() -> GameData {
    GameData {
        resources: Some(ResourcesWrapper {
            entries: vec![resource("Supplies")],
        }),
        pops: Some(PopsWrapper {
            entries: vec!["Unit".to_owned()],
        }),
        transport_max: Some(3),
        transport_incoming_height: Some(0.0),
        transport_incoming_offset: Some(10.0),
        transport_outgoing_height: Some(0.0),
        transport_outgoing_offset: Some(10.0),
        transport_pickup_height: Some(0.0),
        transport_dropoff_height: Some(0.0),
        ..GameData::default()
    }
}

fn carrier() -> ProtoObject {
    ProtoObject {
        name: "pelican".to_owned(),
        dbid: Some(10),
        object_class: Some("Unit".to_owned()),
        object_types: vec!["Air".to_owned()],
        contain: vec!["Transportable".to_owned()],
        hitpoints: Some(500.0),
        max_contained: Some(2),
        obstruction_radius_x: Some(3.0),
        obstruction_radius_z: Some(3.0),
        velocity: Some(20.0),
        ..ProtoObject::default()
    }
}

fn infantry() -> ProtoObject {
    passenger_object("infantry", 20, "Infantry")
}

fn vehicle() -> ProtoObject {
    passenger_object("vehicle", 21, "GroundVehicle")
}

fn passenger_object(name: &str, dbid: i32, unit_type: &str) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Unit".to_owned()),
        object_types: vec!["Transportable".to_owned(), unit_type.to_owned()],
        hitpoints: Some(100.0),
        population: vec![PopulationAmount {
            population_type: Some("Unit".to_owned()),
            amount: 1.0,
        }],
        velocity: Some(5.0),
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

fn transport_power() -> Power {
    Power {
        name: "TestTransport".to_owned(),
        attributes: Some(PowerAttributes {
            power_type: Some("Transport".to_owned()),
            ui_radius: Some(20.0),
            cost: Some(PowerCost {
                supplies: Some(200.0),
                ..PowerCost::default()
            }),
            data_levels: vec![DataLevel {
                level: Some(0),
                entries: vec![
                    data("float", "MinTransportDistance", "30"),
                    data("int", "MaxGroundVehicles", "1"),
                    data("int", "MaxInfantryUnits", "2"),
                ],
            }],
            ..PowerAttributes::default()
        }),
        ..Power::default()
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
