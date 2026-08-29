use super::*;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::WorkCommand;
use crate::entities::Unit;
use crate::executor::CommandExecutor;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule};
use pipeline::database::hw1::{Ability, Database, ProtoObject};

#[test]
fn command_places_one_mine_per_member_and_charges_each_unit() {
    let (database, gameplay) = mine_gameplay("test_mine", 10.0, 1.0);
    let (mut world, squad_id, unit_ids) = mine_world(2, 25.0);
    issue_mines(&mut world, &database, squad_id, Vec3::ZERO);
    let active_checksum = world.checksum();

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);

    assert_ne!(world.checksum(), active_checksum);
    assert!(world.get_squad(squad_id).unwrap().is_placing_mines());
    for unit_id in unit_ids {
        assert_close(world.get_unit(unit_id).unwrap().ammunition.current(), 15.0);
    }
    let mines = spawned_mines(&world, "test_mine");
    assert_eq!(mines.len(), 2);
    assert_eq!(mines[0].1.base.player_id, 1);
    assert_eq!(mines[1].1.base.player_id, 1);
    assert_eq!(mines[0].1.base.position, Vec3::new(-5.0, 0.0, -5.0));
    assert_eq!(mines[1].1.base.position, Vec3::ZERO);
}

#[test]
fn insufficient_ammunition_completes_after_prior_success() {
    let (database, gameplay) = mine_gameplay("test_mine", 10.0, 1.0);
    let (mut world, squad_id, unit_ids) = mine_world(1, 15.0);
    issue_mines(&mut world, &database, squad_id, Vec3::ZERO);

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert!(world.get_squad(squad_id).unwrap().is_placing_mines());
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);

    assert!(!world.get_squad(squad_id).unwrap().is_placing_mines());
    assert_eq!(world.get_squad(squad_id).unwrap().state, SquadState::Idle);
    assert_close(
        world.get_unit(unit_ids[0]).unwrap().ammunition.current(),
        5.0,
    );
    assert_eq!(spawned_mines(&world, "test_mine").len(), 1);
}

#[test]
fn failed_creation_refunds_ammunition_and_out_of_range_never_charges() {
    let (database, gameplay) = mine_gameplay("missing_mine", 10.0, 1.0);
    let (mut world, squad_id, unit_ids) = mine_world(1, 20.0);
    issue_mines(&mut world, &database, squad_id, Vec3::ZERO);
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);

    assert!(!world.get_squad(squad_id).unwrap().is_placing_mines());
    assert_close(
        world.get_unit(unit_ids[0]).unwrap().ammunition.current(),
        20.0,
    );

    let (database, gameplay) = mine_gameplay("test_mine", 10.0, 1.0);
    issue_mines(&mut world, &database, squad_id, Vec3::X * 2.0);
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert!(!world.get_squad(squad_id).unwrap().is_placing_mines());
    assert_close(
        world.get_unit(unit_ids[0]).unwrap().ammunition.current(),
        20.0,
    );
    assert!(spawned_mines(&world, "test_mine").is_empty());
}

#[test]
fn mine_spiral_matches_recovered_grid_order() {
    let positions = mine_spiral_positions(Vec3::new(10.0, 3.0, -10.0));
    assert_eq!(positions.len(), 81);
    assert_eq!(positions[0], Vec3::new(10.0, 3.0, -10.0));
    assert_eq!(positions[1], Vec3::new(5.0, 3.0, -15.0));
    assert_eq!(positions[8], Vec3::new(5.0, 3.0, -10.0));
    assert_eq!(positions[80], Vec3::new(-10.0, 3.0, -25.0));
}

fn mine_world(member_count: usize, ammunition: f32) -> (World, EntityId, Vec<EntityId>) {
    let mut world = World::with_seed(41);
    world.init_players(1);
    let squad_id = world.create_squad_at(1, Vec3::ZERO);
    let mut unit_ids = Vec::new();
    for _ in 0..member_count {
        let unit_id = world.create_unit_at(1, Vec3::ZERO);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = "minelayer".to_owned();
        unit.ammunition.configure(ammunition, 0.0, true);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        unit_ids.push(unit_id);
    }
    (world, squad_id, unit_ids)
}

fn issue_mines(world: &mut World, database: &Database, squad_id: EntityId, target: Vec3) {
    let command = WorkCommand::place_mines_at(1, vec![squad_id], target, 0);
    CommandExecutor::with_database(database).execute(
        world,
        &CommandEntry {
            command: QueuedCommand::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
    assert!(world.get_squad(squad_id).unwrap().is_placing_mines());
}

fn mine_gameplay(
    mine_object_name: &str,
    ammunition_cost: f32,
    work_range: f32,
) -> (Database, GameplayCatalog) {
    let mut database = Database::new();
    database.abilities.extend([
        Ability {
            name: "Command".to_owned(),
            ..Ability::default()
        },
        Ability {
            name: "LayMines".to_owned(),
            ability_type: Some("Work".to_owned()),
            target_type: Some("Location".to_owned()),
            objects: vec![mine_object_name.to_owned()],
            ammo_cost: Some(ammunition_cost),
            ..Ability::default()
        },
    ]);
    database.objects.push(ProtoObject {
        name: "minelayer".to_owned(),
        object_class: Some("Unit".to_owned()),
        tactics: Some("minelayer.tactics".to_owned()),
        ability_command: Some("LayMines".to_owned()),
        ..ProtoObject::default()
    });
    if mine_object_name == "test_mine" {
        database.objects.push(ProtoObject {
            name: "test_mine".to_owned(),
            object_class: Some("Unit".to_owned()),
            hitpoints: Some(5.0),
            flags: vec!["Immoveable".to_owned()],
            ..ProtoObject::default()
        });
    }
    let tactics = TacticData {
        actions: vec![Action {
            name: "PlaceMine".to_owned(),
            action_type: Some("Mines".to_owned()),
            work_range: Some(work_range),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                action: Some("PlaceMine".to_owned()),
                ability: Some("Command".to_owned()),
                relation: Some("Any".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("minelayer".to_owned(), tactics)]);
    (database, gameplay)
}

fn spawned_mines<'world>(world: &'world World, proto_name: &str) -> Vec<(EntityId, &'world Unit)> {
    world
        .units
        .iter()
        .filter(|(_, unit)| unit.proto_object_name == proto_name)
        .collect()
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
}
