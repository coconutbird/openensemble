use super::*;
use pipeline::database::hw1::ProtoObject;
use pipeline::database::hw1::tactics::{ActionDuration, ProtoObjectRef, TacticData, TacticRules};

#[test]
fn persistent_shield_profiles_preserve_external_and_infantry_inputs() {
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "shield_fx".to_owned(),
                dbid: Some(71),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "carrier".to_owned(),
                ..ProtoObject::default()
            },
        ],
        ..Database::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("carrier".to_owned(), tactics())]);

    let profiles = catalog.energy_shield_actions("CARRIER");
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].action_name(), "ShieldAction");
    assert!(!profiles[0].starts_disabled());
    assert_eq!(
        profiles[0].visual(),
        &EnergyShieldVisualProfile::Attachment {
            prototype_name: "shield_fx".to_owned(),
            prototype_id: Some(71),
            bone_name: Some("bone_chair".to_owned()),
        }
    );
    assert_eq!(profiles[1].action_name(), "InfantryShield");
    assert!(profiles[1].starts_disabled());
    assert_eq!(
        profiles[1].visual(),
        &EnergyShieldVisualProfile::Infantry {
            component_name: "Shield".to_owned(),
            hit_duration_ms: 1_250,
        }
    );
    assert!(
        catalog
            .energy_shield_action("carrier", "DecorativeShield")
            .is_none()
    );
}

fn tactics() -> TacticData {
    TacticData {
        actions: vec![
            Action {
                name: "ShieldAction".to_owned(),
                action_type: Some("EnergyShield".to_owned()),
                proto_object: Some(ProtoObjectRef {
                    name: "shield_fx".to_owned(),
                    bone: Some("bone_chair".to_owned()),
                    ..ProtoObjectRef::default()
                }),
                ..Action::default()
            },
            Action {
                name: "InfantryShield".to_owned(),
                action_type: Some("InfantryEnergyShield".to_owned()),
                duration: Some(ActionDuration {
                    seconds: 1.25,
                    ..ActionDuration::default()
                }),
                start_disabled: Some(true),
                ..Action::default()
            },
            Action {
                name: "DecorativeShield".to_owned(),
                action_type: Some("EnergyShield".to_owned()),
                proto_object: Some(ProtoObjectRef {
                    name: "shield_fx".to_owned(),
                    ..ProtoObjectRef::default()
                }),
                ..Action::default()
            },
        ],
        tactic: Some(TacticRules {
            persistent_actions: vec!["ShieldAction".to_owned(), "InfantryShield".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}
