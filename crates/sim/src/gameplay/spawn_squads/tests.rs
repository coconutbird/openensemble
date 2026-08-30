use super::*;
use pipeline::database::hw1::tactics::{Action, AnimationRef, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn persistent_spawn_profiles_preserve_retail_action_fields() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "spawner".to_owned(),
            tactics: Some("spawner.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let persistent = Action {
        name: "SpawnEgg".to_owned(),
        action_type: Some("SpawnSquad".to_owned()),
        squad_type: Some("egg_squad".to_owned()),
        anim: Some(AnimationRef {
            name: "ReleaseEgg".to_owned(),
            ..AnimationRef::default()
        }),
        work_rate: Some(10.0),
        work_rate_variance: Some(4.0),
        count: Some(3),
        stationary: Some(true),
        auto_join: Some(true),
        hide_spawn_until_release: Some(true),
        start_disabled: Some(true),
        ..Action::default()
    };
    let ordinary = Action {
        name: "UnusedSpawn".to_owned(),
        action_type: Some("SpawnSquad".to_owned()),
        squad_type: Some("unused".to_owned()),
        ..Action::default()
    };
    let tactics = TacticData {
        actions: vec![persistent, ordinary],
        tactic: Some(TacticRules {
            persistent_actions: vec!["SpawnEgg".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };

    let catalog = GameplayCatalog::from_tactics(&database, [("spawner".to_owned(), tactics)]);
    let profiles = catalog.persistent_squad_spawns("SPAWNER");

    assert_eq!(profiles.len(), 1);
    let profile = &profiles[0];
    assert_eq!(profile.action_name(), "SpawnEgg");
    assert_eq!(profile.squad_type(), "egg_squad");
    assert_eq!(profile.animation(), Some("ReleaseEgg"));
    assert_eq!(profile.work_rate().to_bits(), 10.0_f32.to_bits());
    assert_eq!(profile.work_rate_variance().to_bits(), 4.0_f32.to_bits());
    assert_eq!(profile.count(), 3);
    assert!(profile.stationary());
    assert!(profile.auto_join());
    assert!(profile.hide_until_release());
    assert!(profile.starts_disabled());
}

#[test]
fn typed_persistent_spawn_does_not_require_a_named_rule() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "spawner".to_owned(),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };
    let action = Action {
        name: "TypedSpawn".to_owned(),
        action_type: Some("NoOp".to_owned()),
        persistent_action_type: Some("SpawnSquad".to_owned()),
        squad_type: Some("child".to_owned()),
        ..Action::default()
    };
    let tactics = TacticData {
        actions: vec![action],
        ..TacticData::default()
    };

    let catalog = GameplayCatalog::from_tactics(&database, [("spawner".to_owned(), tactics)]);

    assert_eq!(catalog.persistent_squad_spawns("spawner").len(), 1);
}
