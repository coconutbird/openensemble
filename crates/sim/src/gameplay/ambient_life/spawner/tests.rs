use super::*;
use pipeline::database::hw1::ProtoObject;
use pipeline::database::hw1::tactics::{TacticData, TacticRules};

#[test]
fn persistent_spawner_joins_squad_type_and_float_frequency() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "tree_spawner".to_owned(),
            tactics: Some("tree_spawner.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            al_spawner_check_frequency: Some(0.1259),
            al_opp_check_radius: Some(23.0),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let gameplay = catalog(&database, "AmbientLifeSpawner", Some("bird"), true);

    let profile = gameplay
        .ambient_life_spawner("TREE_SPAWNER")
        .expect("spawner profile");
    assert_eq!(profile.action_name(), "SpawnBird");
    assert_eq!(profile.squad_type(), "bird");
    assert_eq!(profile.check_frequency().to_bits(), 0.125_f32.to_bits());
    assert_eq!(
        profile.opportunity_check_radius().to_bits(),
        23.0_f32.to_bits()
    );
    assert!(profile.starts_disabled());
}

#[test]
fn spawner_requires_named_persistent_action_and_squad_type() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "tree_spawner".to_owned(),
            tactics: Some("tree_spawner.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };

    let wrong_type = catalog(&database, "Move", Some("bird"), false);
    assert!(wrong_type.ambient_life_spawner("tree_spawner").is_none());
    let missing_squad = catalog(&database, "AmbientLifeSpawner", None, false);
    assert!(missing_squad.ambient_life_spawner("tree_spawner").is_none());
}

fn catalog(
    database: &Database,
    action_type: &str,
    squad_type: Option<&str>,
    starts_disabled: bool,
) -> GameplayCatalog {
    GameplayCatalog::from_tactics(
        database,
        [(
            "tree_spawner".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: "SpawnBird".to_owned(),
                    action_type: Some(action_type.to_owned()),
                    squad_type: squad_type.map(str::to_owned),
                    start_disabled: Some(starts_disabled),
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_actions: vec!["SpawnBird".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    )
}
