use super::*;
use pipeline::database::hw1::tactics::{ProtoObjectRef, TacticData, TacticRules};
use pipeline::database::hw1::{Database, GameData, ProtoObject};

#[test]
fn named_persistent_cloak_joins_object_flags_and_game_data() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "commando".to_owned(),
            flags: vec![
                "AutoCloak".to_owned(),
                "MoveWhileCloaked".to_owned(),
                "AttackWhileCloaked".to_owned(),
            ],
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            cloaking_delay: Some(1.25),
            recloak_delay: Some(4.5),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [("commando".to_owned(), cloak_tactics(true, true))],
    );

    let profile = gameplay.cloak("COMMANDO").unwrap();
    assert_eq!(profile.action_name(), "Stealth");
    assert_eq!(profile.effect_proto_object(), Some("fx_cloak"));
    assert!(profile.starts_disabled());
    assert!(profile.permanent());
    assert!(profile.auto_cloak());
    assert!(profile.move_while_cloaked());
    assert!(profile.attack_while_cloaked());
    assert_eq!(profile.cloaking_delay().to_bits(), 1.25_f32.to_bits());
    assert_eq!(profile.recloak_delay().to_bits(), 4.5_f32.to_bits());
}

#[test]
fn unnamed_or_wrong_type_actions_do_not_become_cloak_profiles() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "unit".to_owned(),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let mut tactics = cloak_tactics(false, false);
    tactics.actions[0].action_type = Some("Wander".to_owned());
    let gameplay = GameplayCatalog::from_tactics(&database, [("unit".to_owned(), tactics)]);

    assert!(gameplay.cloak("unit").is_none());
}

fn cloak_tactics(starts_disabled: bool, permanent: bool) -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "Stealth".to_owned(),
            action_type: Some("Cloak".to_owned()),
            proto_object: Some(ProtoObjectRef {
                name: "fx_cloak".to_owned(),
                ..ProtoObjectRef::default()
            }),
            start_disabled: Some(starts_disabled),
            no_auto_target: Some(permanent),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_squad_actions: vec!["Stealth".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}
