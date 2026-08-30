use super::*;
use pipeline::database::hw1::tactics::{ActionDuration, AnimationRef, Weapon};

#[test]
fn jump_profiles_preserve_retail_action_fields() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "Brute".to_owned(),
            flags: vec!["AbilityDisabled".to_owned()],
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let gameplay =
        GameplayCatalog::from_tactics(&database, [("Brute".to_owned(), brute_jump_tactics())]);
    let profiles = gameplay.jump_actions("BRUTE");

    assert_eq!(profiles.len(), 4);
    assert_eq!(profiles[0].kind(), JumpOrderType::Jump);
    assert_eq!(profiles[1].kind(), JumpOrderType::Gather);
    assert_eq!(profiles[2].kind(), JumpOrderType::Garrison);
    assert_eq!(profiles[3].kind(), JumpOrderType::Attack);
    for profile in profiles {
        assert_eq!(profile.max_distance().to_bits(), 175.0_f32.to_bits());
        assert_eq!(profile.weapon_name(), Some("BruteGun"));
        assert_eq!(profile.weapon_max_range().to_bits(), 35.0_f32.to_bits());
        assert_eq!(profile.animation_type(), Some("JumpWork"));
        assert!(profile.starts_disabled());
        assert!(profile.ability_starts_disabled());
    }
    assert_eq!(profiles[0].velocity_scalar().to_bits(), 1.0_f32.to_bits());
    assert_eq!(profiles[3].velocity_scalar().to_bits(), 40.0_f32.to_bits());
}

#[test]
fn missing_duration_has_zero_range_and_jump_pull_is_not_voluntary() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "Brute".to_owned(),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let tactics = TacticData {
        actions: vec![
            Action {
                name: "Jump".to_owned(),
                action_type: Some("Jump".to_owned()),
                ..Action::default()
            },
            Action {
                name: "JumpPull".to_owned(),
                action_type: Some("JumpPull".to_owned()),
                ..Action::default()
            },
        ],
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("Brute".to_owned(), tactics)]);

    let profiles = gameplay.jump_actions("Brute");
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].max_distance().to_bits(), 0.0_f32.to_bits());
}

fn brute_jump_tactics() -> TacticData {
    TacticData {
        weapons: vec![Weapon {
            name: "BruteGun".to_owned(),
            max_range: Some(35.0),
            ..Weapon::default()
        }],
        actions: [
            ("Jump", "Jump", None),
            ("JumpGather", "JumpGather", None),
            ("JumpGarrison", "JumpGarrison", None),
            ("JumpAttack", "JumpAttack", Some(40.0)),
        ]
        .into_iter()
        .map(|(name, action_type, velocity_scalar)| Action {
            name: name.to_owned(),
            action_type: Some(action_type.to_owned()),
            weapon: Some("BruteGun".to_owned()),
            anim: Some(AnimationRef {
                name: "JumpWork".to_owned(),
                ..AnimationRef::default()
            }),
            duration: Some(ActionDuration {
                seconds: 175.0,
                ..ActionDuration::default()
            }),
            velocity_scalar,
            start_disabled: Some(true),
            ..Action::default()
        })
        .collect(),
        ..TacticData::default()
    }
}
