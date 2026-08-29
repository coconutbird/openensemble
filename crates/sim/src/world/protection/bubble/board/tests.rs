use super::*;
use crate::entities::{JoinKind, JoinMergeType, SquadContainmentState, UnitDataScalar};
use crate::scenario::create_squad_from_prototype;
use pipeline::database::hw1::objects::VeterancyLevel;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{
    Action, DamageModifiers, JoinType, ProtoObjectRef, TacticData,
};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[test]
fn timed_enemy_board_reserves_then_captures_and_contains() {
    let (database, gameplay) = board_gameplay(0.1);
    let (mut world, source_id, target_id, source_unit_id, target_unit_id) = board_world(&database);
    assert!(world.issue_join_order(1, source_id, target_id, None));

    world.update_entities_with_gameplay(0.05, &gameplay);
    let board = world.get_squad(source_id).unwrap().board_state().unwrap();
    assert_eq!(
        world.get_squad(source_id).unwrap().join_kind(),
        Some(JoinKind::Board)
    );
    assert_eq!(
        world.get_squad(source_id).unwrap().join_merge_type(),
        Some(JoinMergeType::Ground)
    );
    assert!(!board.is_complete());
    assert_eq!(board.former_owner(), 2);
    assert!(world.get_unit(target_unit_id).unwrap().is_being_boarded());
    assert!(!world.get_unit(target_unit_id).unwrap().is_attackable());
    assert_eq!(world.get_squad(target_id).unwrap().base.player_id, 2);

    world.update_entities_with_gameplay(0.05, &gameplay);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(
        !world
            .get_squad(source_id)
            .unwrap()
            .board_state()
            .unwrap()
            .is_complete()
    );
    world.update_entities_with_gameplay(0.05, &gameplay);

    let source = world.get_squad(source_id).unwrap();
    let board = source.board_state().unwrap();
    assert!(board.is_complete());
    assert!(board.veterancy_override());
    assert_eq!(board.levels(), 1);
    assert_eq!(board.source_veterancy_level(), 2);
    assert_eq!(board.effective_veterancy_bonus(), 3);
    assert!(matches!(
        source.garrison.state(),
        SquadContainmentState::Garrisoned { container, .. } if container == target_unit_id
    ));
    assert_eq!(world.get_squad(target_id).unwrap().base.player_id, 1);
    assert_eq!(world.get_unit(target_unit_id).unwrap().base.player_id, 1);
    assert!(!world.get_unit(target_unit_id).unwrap().is_being_boarded());
    assert!(world.get_unit(source_unit_id).unwrap().is_invulnerable());
    assert_eq!(
        world
            .get_unit(source_unit_id)
            .unwrap()
            .garrison
            .container_id(),
        Some(target_unit_id)
    );
    assert!(
        world
            .get_unit(target_unit_id)
            .unwrap()
            .garrison
            .contained_unit_ids()
            .contains(&source_unit_id)
    );
    let boarded_target = world.get_unit(target_unit_id).unwrap();
    assert!(nearly_equal(
        boarded_target.data_scalar(UnitDataScalar::Damage),
        1.5
    ));
    assert!(nearly_equal(
        boarded_target.effective_damage_multiplier(),
        1.725
    ));
    assert!(nearly_equal(
        boarded_target.data_scalar(UnitDataScalar::DamageTaken),
        0.72
    ));
    assert!(nearly_equal(
        boarded_target.effective_damage_taken_multiplier(),
        0.6264
    ));
    let attachment_id = board
        .attachment_entity_id()
        .expect("completed Board creates its authored attachment");
    assert_eq!(board.attachment_proto_object_name(), Some("hijacked_fx"));
    let attachment = world.get_object(attachment_id).unwrap();
    assert_eq!(attachment.proto_object_name, "hijacked_fx");
    assert_eq!(attachment.object_state.attached_to(), Some(target_unit_id));
}

