use super::*;
use pipeline::database::hw1::tactics::{TacticData, TacticRules};

#[test]
fn named_persistent_wander_uses_retail_default_range() {
    let catalog = catalog_with(Action {
        name: "WanderAction".to_owned(),
        action_type: Some("Wander".to_owned()),
        start_disabled: Some(true),
        ..Action::default()
    });

    let profile = catalog.wander("SPORE").expect("persistent Wander profile");
    assert_eq!(profile.action_name(), "WanderAction");
    assert_eq!(profile.work_range().to_bits(), DEFAULT_WORK_RANGE.to_bits());
    assert!(profile.starts_disabled());
}

#[test]
fn wrong_or_unlisted_actions_do_not_become_wander_profiles() {
    let mut action = Action {
        name: "WanderAction".to_owned(),
        action_type: Some("Move".to_owned()),
        ..Action::default()
    };
    assert!(catalog_with(action.clone()).wander("spore").is_none());
    action.action_type = Some("Wander".to_owned());
    action.name = "DifferentAction".to_owned();
    assert!(catalog_with(action).wander("spore").is_none());
}

fn catalog_with(action: Action) -> GameplayCatalog {
    let database = pipeline::database::hw1::Database {
        objects: vec![pipeline::database::hw1::ProtoObject {
            name: "spore".to_owned(),
            tactics: Some("spore.tactics".to_owned()),
            ..pipeline::database::hw1::ProtoObject::default()
        }],
        ..pipeline::database::hw1::Database::default()
    };
    GameplayCatalog::from_tactics(
        &database,
        [(
            "spore".to_owned(),
            TacticData {
                actions: vec![action],
                tactic: Some(TacticRules {
                    persistent_squad_actions: vec!["WanderAction".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    )
}
