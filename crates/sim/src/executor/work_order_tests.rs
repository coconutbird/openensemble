//! Work orders that `BSquad_executeOrder` (`0x140399780`) gives a squad action, and the orders
//! that it refuses because its jump table sends them to the default case.

use super::CommandExecutor;
use crate::EntityId;
use crate::command::Command;
use crate::command_queue::CommandEntry;
use crate::commands::{CommandPayload, WorkCommand};
use crate::entities::SquadWorkAction;
use crate::entity::Entity;
use crate::order::OrderType;
use glam::Vec3;
use pipeline::database::hw1::{Ability, Database, ProtoObject};

use crate::world::World;

fn execute(world: &mut World, database: Option<&Database>, command: WorkCommand) {
    let entry = CommandEntry {
        command: CommandPayload::Work(command),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };
    database
        .map_or_else(CommandExecutor::new, CommandExecutor::with_database)
        .execute(world, &entry);
}

fn work_command(order: OrderType, squad_id: EntityId) -> WorkCommand {
    WorkCommand {
        base: Command {
            id: order as i32,
            player_id: 1,
            recipients: vec![squad_id],
            ..Command::default()
        },
        ..WorkCommand::default()
    }
}

/// Player 1 owns a squad of one `packed_unit` member at the origin.
fn world_with_squad() -> (World, EntityId) {
    let mut world = World::new();
    world.init_players(2);
    let squad_id = world.create_squad(1);
    let unit_id = world.create_unit(1);
    world.get_unit_mut(unit_id).unwrap().proto_object_name = "packed_unit".to_owned();
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (world, squad_id)
}

fn unpack_database() -> Database {
    Database {
        objects: vec![
            ProtoObject {
                name: "packed_unit".to_owned(),
                dbid: Some(1),
                hitpoints: Some(50.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "unpacked_turret".to_owned(),
                dbid: Some(2),
                hitpoints: Some(200.0),
                object_class: Some("Building".to_owned()),
                ..ProtoObject::default()
            },
        ],
        abilities: vec![Ability {
            name: "Unpack".to_owned(),
            objects: vec!["unpacked_turret".to_owned()],
            ..Ability::default()
        }],
        ..Database::default()
    }
}

#[test]
fn unpack_goes_to_the_target_then_replaces_the_squad_with_the_ability_object() {
    let database = unpack_database();
    let (mut world, squad_id) = world_with_squad();
    let target = Vec3::new(0.25, 0.0, 0.0);
    let mut command = work_command(OrderType::Unpack, squad_id);
    command.terrain_point = Some(target);
    command.ability_id = 0;
    command.angle = 90.0;

    execute(&mut world, Some(&database), command);

    assert_eq!(
        world.get_squad(squad_id).unwrap().work_action,
        Some(SquadWorkAction::Unpack {
            target,
            ability_id: Some(0),
            angle: 90.0,
        })
    );
    assert_eq!(world.get_squad(squad_id).unwrap().move_target, Some(target));

    world.update_entities_with_database(0.05, &database);

    assert!(
        world
            .get_squad(squad_id)
            .is_none_or(|squad| !squad.is_alive())
    );
    let turret = world
        .units
        .iter()
        .map(|(_, unit)| unit)
        .find(|unit| unit.proto_object_name == "unpacked_turret")
        .expect("the unpack creates the ability object");
    assert_eq!(turret.base.player_id, 1);
    assert!((turret.base.position.x - target.x).abs() < 1.0e-6);
}

#[test]
fn unpack_without_a_target_position_is_refused() {
    let database = unpack_database();
    let (mut world, squad_id) = world_with_squad();
    let mut command = work_command(OrderType::Unpack, squad_id);
    command.ability_id = 0;

    execute(&mut world, Some(&database), command);

    let squad = world.get_squad(squad_id).unwrap();
    assert!(squad.work_action.is_none());
    assert!(squad.move_target.is_none());
}

#[test]
fn transport_starts_a_controllable_transport_to_the_command_position() {
    let (mut world, squad_id) = world_with_squad();
    let dropoff = Vec3::new(30.0, 0.0, 10.0);
    let mut command = work_command(OrderType::Transport, squad_id);
    command.terrain_point = Some(dropoff);

    execute(&mut world, None, command);

    let transport = world
        .get_squad(squad_id)
        .unwrap()
        .transport_fly_in()
        .expect("the order starts the transport action");
    assert!(transport.is_controllable());
}

#[test]
fn transport_of_another_player_squad_is_refused() {
    let (mut world, squad_id) = world_with_squad();
    let mut command = work_command(OrderType::Transport, squad_id);
    command.base.player_id = 2;
    command.terrain_point = Some(Vec3::X * 30.0);

    execute(&mut world, None, command);

    assert!(
        world
            .get_squad(squad_id)
            .unwrap()
            .transport_fly_in()
            .is_none()
    );
}

#[test]
fn point_blank_attack_records_the_member_opportunity_with_its_target() {
    let (mut world, squad_id) = world_with_squad();
    let target_id = world.create_unit_at(2, Vec3::X * 5.0);
    let mut command = work_command(OrderType::PointBlankAttack, squad_id);
    command.unit_id = target_id;
    command.terrain_point = Some(Vec3::X * 5.0);

    execute(&mut world, None, command);

    assert_eq!(
        world.get_squad(squad_id).unwrap().work_action,
        Some(SquadWorkAction::PointBlankAttack {
            target_id: Some(target_id),
            target_position: Some(Vec3::X * 5.0),
        })
    );
}

#[test]
fn play_blocking_animation_records_the_order_animation_type() {
    let (mut world, squad_id) = world_with_squad();
    let mut command = work_command(OrderType::PlayBlockingAnimation, squad_id);
    command.squad_mode = 7;

    execute(&mut world, None, command);

    assert_eq!(
        world.get_squad(squad_id).unwrap().work_action,
        Some(SquadWorkAction::PlayBlockingAnimation { animation_type: 7 })
    );
}

/// `BSquad_executeOrder` sends order types 6, 7, 13, 14, 18, 24 and 31 to the default case,
/// and order 33 is outside its jump table: these orders get no squad action.
#[test]
fn orders_without_a_squad_action_are_refused_like_retail() {
    for order in [
        OrderType::RequestRepair,
        OrderType::Build,
        OrderType::Honk,
        OrderType::Guard,
        OrderType::RallyPoint,
        OrderType::Wander,
        OrderType::EnergyShield,
        OrderType::InfantryEnergyShield,
    ] {
        let (mut world, squad_id) = world_with_squad();
        let before = world.checksum();
        let mut command = work_command(order, squad_id);
        command.terrain_point = Some(Vec3::X * 20.0);
        command.ability_id = 0;

        execute(&mut world, Some(&unpack_database()), command);

        let squad = world.get_squad(squad_id).unwrap();
        assert!(squad.work_action.is_none(), "{order:?} starts an action");
        assert!(squad.move_target.is_none(), "{order:?} moves the squad");
        assert!(squad.transport_fly_in().is_none(), "{order:?} transports");
        assert_eq!(world.checksum(), before, "{order:?} changes the world");
    }
}
