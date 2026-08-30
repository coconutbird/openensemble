use super::*;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::WorkCommand;
use crate::executor::CommandExecutor;
use crate::gameplay::GameplayCatalog;
use crate::scenario::configure_unit_from_proto;
use pipeline::database::hw1::gamedata::{ResourceDef, ResourcesWrapper};
use pipeline::database::hw1::objects::CaptureCost;
use pipeline::database::hw1::tactics::{Action, TacticData};
use pipeline::database::hw1::{Civ, Database, GameData, ProtoObject};

#[test]
fn capture_transfers_owner_and_restores_target_health() {
    let (mut world, database, gameplay, squad_id, target_id) = fixture(5.0, Vec3::ZERO);
    world.get_unit_mut(target_id).unwrap().hitpoints = 10.0;

    assert!(world.issue_capture_order(1, squad_id, target_id, &database, &gameplay));
    world.update_entities_with_database_and_gameplay(1.0, &database, &gameplay);

    assert_eq!(world.get_unit(target_id).unwrap().base.player_id, 1);
    assert_eq!(
        world.get_unit(target_id).unwrap().hitpoints.to_bits(),
        world.get_unit(target_id).unwrap().max_hitpoints.to_bits()
    );
    assert_eq!(
        world.get_squad(squad_id).unwrap().capture_phase(),
        CapturePhase::Done
    );
}

#[test]
fn capture_cost_is_paid_once_and_refunded_after_last_same_player_link() {
    let (mut world, mut database, gameplay, first_squad, target_id) = fixture(20.0, Vec3::ZERO);
    database.objects[1].capture_costs.push(CaptureCost {
        civilization: Some("UNSC".to_owned()),
        resource_type: "Supplies".to_owned(),
        amount: 100.0,
    });
    world.get_player_mut(1).unwrap().civ_id = 0;
    world.get_player_mut(1).unwrap().resources.set(0, 250.0);
    let second_squad = add_worker_squad(&mut world, &database, 1, Vec3::ZERO);

    assert!(world.issue_capture_order(1, first_squad, target_id, &database, &gameplay));
    assert_close(world.get_player(1).unwrap().get_resource(0), 150.0);
    assert!(world.issue_capture_order(1, second_squad, target_id, &database, &gameplay));
    assert_close(world.get_player(1).unwrap().get_resource(0), 150.0);

    assert!(world.cancel_capture_order(first_squad));
    assert_close(world.get_player(1).unwrap().get_resource(0), 150.0);
    assert!(world.cancel_capture_order(second_squad));
    assert_close(world.get_player(1).unwrap().get_resource(0), 250.0);
}

#[test]
fn opposing_progress_is_removed_before_new_progress_and_idle_work_decays() {
    let (mut world, mut database, gameplay, first_squad, target_id) = fixture(10.0, Vec3::ZERO);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.set_team_relation(1, 2, TeamRelation::Enemy);
    world.set_team_relation(2, 1, TeamRelation::Enemy);
    let second_squad = add_worker_squad(&mut world, &database, 2, Vec3::ZERO);

    assert!(world.issue_capture_order(1, first_squad, target_id, &database, &gameplay));
    world.update_entities_with_database_and_gameplay(0.5, &database, &gameplay);
    assert_eq!(
        world.get_unit(target_id).unwrap().capture_player_id(),
        Some(1)
    );
    assert!(world.cancel_capture_order(first_squad));
    assert_close(world.get_unit(target_id).unwrap().capture_points(), 0.0);

    assert!(world.issue_capture_order(2, second_squad, target_id, &database, &gameplay));
    world.update_entities_with_database_and_gameplay(0.25, &database, &gameplay);
    assert_eq!(
        world.get_unit(target_id).unwrap().capture_player_id(),
        Some(2)
    );
    let before = world.get_unit(target_id).unwrap().capture_points();
    database.game_data.as_mut().unwrap().capture_decay_rate = Some(1.0);
    world
        .get_squad_mut(second_squad)
        .unwrap()
        .set_position(Vec3::X * 100.0);
    for unit_id in world.get_squad(second_squad).unwrap().unit_ids.clone() {
        world.get_unit_mut(unit_id).unwrap().base.position = Vec3::X * 100.0;
    }
    world.update_entities_with_database_and_gameplay(0.25, &database, &gameplay);
    assert!(world.get_unit(target_id).unwrap().capture_points() < before);
}

