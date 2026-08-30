use super::*;
use pipeline::database::hw1::tactics::{DamageModifiers, ProtoObjectRef, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn squad_persistent_spirit_bond_preserves_retail_fields() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "hunter".to_owned(),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let action = Action {
        name: "Bond".to_owned(),
        action_type: Some("SpiritBond".to_owned()),
        damage_modifiers: Some(DamageModifiers {
            damage: Some(1.35),
            ..DamageModifiers::default()
        }),
        proto_object: Some(ProtoObjectRef {
            name: "bond_beam".to_owned(),
            ..ProtoObjectRef::default()
        }),
        start_disabled: Some(true),
        ..Action::default()
    };
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [(
            "hunter".to_owned(),
            TacticData {
                actions: vec![action],
                tactic: Some(TacticRules {
                    persistent_squad_actions: vec!["Bond".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    );

    let profile = gameplay.spirit_bond("HUNTER").expect("SpiritBond profile");
    assert_eq!(profile.action_name(), "Bond");
    assert_eq!(profile.damage_modifier().to_bits(), 1.35_f32.to_bits());
    assert_eq!(profile.beam_proto_object(), Some("bond_beam"));
    assert!(profile.starts_disabled());
}

#[test]
fn ordinary_or_unit_persistent_spirit_bond_is_excluded() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "hunter".to_owned(),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [(
            "hunter".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: "Bond".to_owned(),
                    action_type: Some("SpiritBond".to_owned()),
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_actions: vec!["Bond".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    );

    assert!(gameplay.spirit_bond("hunter").is_none());
}
