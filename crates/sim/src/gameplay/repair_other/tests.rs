use super::*;
use crate::gameplay::{AttackQueryFlags, TacticRelation};
use pipeline::database::hw1::tactics::{
    AutoRepair, ProtoObjectRef, TacticData, TacticRules, TargetRule,
};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn profile_preserves_authored_repair_fields_and_defaults() {
    let database = database();
    let authored = catalog(
        &database,
        Action {
            name: "FixVehicle".to_owned(),
            action_type: Some("RepairOther".to_owned()),
            work_rate: Some(0.8),
            work_range: Some(4.0),
            allow_reinforce: Some(true),
            start_disabled: Some(true),
            auto_repair: Some(AutoRepair {
                idle_time: Some(1000),
                threshold: Some(0.9),
                search_distance: Some(400.0),
            }),
            proto_object: Some(ProtoObjectRef {
                name: "fx_repair_beam".to_owned(),
                bone: Some("bone_launchpoint".to_owned()),
                ..ProtoObjectRef::default()
            }),
            ..Action::default()
        },
        None,
    );
    let profile = &authored.repair_other_actions("worker")[0];
    assert_eq!(profile.action_name(), "FixVehicle");
    assert_eq!(profile.work_rate().to_bits(), 0.8_f32.to_bits());
    assert_eq!(profile.work_range().to_bits(), 4.0_f32.to_bits());
    assert!(profile.allow_reinforce());
    assert!(profile.starts_disabled());
    assert_eq!(profile.auto_repair().unwrap().idle_time_ms(), 1000);
    assert_eq!(
        profile.auto_repair().unwrap().threshold().to_bits(),
        0.9_f32.to_bits()
    );
    assert_eq!(
        profile.auto_repair().unwrap().search_distance().to_bits(),
        400.0_f32.to_bits()
    );
    assert_eq!(profile.effect_proto_object(), Some("fx_repair_beam"));
    assert_eq!(profile.effect_bone(), Some("bone_launchpoint"));

    let fallback = catalog(
        &database,
        Action {
            name: "RepairOther".to_owned(),
            action_type: Some("repairother".to_owned()),
            work_rate: Some(f32::NAN),
            work_range: Some(-1.0),
            ..Action::default()
        },
        None,
    );
    let profile = &fallback.repair_other_actions("WORKER")[0];
    assert_eq!(
        profile.work_rate().to_bits(),
        DEFAULT_REPAIR_WORK_RATE.to_bits()
    );
    assert_eq!(
        profile.work_range().to_bits(),
        DEFAULT_REPAIR_WORK_RANGE.to_bits()
    );
    assert!(!profile.allow_reinforce());
}

#[test]
fn target_rules_require_an_allied_damaged_vehicle() {
    let database = database();
    let catalog = catalog(
        &database,
        Action {
            name: "RepairOther".to_owned(),
            action_type: Some("RepairOther".to_owned()),
            ..Action::default()
        },
        Some(TacticRules {
            target_rules: vec![TargetRule {
                action: Some("RepairOther".to_owned()),
                relation: Some("Ally".to_owned()),
                target_types: vec!["GroundVehicle".to_owned()],
                target_states: vec!["Damaged".to_owned()],
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
    );
    let query = AttackQuery {
        relation: TacticRelation::SelfPlayer,
        target_proto_object_name: Some("vehicle"),
        ..AttackQuery::default()
    };
    assert!(
        catalog
            .select_repair_other_action("worker", &query, |_| true)
            .is_none()
    );
    let mut flags = AttackQueryFlags::empty();
    flags.insert(AttackQueryFlags::TARGET_DAMAGED);
    let query = AttackQuery { flags, ..query };
    assert_eq!(
        catalog
            .select_repair_other_action("worker", &query, |_| true)
            .map(RepairOtherActionProfile::action_name),
        Some("RepairOther")
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
                name: "vehicle".to_owned(),
                object_types: vec!["GroundVehicle".to_owned()],
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
