use super::*;
use crate::entities::JoinKind;
use crate::scenario::create_squad_from_prototype;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, JoinType, TacticData, TacticRules, TargetRule};
use pipeline::database::hw1::{Database, GameData, ProtoObject, Squad as ProtoSquad};
use pipeline::xmb::Document;

#[test]
fn follow_join_creates_tracks_rebuilds_and_removes_bubble() {
    let (database, gameplay) = bubble_gameplay();
    let mut world = World::new();
    world.init_players(1);
    let target_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "target_squad",
        &database,
    );
    let source_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::Z,
        "monitor_squad",
        &database,
    );

    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.advance_time(50);
    world.update_entities_with_gameplay(0.05, &gameplay);

    let shield_id = world
        .get_squad(source_id)
        .and_then(Squad::bubble_shield_squad)
        .expect("joined monitor owns a bubble");
    let shield_unit_id = world.get_squad(shield_id).unwrap().unit_ids[0];
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    assert_eq!(
        world.get_squad(target_id).unwrap().damage_proxy(),
        Some(shield_id)
    );
    assert!(world.get_unit(shield_unit_id).unwrap().shields.current > 0.1);
    assert!(
        world
            .get_unit(world.get_squad(source_id).unwrap().unit_ids[0])
            .unwrap()
            .is_invulnerable()
    );

    assert!(world.teleport_squad(target_id, Vec3::new(20.0, 0.0, 5.0)));
    world.advance_time(50);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(
        world.get_squad(shield_id).unwrap().base.position,
        Vec3::new(20.0, 0.0, 5.0)
    );

    assert!(world.damage_unit(target_unit_id, 100.0));
    world.advance_time(50);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_squad(shield_id).is_none());
    assert_eq!(world.get_squad(target_id).unwrap().damage_proxy(), None);
    assert_eq!(
        world.get_squad(source_id).unwrap().bubble_shield_squad(),
        None
    );

    world.advance_time(1_950);
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(
        world.get_squad(source_id).unwrap().bubble_shield_squad(),
        None
    );
    world.advance_time(1);
    world.update_entities_with_gameplay(0.05, &gameplay);
    let rebuilt_id = world
        .get_squad(source_id)
        .and_then(Squad::bubble_shield_squad)
        .expect("strict post-damage delay elapsed");
    assert_ne!(rebuilt_id, shield_id);

    assert!(world.kill_squad(target_id, true));
    assert!(world.get_squad(target_id).is_none());
    assert!(world.get_squad(source_id).is_none());
    assert!(world.get_squad(rebuilt_id).is_none());
}

#[test]
fn join_waits_for_work_range_before_connecting() {
    let (database, gameplay) = bubble_gameplay();
    let mut world = World::new();
    world.init_players(1);
    let target_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::new(20.0, 0.0, 0.0),
        Vec3::Z,
        "target_squad",
        &database,
    );
    let source_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "monitor_squad",
        &database,
    );

    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(!world.get_squad(source_id).unwrap().join_is_connected());
    assert_eq!(
        world.get_squad(source_id).unwrap().bubble_shield_squad(),
        None
    );

    world.update_entities_with_gameplay(2.0, &gameplay);
    let source = world.get_squad(source_id).unwrap();
    assert!(source.join_is_connected());
    assert!((source.speed - 24.0).abs() < f32::EPSILON);
    let source_unit = world.get_unit(source.unit_ids[0]).unwrap();
    assert!((source_unit.velocity_scalar - 1.2).abs() < f32::EPSILON);
    assert!(source.bubble_shield_squad().is_some());
}

#[test]
fn join_work_range_is_measured_between_obstruction_surfaces() {
    let (database, gameplay) = bubble_gameplay();
    let mut world = World::new();
    world.init_players(1);
    let target_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::Z,
        "target_squad",
        &database,
    );
    let source_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "monitor_squad",
        &database,
    );
    for squad_id in [source_id, target_id] {
        let unit_id = world.get_squad(squad_id).unwrap().unit_ids[0];
        world
            .get_unit_mut(unit_id)
            .unwrap()
            .obstruction_half_extents = Vec3::splat(3.0);
    }

    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(world.get_squad(source_id).unwrap().join_is_connected());
}

#[test]
fn initial_bubble_ignores_target_damage_that_predates_the_join() {
    let (database, gameplay) = bubble_gameplay();
    let mut world = World::new();
    world.init_players(1);
    let target_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "target_squad",
        &database,
    );
    let source_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "monitor_squad",
        &database,
    );
    world.advance_time(100);
    let target_unit_id = world.get_squad(target_id).unwrap().unit_ids[0];
    assert!(world.damage_unit(target_unit_id, 1.0));

    assert!(world.issue_join_order(1, source_id, target_id, None));
    world.update_entities_with_gameplay(0.05, &gameplay);

    assert!(
        world
            .get_squad(source_id)
            .unwrap()
            .bubble_shield_squad()
            .is_some()
    );
    let source_unit_id = world.get_squad(source_id).unwrap().unit_ids[0];
    assert!(!world.get_unit(source_unit_id).unwrap().is_attackable());
}

