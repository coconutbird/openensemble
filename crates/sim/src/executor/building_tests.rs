//! Building commands of `BBuildingCommand_processUnit` (`0x1401987D0`).

use super::CommandExecutor;
use crate::EntityId;
use crate::command::Command;
use crate::command_queue::CommandEntry;
use crate::commands::{BuildingCommand, BuildingCommandType, CommandPayload};
use crate::entities::{SquadContainmentState, SquadMode, UnitGarrison};
use crate::entity::Entity;
use crate::player::GAIA_PLAYER;
use crate::world::{GeneralEventType, World};
use glam::Vec3;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::{ObjectCommand, ResourceCost};
use pipeline::database::hw1::{Database, GameData, ProtoObject};

const RECYCLE_BUILDING: &str = "recycle_building";

fn execute(world: &mut World, database: Option<&Database>, command: BuildingCommand) {
    let entry = CommandEntry {
        command: CommandPayload::Building(command),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };
    database
        .map_or_else(CommandExecutor::new, CommandExecutor::with_database)
        .execute(world, &entry);
}

fn building_command(kind: BuildingCommandType, unit_id: EntityId) -> BuildingCommand {
    BuildingCommand {
        base: Command {
            player_id: 1,
            recipients: vec![unit_id],
            ..BuildingCommand::default().base
        },
        building_type: kind,
        ..BuildingCommand::default()
    }
}

fn squad_with_unit(world: &mut World, player_id: u8) -> (EntityId, EntityId) {
    let squad_id = world.create_squad(player_id);
    let unit_id = world.create_unit(player_id);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}

/// A built `RecycleBuilding` of player 1 at half of its 200 hitpoints.
fn recycle_building(world: &mut World) -> EntityId {
    let building_id = world.create_building(1);
    let building = world.get_building_mut(building_id).unwrap();
    building.proto_object_name = RECYCLE_BUILDING.to_owned();
    building.built = true;
    building.max_hitpoints = 200.0;
    building.hitpoints = 100.0;
    building_id
}

/// `RecycleBuilding` costs 300 Supplies and has a Kill command; the recycle refund rate is
/// one half.
fn recycle_database(command_type: &str) -> Database {
    Database {
        objects: vec![ProtoObject {
            name: RECYCLE_BUILDING.to_owned(),
            dbid: Some(1),
            object_class: Some("Building".to_owned()),
            hitpoints: Some(200.0),
            costs: vec![ResourceCost {
                resource_type: "Supplies".to_owned(),
                amount: 300.0,
            }],
            commands: vec![ObjectCommand {
                command_type: Some(command_type.to_owned()),
                ..ObjectCommand::default()
            }],
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    deductable: Some(true),
                }],
            }),
            recyle_refund_rate: Some(0.5),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn supplies(world: &World, player_id: u8) -> f32 {
    world.get_player(player_id).unwrap().resources.get(0)
}

#[test]
fn unload_units_unloads_only_the_command_player_squads_of_the_building_squad() {
    let mut world = World::new();
    world.init_players(2);
    let (container_squad, container_unit) = squad_with_unit(&mut world, GAIA_PLAYER);
    world.get_unit_mut(container_unit).unwrap().garrison =
        UnitGarrison::container(0.0, false, false, Vec::new());
    let own = squad_with_unit(&mut world, 1).0;
    let other = squad_with_unit(&mut world, 2).0;
    world
        .issue_garrison_order(1, own, container_squad, 0.0)
        .unwrap();
    world
        .issue_garrison_order(2, other, container_squad, 0.0)
        .unwrap();
    world.advance_time(50);
    world.update_entities(0.05);

    execute(
        &mut world,
        None,
        building_command(BuildingCommandType::UnloadUnits, container_unit),
    );

    assert!(matches!(
        world.get_squad(own).unwrap().garrison.state(),
        SquadContainmentState::Ungarrisoning { .. }
    ));
    assert!(matches!(
        world.get_squad(other).unwrap().garrison.state(),
        SquadContainmentState::Garrisoned { .. }
    ));
}

#[test]
fn kill_recycles_the_building_on_its_next_update_and_refunds_the_recycle_value() {
    let database = recycle_database("Kill");
    let mut world = World::new();
    world.init_players(1);
    let building_id = recycle_building(&mut world);
    let subscriber = world.subscribe_general_event(GeneralEventType::CommandRecycle, None, false);

    execute(
        &mut world,
        Some(&database),
        building_command(BuildingCommandType::Kill, building_id),
    );

    assert!(world.general_event_fired(subscriber));
    assert!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .self_destructing
    );
    assert!(world.get_building(building_id).unwrap().is_alive());

    let _update = world.update_production(0.05, &database);

    // 300 Supplies times the refund rate 0.5 times the hitpoint part 100 / 200.
    assert_eq!(supplies(&world, 1).to_bits(), 75.0_f32.to_bits());
    assert!(
        world
            .get_unit(building_id)
            .is_none_or(|building| !building.is_alive())
    );
}

