use super::*;
use crate::command_queue::{CommandEntry, QueuedCommand};
use crate::commands::WorkCommand;
use crate::entities::RepairOtherPhase;
use crate::executor::CommandExecutor;
use crate::gameplay::GameplayCatalog;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{
    Action, AutoRepair, ProtoObjectRef, TacticData, TacticRules, TargetRule,
};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[test]
fn repair_other_moves_then_heals_by_squad_combat_value() {
    let (mut world, database, gameplay, source_id, target_id) = fixture(Vec3::ZERO, Vec3::X * 12.0);
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    let source_unit_id = world.get_squad(source_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 50.0;
    world.get_unit_mut(source_unit_id).unwrap().work_rate_scalar = 100.0;

    assert!(world.issue_repair_other_order(
        1,
        source_id,
        target_unit_id,
        None,
        &database,
        &gameplay,
    ));
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert_eq!(
        world.get_squad(source_id).unwrap().repair_other_phase(),
        RepairOtherPhase::Moving
    );

    for _ in 0..100 {
        world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
        if world.get_squad(source_id).unwrap().is_repairing_other() {
            break;
        }
    }
    assert!(world.get_squad(source_id).unwrap().is_repairing_other());
    let before = world.get_unit(target_unit_id).unwrap().hitpoints;
    world.update_entities_with_database_and_gameplay(0.1, &database, &gameplay);
    let healed = world.get_unit(target_unit_id).unwrap().hitpoints - before;
    assert_close(healed, 2.0);
}

#[test]
fn explicit_effect_objects_follow_world_state_and_are_removed_on_cancel() {
    let (mut world, database, gameplay, source_id, target_id) = fixture(Vec3::ZERO, Vec3::X);
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 50.0;
    assert!(world.issue_repair_other_order(1, source_id, target_id, None, &database, &gameplay,));

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    let squad = world.get_squad(source_id).unwrap();
    let effect_id = squad.repair_other_effect_id().expect("repair beam");
    let head_id = squad.repair_other_beam_head_id().expect("beam head");
    let tail_id = squad.repair_other_beam_tail_id().expect("beam tail");
    let beam = world.get_object(effect_id).unwrap();
    assert_eq!(beam.base.player_id, 1);
    assert_eq!(beam.proto_object_name, "fx_repair_beam");
    assert_eq!(
        beam.visual_secondary_position(),
        Some(world.get_unit(target_unit_id).unwrap().simulation_center())
    );
    assert_eq!(
        world.get_object(head_id).unwrap().proto_object_name,
        "beam_head"
    );
    assert_eq!(
        world.get_object(tail_id).unwrap().proto_object_name,
        "beam_tail"
    );

    assert!(world.cancel_repair_other_order(source_id));
    assert!(world.get_object(effect_id).is_none());
    assert!(world.get_object(head_id).is_none());
    assert!(world.get_object(tail_id).is_none());
}

#[test]
fn ruleless_effect_uses_the_target_attached_retail_fallback() {
    let (mut world, database, _, source_id, target_id) = fixture(Vec3::ZERO, Vec3::X);
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 50.0;
    let mut tactics = repair_tactic();
    tactics.actions[0].proto_object = None;
    let gameplay = GameplayCatalog::from_tactics(&database, [("repair_unit".to_owned(), tactics)]);
    assert!(world.issue_repair_other_order(1, source_id, target_id, None, &database, &gameplay,));

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    let effect_id = world
        .get_squad(source_id)
        .unwrap()
        .repair_other_effect_id()
        .expect("fallback repair effect");
    let effect = world.get_object(effect_id).unwrap();
    assert_eq!(effect.proto_object_name, "fx_repairing");
    assert_eq!(effect.object_state.attached_to(), Some(target_unit_id));

    assert!(world.cancel_repair_other_order(source_id));
    assert!(world.get_object(effect_id).is_none());
    assert!(
        !world
            .get_unit(target_unit_id)
            .unwrap()
            .object_state
            .attachments()
            .contains(&effect_id)
    );
}

#[test]
fn repair_stops_when_the_target_changes_to_an_enemy_team() {
    let (mut world, database, gameplay, source_id, target_id) = fixture(Vec3::ZERO, Vec3::X);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.set_team_relation(1, 2, crate::player::TeamRelation::Enemy);
    world.set_team_relation(2, 1, crate::player::TeamRelation::Enemy);
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 50.0;
    assert!(world.issue_repair_other_order(1, source_id, target_id, None, &database, &gameplay,));
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    let effect_id = world
        .get_squad(source_id)
        .unwrap()
        .repair_other_effect_id()
        .unwrap();

    assert!(world.change_squad_owner(target_id, 2));
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);

    assert_eq!(
        world.get_squad(source_id).unwrap().repair_other_phase(),
        RepairOtherPhase::Done
    );
    assert!(world.get_object(effect_id).is_none());
}