#[test]
fn capture_work_command_dispatches_to_the_authoritative_order() {
    let (mut world, database, gameplay, squad_id, target_id) = fixture(20.0, Vec3::ZERO);
    let entry = CommandEntry {
        command: QueuedCommand::Work(WorkCommand::capture_squads(1, vec![squad_id], target_id)),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };

    CommandExecutor::with_database_and_gameplay(&database, &gameplay).execute(&mut world, &entry);

    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.capture_target(), Some(target_id));
    assert_eq!(squad.capture_phase(), CapturePhase::Moving);
}

#[test]
fn only_one_squad_can_work_a_capture_target_at_a_time() {
    let (mut world, database, gameplay, first_squad, target_id) = fixture(20.0, Vec3::ZERO);
    let second_squad = add_worker_squad(&mut world, &database, 1, Vec3::ZERO);

    assert!(world.issue_capture_order(1, first_squad, target_id, &database, &gameplay));
    assert!(world.issue_capture_order(1, second_squad, target_id, &database, &gameplay));
    world.update_entities_with_database_and_gameplay(0.5, &database, &gameplay);

    assert_eq!(
        world.get_squad(first_squad).unwrap().capture_phase(),
        CapturePhase::Working
    );
    assert_eq!(
        world.get_squad(second_squad).unwrap().capture_phase(),
        CapturePhase::Failed
    );
    assert_close(world.get_unit(target_id).unwrap().capture_points(), 2.5);
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= f32::EPSILON,
        "expected {left} to equal {right}"
    );
}

fn fixture(
    capture_points: f32,
    position: Vec3,
) -> (World, Database, GameplayCatalog, EntityId, EntityId) {
    let database = database(capture_points);
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("worker".to_owned(), capture_tactic())]);
    let mut world = World::new();
    world.init_players(2);
    let squad_id = add_worker_squad(&mut world, &database, 1, position);
    let target_id = world.create_building_at(0, position);
    configure_unit_from_proto(&mut world, target_id, "node", 1, &database.objects[1]);
    (world, database, gameplay, squad_id, target_id)
}

fn add_worker_squad(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    position: Vec3,
) -> EntityId {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = world.create_unit_at(player_id, position);
    configure_unit_from_proto(world, unit_id, "worker", 0, &database.objects[0]);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    squad_id
}

fn database(capture_points: f32) -> Database {
    Database {
        objects: vec![
            ProtoObject {
                name: "worker".to_owned(),
                tactics: Some("worker.tactics".to_owned()),
                max_velocity: Some(10.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "node".to_owned(),
                object_class: Some("Building".to_owned()),
                build_points: Some(capture_points),
                hitpoints: Some(100.0),
                flags: vec!["Capturable".to_owned(), "Invulnerable".to_owned()],
                ..ProtoObject::default()
            },
        ],
        civs: vec![Civ {
            name: "UNSC".to_owned(),
            ..Civ::default()
        }],
        game_data: Some(GameData {
            resources: Some(ResourcesWrapper {
                entries: vec![ResourceDef {
                    name: "Supplies".to_owned(),
                    ..ResourceDef::default()
                }],
            }),
            capture_decay_rate: Some(0.5),
            ..GameData::default()
        }),
        ..Database::default()
    }
}

fn capture_tactic() -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "Capture".to_owned(),
            action_type: Some("Capture".to_owned()),
            work_rate: Some(5.0),
            work_range: Some(0.1),
            ..Action::default()
        }],
        ..TacticData::default()
    }
}
