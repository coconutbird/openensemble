use super::*;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn persistent_heal_fields_are_materialized_in_authored_order() {
    let database = database();
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [("medic".to_owned(), tactic(true, Some("MedicHeal")))],
    );

    let profile = gameplay.heal("MEDIC").expect("persistent Heal profile");
    assert_eq!(profile.action_name(), "MedicHeal");
    assert_eq!(profile.work_rate().to_bits(), 50.0_f32.to_bits());
    assert_eq!(profile.min_idle_duration_ms(), 3_250);
    assert!(profile.allow_reinforce());
    assert!(profile.heal_target());
    assert!(profile.starts_disabled());
}

#[test]
fn nonpersistent_and_invalid_heal_values_do_not_create_live_work() {
    let database = database();
    let nonpersistent =
        GameplayCatalog::from_tactics(&database, [("medic".to_owned(), tactic(false, None))]);
    assert!(nonpersistent.heal("medic").is_none());

    let invalid = GameplayCatalog::from_tactics(
        &database,
        [("medic".to_owned(), tactic(false, Some("MedicHeal")))],
    );
    let profile = invalid.heal("medic").unwrap();
    assert_eq!(profile.work_rate().to_bits(), 0.0_f32.to_bits());
    assert_eq!(profile.min_idle_duration_ms(), 0);
    assert!(!profile.allow_reinforce());
    assert!(!profile.heal_target());
    assert!(!profile.starts_disabled());
}

fn tactic(authored: bool, persistent: Option<&str>) -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "MedicHeal".to_owned(),
            action_type: Some("Heal".to_owned()),
            work_rate: Some(if authored { 50.0 } else { f32::NAN }),
            min_idle_duration: Some(if authored { 3.25 } else { -2.0 }),
            allow_reinforce: authored.then_some(true),
            heal_target: authored.then_some(true),
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
            name: "medic".to_owned(),
            tactics: Some("medic.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    }
}
