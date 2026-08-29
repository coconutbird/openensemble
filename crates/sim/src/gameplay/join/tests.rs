use super::*;
use pipeline::database::hw1::tactics::{
    DamageModifiers, JoinType, TacticData, TacticRules, TargetRule,
};
use pipeline::database::hw1::{Database, ProtoObject, Squad};
use pipeline::xmb::Document;

#[test]
fn target_rules_select_distinct_merge_and_board_profiles() {
    let database = database();
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("spartan".to_owned(), spartan_tactics())]);
    let ally = AttackQuery {
        relation: super::super::TacticRelation::Ally,
        target_proto_object_name: Some("infantry"),
        ..AttackQuery::default()
    };
    let merge = gameplay
        .select_join_action("spartan", &ally, |_| true)
        .expect("ally infantry merge");
    assert_eq!(merge.kind(), JoinKind::Merge);
    assert_eq!(merge.merge_type(), JoinMergeType::Ground);
    assert_eq!(merge.action_name(), "InfantryJoin");
    assert_eq!(merge.damage_modifier().to_bits(), 1.4_f32.to_bits());

    let enemy = AttackQuery {
        relation: super::super::TacticRelation::Enemy,
        target_proto_object_name: Some("vehicle"),
        ..AttackQuery::default()
    };
    let board = gameplay
        .select_join_action("spartan", &enemy, |_| true)
        .expect("enemy vehicle board");
    assert_eq!(board.kind(), JoinKind::Board);
    assert_eq!(board.board_time().to_bits(), 8.0_f32.to_bits());
    assert_eq!(board.revert_damage_fraction().to_bits(), 0.5_f32.to_bits());
    assert!(board.veterancy_override());
    assert_eq!(board.board_animation(), Some("HijackIdle"));
    assert_eq!(board.levels(), 2);
}

#[test]
fn missing_or_unknown_join_fields_keep_retail_follow_ground_defaults() {
    let profile = JoinActionProfile::from_action(&Action {
        name: "Join".to_owned(),
        action_type: Some("Join".to_owned()),
        join_type: Some(JoinType {
            kind: "Unexpected".to_owned(),
            ..JoinType::default()
        }),
        merge_type: Some("Unexpected".to_owned()),
        work_range: Some(f32::NAN),
        damage_modifiers: Some(DamageModifiers {
            damage: Some(f32::INFINITY),
            damage_taken: Some(0.75),
            ..DamageModifiers::default()
        }),
        ..Action::default()
    });

    assert_eq!(profile.kind(), JoinKind::Follow);
    assert_eq!(profile.merge_type(), JoinMergeType::Ground);
    assert_eq!(profile.work_range().to_bits(), 0.0_f32.to_bits());
    assert_eq!(profile.damage_modifier().to_bits(), 1.0_f32.to_bits());
    assert_eq!(
        profile.damage_taken_modifier().to_bits(),
        0.75_f32.to_bits()
    );
}

#[test]
fn catalog_can_install_a_scenario_layered_merged_squad_table() {
    let database = Database {
        squads: vec![squad("spartan"), squad("marine")],
        ..Database::default()
    };
    let mut gameplay = GameplayCatalog::from_tactics(&database, []);
    gameplay.load_test_merged_squads_document(
        &database,
        &Document::from_xml(
            "<Squads><MergedSquads>spartan<MergedSquad>marine</MergedSquad>\
             </MergedSquads></Squads>",
        )
        .unwrap(),
    );

    let merged = gameplay
        .merged_squad_profile("SPARTAN", "MARINE")
        .expect("scenario table should resolve case-insensitively");
    assert_eq!(merged.proto_squad_name(), "merged_marine_spartan");
    assert_eq!(merged.proto_squad_id(), 2);
}

fn database() -> Database {
    let mut database = Database::new();
    database.objects = vec![
        ProtoObject {
            name: "spartan".to_owned(),
            tactics: Some("spartan.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "infantry".to_owned(),
            object_types: vec!["Infantry".to_owned()],
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "vehicle".to_owned(),
            object_types: vec!["Vehicle".to_owned()],
            ..ProtoObject::default()
        },
    ];
    database
}

fn spartan_tactics() -> TacticData {
    TacticData {
        actions: vec![
            Action {
                name: "InfantryJoin".to_owned(),
                action_type: Some("Join".to_owned()),
                join_type: Some(JoinType {
                    kind: "Merge".to_owned(),
                    ..JoinType::default()
                }),
                merge_type: Some("Ground".to_owned()),
                damage_modifiers: Some(DamageModifiers {
                    damage: Some(1.4),
                    damage_taken: Some(0.714),
                    ..DamageModifiers::default()
                }),
                ..Action::default()
            },
            Action {
                name: "VehicleTakeOver".to_owned(),
                action_type: Some("Join".to_owned()),
                join_type: Some(JoinType {
                    kind: "Board".to_owned(),
                    revert_damage_pct: Some(0.5),
                    veterancy_override: Some(true),
                    board_time: Some(8.0),
                    board_anim: Some("HijackIdle".to_owned()),
                    levels: Some(2),
                    ..JoinType::default()
                }),
                merge_type: Some("Ground".to_owned()),
                ..Action::default()
            },
        ],
        tactic: Some(TacticRules {
            target_rules: vec![
                TargetRule {
                    relation: Some("Ally".to_owned()),
                    target_types: vec!["Infantry".to_owned()],
                    action: Some("InfantryJoin".to_owned()),
                    ..TargetRule::default()
                },
                TargetRule {
                    relation: Some("Enemy".to_owned()),
                    target_types: vec!["Vehicle".to_owned()],
                    action: Some("VehicleTakeOver".to_owned()),
                    ..TargetRule::default()
                },
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}

fn squad(name: &str) -> Squad {
    Squad {
        name: name.to_owned(),
        ..Squad::default()
    }
}