#[test]
fn full_self_and_enemy_targets_are_rejected_and_excess_finishes() {
    let (mut world, database, gameplay, source_id, target_id) = fixture(Vec3::ZERO, Vec3::X);
    assert!(!world.issue_repair_other_order(1, source_id, source_id, None, &database, &gameplay,));
    assert!(!world.issue_repair_other_order(1, source_id, target_id, None, &database, &gameplay,));
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 99.0;
    assert!(world.issue_repair_other_order(1, source_id, target_id, None, &database, &gameplay,));
    world.update_entities_with_database_and_gameplay(0.1, &database, &gameplay);
    assert_eq!(
        world.get_squad(source_id).unwrap().repair_other_phase(),
        RepairOtherPhase::Done
    );
    assert_eq!(
        world.get_unit(target_unit_id).unwrap().hitpoints.to_bits(),
        100.0_f32.to_bits()
    );
}

#[test]
fn work_command_dispatches_to_the_authoritative_repair_order() {
    let (mut world, database, gameplay, source_id, target_id) = fixture(Vec3::ZERO, Vec3::X);
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 50.0;
    let entry = CommandEntry {
        command: QueuedCommand::Work(WorkCommand::repair_squads(
            1,
            vec![source_id],
            target_unit_id,
            None,
        )),
        exec_time: 0,
        sequence: 0,
        source_client: 1,
    };

    CommandExecutor::with_database_and_gameplay(&database, &gameplay).execute(&mut world, &entry);

    let source = world.get_squad(source_id).unwrap();
    assert_eq!(source.repair_other_target(), Some(target_id));
    assert_eq!(source.repair_other_phase(), RepairOtherPhase::Moving);
}

#[test]
fn idle_opportunity_automatically_repairs_a_visible_same_team_target() {
    let (mut world, database, gameplay, source_id, target_id) = fixture(Vec3::ZERO, Vec3::X * 10.0);
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    world.get_unit_mut(target_unit_id).unwrap().hitpoints = 50.0;

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);

    let source = world.get_squad(source_id).unwrap();
    assert_eq!(source.repair_other_target(), Some(target_id));
    assert_eq!(source.repair_other_action(), Some("RepairOther"));
    assert_eq!(source.repair_other_phase(), RepairOtherPhase::Moving);
}

fn fixture(
    source_position: Vec3,
    target_position: Vec3,
) -> (World, Database, GameplayCatalog, EntityId, EntityId) {
    let database = database();
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("repair_unit".to_owned(), repair_tactic())]);
    let mut world = World::new();
    world.init_players(1);
    world.configure_prototype_catalogs(&database);
    let source_id = spawn(&mut world, &database, "repair_source", source_position);
    let target_id = spawn(&mut world, &database, "repair_target", target_position);
    (world, database, gameplay, source_id, target_id)
}

fn spawn(world: &mut World, database: &Database, name: &str, position: Vec3) -> EntityId {
    let prototype_id = squad_prototype_id(database, name).unwrap();
    spawn_squad_at(world, database, 1, prototype_id, position, Vec3::Z).unwrap()
}

fn repair_tactic() -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "RepairOther".to_owned(),
            action_type: Some("RepairOther".to_owned()),
            work_rate: Some(2.0),
            work_range: Some(3.0),
            allow_reinforce: Some(true),
            auto_repair: Some(AutoRepair {
                idle_time: Some(0),
                threshold: Some(0.99),
                search_distance: Some(50.0),
            }),
            proto_object: Some(ProtoObjectRef {
                name: "fx_repair_beam".to_owned(),
                bone: Some("bone_launchpoint".to_owned()),
                ..ProtoObjectRef::default()
            }),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                action: Some("RepairOther".to_owned()),
                relation: Some("Ally".to_owned()),
                target_types: vec!["Military".to_owned()],
                target_states: vec!["Damaged".to_owned()],
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}

fn database() -> Database {
    Database {
        objects: vec![
            ProtoObject {
                name: "repair_unit".to_owned(),
                dbid: Some(10),
                tactics: Some("repair_unit.tactics".to_owned()),
                hitpoints: Some(100.0),
                combat_value: Some(10.0),
                max_velocity: Some(10.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "vehicle_unit".to_owned(),
                dbid: Some(11),
                hitpoints: Some(100.0),
                combat_value: Some(10.0),
                object_types: vec!["Military".to_owned()],
                ..ProtoObject::default()
            },
            visual("fx_repair_beam", 20, Some("beam_head"), Some("beam_tail")),
            visual("beam_head", 21, None, None),
            visual("beam_tail", 22, None, None),
            visual("fx_repairing", 23, None, None),
        ],
        squads: vec![
            squad("repair_source", 30, "repair_unit"),
            squad("repair_target", 31, "vehicle_unit"),
        ],
        ..Database::default()
    }
}

fn visual(name: &str, dbid: i32, head: Option<&str>, tail: Option<&str>) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        dbid: Some(dbid),
        object_class: Some("Object".to_owned()),
        beam_head: head.map(str::to_owned),
        beam_tail: tail.map(str::to_owned),
        ..ProtoObject::default()
    }
}

fn squad(name: &str, dbid: i32, object: &str) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        dbid: Some(dbid),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: object.to_owned(),
                count: 1,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn assert_close(left: f32, right: f32) {
    assert!(
        (left - right).abs() <= 0.000_1,
        "expected {left} to equal {right}"
    );
}
