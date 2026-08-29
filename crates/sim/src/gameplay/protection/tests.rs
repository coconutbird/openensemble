use super::*;
use crate::gameplay::GameplayCatalog;
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{
    Action, ActionDuration, JoinType, ProtoObjectRef, TacticData, TacticRules, TargetRule,
};
use pipeline::database::hw1::{ProtoObject, Squad};

#[test]
fn persistent_plasma_action_joins_to_its_shield_prototype() {
    let mut database = Database::new();
    database.objects = vec![
        ProtoObject {
            name: "generator".to_owned(),
            tactics: Some("generator.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "main_shield".to_owned(),
            build_points: Some(30.0),
            shieldpoints: Some(5_000.0),
            ..ProtoObject::default()
        },
    ];
    let tactics = TacticData {
        actions: vec![Action {
            name: "PersistentShield".to_owned(),
            action_type: Some("PlasmaShieldGen".to_owned()),
            duration: Some(ActionDuration {
                seconds: 5.0,
                ..ActionDuration::default()
            }),
            deflect_timeout: Some(12.0),
            proto_object: Some(ProtoObjectRef {
                name: "MAIN_SHIELD".to_owned(),
                ..ProtoObjectRef::default()
            }),
            count: Some(25_136),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_actions: vec!["PersistentShield".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };

    let catalog = GameplayCatalog::from_tactics(&database, [("generator".to_owned(), tactics)]);
    let profile = catalog
        .plasma_shield_generator("GENERATOR")
        .expect("persistent generator profile");

    assert_eq!(profile.generator_proto_object_name(), "generator");
    assert_eq!(profile.shield_proto_object_name(), "main_shield");
    assert_eq!(profile.shield_proto_object_index(), 1);
    assert!(nearly_equal(profile.rebuild_time(), 30.0));
    assert!(nearly_equal(profile.under_attack_wait(), 5.0));
    assert!(nearly_equal(profile.deflect_timeout(), 12.0));
    assert_eq!(profile.recharge_text_id(), Some(25_136));
}

#[test]
fn only_named_persistent_plasma_actions_are_collected() {
    let mut database = Database::new();
    database.objects = vec![
        ProtoObject {
            name: "generator".to_owned(),
            tactics: Some("generator.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "shield".to_owned(),
            ..ProtoObject::default()
        },
    ];
    let tactics = TacticData {
        actions: vec![Action {
            name: "NotPersistent".to_owned(),
            action_type: Some("PlasmaShieldGen".to_owned()),
            proto_object: Some(ProtoObjectRef {
                name: "shield".to_owned(),
                ..ProtoObjectRef::default()
            }),
            ..Action::default()
        }],
        ..TacticData::default()
    };

    let catalog = GameplayCatalog::from_tactics(&database, [("generator".to_owned(), tactics)]);

    assert!(catalog.plasma_shield_generator("generator").is_none());
}

#[test]
fn persistent_bubble_action_pairs_with_command_follow_join() {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "monitor".to_owned(),
        tactics: Some("monitor.tactics".to_owned()),
        ..ProtoObject::default()
    });
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

    let catalog = GameplayCatalog::from_tactics(&database, [("monitor".to_owned(), tactics)]);
    let profile = catalog
        .bubble_shield_action("MONITOR")
        .expect("persistent BubbleShield owner");

    assert_eq!(profile.owner_proto_object_name(), "monitor");
    assert_eq!(profile.join_action_name(), "Join");
    assert!(nearly_equal(profile.work_range(), 5.0));
    assert_eq!(profile.merge_type(), Some("Air"));
}

#[test]
fn raw_bubble_table_preserves_default_and_target_overrides() {
    let mut database = Database::new();
    database.objects = ["small_object", "large_object"]
        .into_iter()
        .map(|name| ProtoObject {
            name: name.to_owned(),
            ..ProtoObject::default()
        })
        .collect();
    database.squads = vec![
        Squad {
            name: "target".to_owned(),
            ..Squad::default()
        },
        bubble_squad("small", "small_object"),
        bubble_squad("large", "large_object"),
    ];
    let document = Document::from_xml(
        r#"<Squads><ShieldBubbleTypes>small
            <ShieldBubble target="target">large</ShieldBubble>
        </ShieldBubbleTypes></Squads>"#,
    )
    .expect("valid squad XML");

    let types = ShieldBubbleTypes::from_document(&database, &document);

    assert_eq!(
        types
            .default_squad()
            .map(BubbleShieldSquadProfile::proto_squad_name),
        Some("small")
    );
    assert_eq!(
        types
            .resolve("TARGET")
            .map(BubbleShieldSquadProfile::proto_squad_name),
        Some("large")
    );
    assert_eq!(
        types
            .resolve("unknown")
            .map(BubbleShieldSquadProfile::proto_squad_name),
        Some("small")
    );
}

fn bubble_squad(name: &str, member: &str) -> Squad {
    Squad {
        name: name.to_owned(),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: member.to_owned(),
                count: 1,
                ..UnitEntry::default()
            }],
        }),
        ..Squad::default()
    }
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON * left.abs().max(right.abs()).max(1.0) * 8.0
}
