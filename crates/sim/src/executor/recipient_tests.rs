//! Command recipient gates of `BCommand_processUnits` (`0x140194560`) and
//! `BCommand_processSquads` (`0x1401946D0`).

use super::CommandExecutor;
use crate::EntityId;
use crate::command::Command;
use crate::command_queue::CommandEntry;
use crate::commands::{BuildingCommand, BuildingCommandType, CommandPayload, WorkCommand};
use crate::entities::{Unit, UnitKind};
use crate::order::OrderType;
use crate::player::{GAIA_PLAYER, Resources};
use crate::world::{CustomCommand, World};
use glam::Vec3;

fn execute(world: &mut World, command: CommandPayload) {
    let entry = CommandEntry {
        command,
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };
    CommandExecutor::new().execute(world, &entry);
}

fn move_command(player_id: i32, squad_id: EntityId, target: Vec3) -> CommandPayload {
    CommandPayload::Work(WorkCommand {
        base: Command {
            id: OrderType::Move as i32,
            player_id,
            recipients: vec![squad_id],
            ..Command::default()
        },
        terrain_point: Some(target),
        ..WorkCommand::default()
    })
}

/// Players 1 and 2 are human teammates, player 3 is on another team.
fn coop_world() -> World {
    let mut world = World::new();
    world.init_players(3);
    world.set_coop(true);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 1;
    world.get_player_mut(3).unwrap().team_id = 2;
    world
}

fn building(world: &mut World, owner: u8) -> EntityId {
    let id = world.units.allocate_id();
    let mut unit = Unit::new(id, owner);
    unit.kind = UnitKind::Building;
    unit.built = true;
    world.units.insert(id, unit);
    id
}

#[test]
fn a_squad_command_reaches_a_gaia_squad() {
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad(GAIA_PLAYER);
    let target = Vec3::new(20.0, 0.0, 5.0);

    execute(&mut world, move_command(1, squad_id, target));

    assert_eq!(world.get_squad(squad_id).unwrap().move_target, Some(target));
}

#[test]
fn a_squad_command_does_not_reach_a_coop_teammate_squad() {
    let mut world = coop_world();
    let squad_id = world.create_squad(2);

    execute(&mut world, move_command(1, squad_id, Vec3::X));

    assert_eq!(world.get_squad(squad_id).unwrap().move_target, None);
}

fn custom_command_at(world: &mut World, owner: u8) -> (EntityId, i32) {
    let unit_id = building(world, owner);
    let command_id = world.add_custom_command(CustomCommand {
        unit_id,
        cost: Resources {
            amounts: [10.0, 0.0, 0.0, 0.0],
        },
        ..CustomCommand::default()
    });
    (unit_id, command_id)
}

fn custom_building_command(unit_id: EntityId, command_id: i32) -> CommandPayload {
    CommandPayload::Building(BuildingCommand {
        base: Command {
            player_id: 1,
            recipients: vec![unit_id],
            ..BuildingCommand::default().base
        },
        building_type: BuildingCommandType::CustomCommand,
        target_id: command_id,
        count: 1,
        ..BuildingCommand::default()
    })
}

#[test]
fn a_building_command_reaches_a_coop_teammate_building_but_not_an_enemy_building() {
    let mut world = coop_world();
    world.get_player_mut(1).unwrap().resources.set(0, 100.0);
    let (teammate_unit, teammate_command) = custom_command_at(&mut world, 2);
    let (enemy_unit, enemy_command) = custom_command_at(&mut world, 3);

    execute(
        &mut world,
        custom_building_command(enemy_unit, enemy_command),
    );
    assert!(world.custom_command(enemy_command).is_some());
    assert!((world.get_player(1).unwrap().resources.get(0) - 100.0).abs() < f32::EPSILON);

    execute(
        &mut world,
        custom_building_command(teammate_unit, teammate_command),
    );
    assert!(world.custom_command(teammate_command).is_none());
    assert!((world.get_player(1).unwrap().resources.get(0) - 90.0).abs() < f32::EPSILON);
}

#[test]
fn a_building_command_does_not_reach_a_teammate_building_outside_coop() {
    let mut world = coop_world();
    world.set_coop(false);
    world.get_player_mut(1).unwrap().resources.set(0, 100.0);
    let (unit_id, command_id) = custom_command_at(&mut world, 2);

    execute(&mut world, custom_building_command(unit_id, command_id));

    assert!(world.custom_command(command_id).is_some());
}