#[test]
fn destroyed_boarded_target_releases_and_damages_the_spartan() {
    let (database, gameplay) = board_gameplay(0.0);
    let (mut world, source_id, target_id, source_unit_id, _) = board_world(&database);
    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(
        world
            .get_squad(source_id)
            .unwrap()
            .board_state()
            .unwrap()
            .is_complete()
    );
    let attachment_id = world
        .get_squad(source_id)
        .unwrap()
        .board_state()
        .unwrap()
        .attachment_entity_id()
        .unwrap();

    assert!(world.kill_squad(target_id, true));

    let source = world.get_squad(source_id).unwrap();
    assert!(source.board_state().is_none());
    assert_eq!(source.join_target(), None);
    assert_eq!(source.garrison.state(), SquadContainmentState::Free);
    let source_unit = world.get_unit(source_unit_id).unwrap();
    assert!(!source_unit.is_invulnerable());
    assert!(source_unit.base.is_selectable());
    assert!(nearly_equal(source_unit.hitpoints, 50.0));
    assert!(world.get_object(attachment_id).is_none());
}

#[test]
fn target_destroyed_during_boarding_cancels_without_revert_damage() {
    let (database, gameplay) = board_gameplay(1.0);
    let (mut world, source_id, target_id, source_unit_id, target_unit_id) = board_world(&database);
    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_unit(target_unit_id).unwrap().is_being_boarded());

    assert!(world.kill_squad(target_id, true));

    assert!(world.get_squad(source_id).unwrap().board_state().is_none());
    let source_unit = world.get_unit(source_unit_id).unwrap();
    assert!(nearly_equal(source_unit.hitpoints, 100.0));
    assert!(!source_unit.is_invulnerable());
}

fn board_world(database: &Database) -> (World, EntityId, EntityId, EntityId, EntityId) {
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let source_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "spartan_squad",
        database,
    );
    let target_id = create_squad_from_prototype(
        &mut world,
        2,
        Vec3::ZERO,
        Vec3::Z,
        "vehicle_squad",
        database,
    );
    let source_unit_id = world.get_squad(source_id).unwrap().unit_ids[0];
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    (world, source_id, target_id, source_unit_id, target_unit_id)
}

fn board_gameplay(board_time: f32) -> (Database, crate::gameplay::GameplayCatalog) {
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "spartan".to_owned(),
                tactics: Some("spartan.tactics".to_owned()),
                hitpoints: Some(100.0),
                veterancy: vec![
                    VeterancyLevel {
                        level: 1,
                        damage: Some(1.2),
                        damage_taken: Some(0.9),
                        ..VeterancyLevel::default()
                    },
                    VeterancyLevel {
                        level: 2,
                        damage: Some(1.25),
                        damage_taken: Some(0.8),
                        ..VeterancyLevel::default()
                    },
                ],
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "vehicle".to_owned(),
                hitpoints: Some(200.0),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "hijacked_fx".to_owned(),
                dbid: Some(37),
                object_class: Some("Object".to_owned()),
                ..ProtoObject::default()
            },
        ],
        squads: vec![
            proto_squad("spartan_squad", "spartan", 2),
            proto_squad("vehicle_squad", "vehicle", 0),
        ],
        ..Database::default()
    };
    let tactics = TacticData {
        actions: vec![Action {
            name: "TakeOver".to_owned(),
            action_type: Some("Join".to_owned()),
            work_range: Some(1.0),
            join_type: Some(JoinType {
                kind: "Board".to_owned(),
                board_time: Some(board_time),
                revert_damage_pct: Some(0.5),
                unjoin_max_dist: Some(25.0),
                veterancy_override: Some(true),
                levels: Some(1),
                ..JoinType::default()
            }),
            merge_type: Some("Ground".to_owned()),
            damage_modifiers: Some(DamageModifiers {
                damage: Some(1.15),
                damage_taken: Some(0.87),
                ..DamageModifiers::default()
            }),
            proto_object: Some(ProtoObjectRef {
                name: "hijacked_fx".to_owned(),
                ..ProtoObjectRef::default()
            }),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    let gameplay = crate::gameplay::GameplayCatalog::from_tactics(
        &database,
        [("spartan".to_owned(), tactics)],
    );
    (database, gameplay)
}

fn proto_squad(name: &str, member: &str, level: i32) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        level: Some(level),
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

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON * left.abs().max(right.abs()).max(1.0)
}
