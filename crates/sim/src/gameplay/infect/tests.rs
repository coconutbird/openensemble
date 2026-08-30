use super::*;
use pipeline::database::hw1::tactics::{Action, ProtoObjectRef, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn persistent_infect_fields_are_materialized_in_authored_order() {
    let database = database();
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [("spore".to_owned(), tactic(true, Some("InfectAction")))],
    );

    let profile = gameplay.infect("SPORE").expect("persistent Infect profile");
    assert_eq!(profile.action_name(), "InfectAction");
    assert_eq!(profile.work_rate().to_bits(), 10.0_f32.to_bits());
    assert_eq!(profile.work_range().to_bits(), 15.0_f32.to_bits());
    assert_eq!(profile.min_idle_duration_ms(), 7_250);
    assert_eq!(profile.attachment_proto_object(), Some("infection_fx"));
    assert_eq!(profile.invalid_targets(), ["Vehicle", "Hero"]);
    assert!(profile.starts_disabled());
}

#[test]
fn only_named_persistent_infect_actions_are_collected() {
    let database = database();
    let nonpersistent =
        GameplayCatalog::from_tactics(&database, [("spore".to_owned(), tactic(true, None))]);
    assert!(nonpersistent.infect("spore").is_none());

    let invalid = GameplayCatalog::from_tactics(
        &database,
        [("spore".to_owned(), tactic(false, Some("InfectAction")))],
    );
    let profile = invalid.infect("spore").unwrap();
    assert_eq!(profile.work_rate().to_bits(), 0.0_f32.to_bits());
    assert_eq!(profile.work_range().to_bits(), 0.0_f32.to_bits());
    assert_eq!(profile.min_idle_duration_ms(), 0);
    assert_eq!(profile.attachment_proto_object(), None);
    assert!(profile.invalid_targets().is_empty());
    assert!(!profile.starts_disabled());
}

fn tactic(authored: bool, persistent: Option<&str>) -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "InfectAction".to_owned(),
            action_type: Some("Infect".to_owned()),
            work_rate: Some(if authored { 10.0 } else { f32::NAN }),
            work_range: Some(if authored { 15.0 } else { -1.0 }),
            min_idle_duration: Some(if authored { 7.25 } else { -2.0 }),
            proto_object: authored.then(|| ProtoObjectRef {
                name: " infection_fx ".to_owned(),
                ..ProtoObjectRef::default()
            }),
            invalid_targets: if authored {
                vec![" Vehicle ".to_owned(), "Hero".to_owned()]
            } else {
                Vec::new()
            },
            start_disabled: authored.then_some(true),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_actions: persistent.into_iter().map(str::to_owned).collect(),
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "spore".to_owned(),
            tactics: Some("spore.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    }
}
