use super::*;
use pipeline::database::hw1::tactics::{TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn squad_persistent_reflect_damage_preserves_retail_fields() {
    let database = database();
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [(
            "arbiter".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: "FiendishReturn".to_owned(),
                    action_type: Some("ReflectDamage".to_owned()),
                    work_rate: Some(0.15),
                    start_disabled: Some(true),
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_squad_actions: vec!["FiendishReturn".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    );

    let profile = gameplay
        .reflect_damage("ARBITER")
        .expect("ReflectDamage profile");
    assert_eq!(profile.action_name(), "FiendishReturn");
    assert_eq!(profile.work_rate().to_bits(), 0.15_f32.to_bits());
    assert!(profile.starts_disabled());
}

#[test]
fn ordinary_or_unit_persistent_reflect_damage_is_excluded() {
    let database = database();
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [(
            "arbiter".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: "FiendishReturn".to_owned(),
                    action_type: Some("ReflectDamage".to_owned()),
                    work_rate: Some(0.15),
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_actions: vec!["FiendishReturn".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    );

    assert!(gameplay.reflect_damage("arbiter").is_none());
}

fn database() -> Database {
    Database {
        objects: vec![ProtoObject {
            name: "arbiter".to_owned(),
            ..ProtoObject::default()
        }],
        ..Database::default()
    }
}