#[test]
fn destroy_base_recycles_a_building_with_a_destroy_base_command() {
    let database = recycle_database("DestroyBase");
    let mut world = World::new();
    world.init_players(1);
    let building_id = recycle_building(&mut world);

    execute(
        &mut world,
        Some(&database),
        building_command(BuildingCommandType::DestroyBase, building_id),
    );

    assert!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .self_destructing
    );
}

#[test]
fn kill_is_refused_without_a_kill_or_destroy_base_command() {
    let database = recycle_database("Research");
    let mut world = World::new();
    world.init_players(1);
    let building_id = recycle_building(&mut world);
    let subscriber = world.subscribe_general_event(GeneralEventType::CommandRecycle, None, false);

    execute(
        &mut world,
        Some(&database),
        building_command(BuildingCommandType::Kill, building_id),
    );

    assert!(!world.general_event_fired(subscriber));
    assert!(
        !world
            .get_building(building_id)
            .unwrap()
            .production
            .self_destructing
    );
}

#[test]
fn cancel_kill_and_cancel_destroy_base_stop_the_recycle() {
    for cancel in [
        BuildingCommandType::CancelKill,
        BuildingCommandType::CancelDestroyBase,
    ] {
        let database = recycle_database("Kill");
        let mut world = World::new();
        world.init_players(1);
        let building_id = recycle_building(&mut world);
        let subscriber =
            world.subscribe_general_event(GeneralEventType::CommandRecycleCancel, None, false);
        execute(
            &mut world,
            Some(&database),
            building_command(BuildingCommandType::Kill, building_id),
        );

        execute(
            &mut world,
            Some(&database),
            building_command(cancel, building_id),
        );
        let _update = world.update_production(0.05, &database);

        assert!(world.general_event_fired(subscriber), "{cancel:?}");
        let building = world.get_building(building_id).unwrap();
        assert!(building.is_alive(), "{cancel:?}");
        assert!(!building.production.self_destructing, "{cancel:?}");
        assert_eq!(supplies(&world, 1).to_bits(), 0.0_f32.to_bits());
    }
}

#[test]
fn train_lock_and_unlock_set_the_lock_and_change_the_building_squad_mode() {
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad(1);
    let building_id = world.create_building(1);
    assert!(world.attach_unit_to_squad(building_id, squad_id));

    execute(
        &mut world,
        None,
        building_command(BuildingCommandType::TrainLock, building_id),
    );

    let building = world.get_building(building_id).unwrap();
    assert_eq!(building.production.train_lock_player, Some(1));
    assert_eq!(
        world.get_squad(squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Lockdown)
    );

    let squad = world.get_squad_mut(squad_id).unwrap();
    let _finished = squad.finish_change_mode();
    assert!(world.building_train_locked(building_id));

    execute(
        &mut world,
        None,
        building_command(BuildingCommandType::TrainUnlock, building_id),
    );

    assert_eq!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .train_lock_player,
        None
    );
    assert_eq!(
        world.get_squad(squad_id).unwrap().changing_to_mode(),
        Some(SquadMode::Normal)
    );
}

#[test]
fn a_train_locked_building_keeps_its_trained_squads_inside() {
    let database = Database::default();
    let mut world = World::new();
    world.init_players(1);
    let building_id = world.create_building(1);
    let trained_squad = squad_with_unit(&mut world, 1).0;
    world
        .get_building_mut(building_id)
        .unwrap()
        .production
        .queue_trained_squad(trained_squad, false);

    execute(
        &mut world,
        None,
        building_command(BuildingCommandType::TrainLock, building_id),
    );
    let _update = world.update_production(0.05, &database);

    assert!(world.building_train_locked(building_id));
    assert_eq!(
        world
            .get_building(building_id)
            .unwrap()
            .production
            .first_trained_squad()
            .map(crate::entities::TrainedSquadBirth::squad_id),
        Some(trained_squad)
    );
}

/// `processUnit` sends Reinforce, Ability, Tribute, Power, `RallyPoint` and `ReverseHotDrop`
/// to its default case: the command changes nothing.
#[test]
fn building_commands_without_a_process_unit_case_change_nothing() {
    for kind in [
        BuildingCommandType::Reinforce,
        BuildingCommandType::Ability,
        BuildingCommandType::Tribute,
        BuildingCommandType::Power,
        BuildingCommandType::RallyPoint,
        BuildingCommandType::ReverseHotDrop,
    ] {
        let database = recycle_database("Kill");
        let mut world = World::new();
        world.init_players(2);
        world.get_player_mut(1).unwrap().resources.set(0, 500.0);
        let building_id = recycle_building(&mut world);
        let before = world.checksum();
        let mut command = building_command(kind, building_id);
        command.target_id = 2;
        command.count = 1;
        command.target_position = Vec3::X * 10.0;

        execute(&mut world, Some(&database), command);

        assert_eq!(world.checksum(), before, "{kind:?}");
    }
}