#[test]
fn follow_attack_mirrors_only_the_targets_validated_attack() {
    let (database, gameplay) = follow_attack_gameplay();
    let mut world = World::new();
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let target_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::ZERO,
        Vec3::Z,
        "target_squad",
        &database,
    );
    let source_id = create_squad_from_prototype(
        &mut world,
        1,
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::Z,
        "monitor_squad",
        &database,
    );
    let enemy_id = create_squad_from_prototype(
        &mut world,
        2,
        Vec3::new(10.0, 0.0, 0.0),
        -Vec3::Z,
        "target_squad",
        &database,
    );
    assert!(world.issue_attack_order(1, target_id, enemy_id, 20.0));
    assert!(world.issue_join_order(1, source_id, target_id, None));

    world.update_entities_with_gameplay(0.55, &gameplay);

    let source = world.get_squad(source_id).unwrap();
    assert!(source.join_is_connected());
    assert_eq!(source.join_kind(), Some(JoinKind::FollowAttack));
    assert_eq!(source.attack_target, Some(enemy_id));
    assert_eq!(source.bubble_shield_squad(), None);
    assert!(
        world
            .get_unit(source.unit_ids[0])
            .unwrap()
            .is_invulnerable()
    );

    world.get_squad_mut(target_id).unwrap().clear_attack_order();
    world.update_entities_with_gameplay(0.55, &gameplay);
    assert_eq!(world.get_squad(source_id).unwrap().attack_target, None);
}

fn bubble_gameplay() -> (Database, GameplayCatalog) {
    let mut database = Database::new();
    database.game_data = Some(GameData {
        shield_regen_delay: Some(2.0),
        shield_regen_time: Some(1.0),
        ..GameData::default()
    });
    database.objects = vec![
        ProtoObject {
            name: "monitor".to_owned(),
            tactics: Some("monitor.tactics".to_owned()),
            hitpoints: Some(10.0),
            max_velocity: Some(20.0),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "target".to_owned(),
            hitpoints: Some(100.0),
            max_velocity: Some(12.0),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "bubble".to_owned(),
            hitpoints: Some(0.1),
            shieldpoints: Some(10.0),
            damage_type: Some("Shielded".to_owned()),
            flags: vec!["ExternalShield".to_owned()],
            ..ProtoObject::default()
        },
    ];
    database.squads = vec![
        proto_squad("monitor_squad", "monitor"),
        proto_squad("target_squad", "target"),
        proto_squad("bubble_squad", "bubble"),
    ];
    let tactics = TacticData {
        actions: vec![
            Action {
                name: "Join".to_owned(),
                action_type: Some("Join".to_owned()),
                work_range: Some(5.0),
                join_type: Some(JoinType {
                    kind: "Follow".to_owned(),
                    ..JoinType::default()
                }),
                merge_type: Some("Air".to_owned()),
                ..Action::default()
            },
            Action {
                name: "Shield".to_owned(),
                action_type: Some("BubbleShield".to_owned()),
                ..Action::default()
            },
        ],
        tactic: Some(TacticRules {
            persistent_actions: vec!["Shield".to_owned()],
            target_rules: vec![TargetRule {
                ability: Some("Command".to_owned()),
                action: Some("Join".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let mut gameplay = GameplayCatalog::from_tactics(&database, [("monitor".to_owned(), tactics)]);
    gameplay.load_test_shield_bubble_document(
        &database,
        &Document::from_xml("<Squads><ShieldBubbleTypes>bubble_squad</ShieldBubbleTypes></Squads>")
            .unwrap(),
    );
    (database, gameplay)
}

fn follow_attack_gameplay() -> (Database, GameplayCatalog) {
    let mut database = Database::new();
    database.objects = vec![
        ProtoObject {
            name: "monitor".to_owned(),
            tactics: Some("monitor.tactics".to_owned()),
            hitpoints: Some(10.0),
            max_velocity: Some(20.0),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "target".to_owned(),
            hitpoints: Some(100.0),
            max_velocity: Some(12.0),
            ..ProtoObject::default()
        },
    ];
    database.squads = vec![
        proto_squad("monitor_squad", "monitor"),
        proto_squad("target_squad", "target"),
    ];
    let tactics = TacticData {
        actions: vec![Action {
            name: "Join".to_owned(),
            action_type: Some("Join".to_owned()),
            work_range: Some(5.0),
            join_type: Some(JoinType {
                kind: "FollowAttack".to_owned(),
                ..JoinType::default()
            }),
            merge_type: Some("Air".to_owned()),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("monitor".to_owned(), tactics)]);
    (database, gameplay)
}

fn proto_squad(name: &str, member: &str) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
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
