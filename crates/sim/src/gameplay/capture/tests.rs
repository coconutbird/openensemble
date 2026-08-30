use super::*;
use crate::gameplay::{AttackQueryFlags, TacticRelation};
use pipeline::database::hw1::tactics::{TacticData, TacticRules, TargetRule};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn capture_profile_preserves_authored_fields_and_retail_fallbacks() {
    let database = database();
    let authored = catalog(
        &database,
        Action {
            name: "TakeNode".to_owned(),
            action_type: Some("Capture".to_owned()),
            work_rate: Some(2.5),
            work_range: Some(3.0),
            die_on_built: Some(true),
            start_disabled: Some(true),
            ..Action::default()
        },
        None,
    );
    let profile = &authored.capture_actions("worker")[0];
    assert_eq!(profile.action_name(), "TakeNode");
    assert_eq!(profile.work_rate().to_bits(), 2.5_f32.to_bits());
    assert_eq!(profile.work_range().to_bits(), 3.0_f32.to_bits());
    assert!(profile.die_on_built());
    assert!(profile.starts_disabled());

    let fallback = catalog(
        &database,
        Action {
            name: "Capture".to_owned(),
            action_type: Some("capture".to_owned()),
            work_rate: Some(f32::NAN),
            work_range: Some(-1.0),
            ..Action::default()
        },
        None,
    );
    let profile = &fallback.capture_actions("WORKER")[0];
    assert_eq!(
        profile.work_rate().to_bits(),
        DEFAULT_CAPTURE_WORK_RATE.to_bits()
    );
    assert_eq!(
        profile.work_range().to_bits(),
        DEFAULT_CAPTURE_WORK_RANGE.to_bits()
    );
}

#[test]
fn authored_target_rules_gate_capture_selection() {
    let database = database();
    let catalog = catalog(
        &database,
        Action {
            name: "Capture".to_owned(),
            action_type: Some("Capture".to_owned()),
            ..Action::default()
        },
        Some(TacticRules {
            target_rules: vec![TargetRule {
                action: Some("Capture".to_owned()),
                relation: Some("Any".to_owned()),
                target_states: vec!["Capturable".to_owned()],
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
    );
    let query = AttackQuery {
        relation: TacticRelation::Neutral,
        target_proto_object_name: Some("node"),
        ..AttackQuery::default()
    };
    assert!(
        catalog
            .select_capture_action("worker", &query, |_| true)
            .is_none()
    );
    let mut flags = AttackQueryFlags::empty();
    flags.insert(AttackQueryFlags::TARGET_CAPTURABLE);
    let query = AttackQuery { flags, ..query };
    assert_eq!(
        catalog
            .select_capture_action("worker", &query, |_| true)
            .map(CaptureActionProfile::action_name),
        Some("Capture")
    );
}

fn database() -> Database {
    Database {
        objects: vec![
            ProtoObject {
                name: "worker".to_owned(),
                tactics: Some("worker.tactics".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "node".to_owned(),
                object_types: vec!["Capturable".to_owned()],
                ..ProtoObject::default()
            },
        ],
        ..Database::default()
    }
}

fn catalog(database: &Database, action: Action, tactic: Option<TacticRules>) -> GameplayCatalog {
    GameplayCatalog::from_tactics(
        database,
        [(
            "worker".to_owned(),
            TacticData {
                actions: vec![action],
                tactic,
                ..TacticData::default()
            },
        )],
    )
}
