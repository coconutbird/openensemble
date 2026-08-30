use super::*;
use pipeline::database::hw1::tactics::{TacticData, TacticRules};
use pipeline::database::hw1::{GameData, ProtoObject};

#[test]
fn persistent_ambient_life_joins_action_and_global_settings() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "bird".to_owned(),
            tactics: Some("bird.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        game_data: Some(GameData {
            al_max_wander_frequency: Some(20.9),
            al_predator_check_frequency: Some(2.9),
            al_prey_check_frequency: Some(0.5),
            al_opp_check_radius: Some(20.0),
            al_flee_distance: Some(40.0),
            al_flee_movement_modifier: Some(1.5),
            al_min_wander_distance: Some(30.0),
            al_max_wander_distance: Some(200.0),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let gameplay = catalog(&database, "AmbientLife", "AmbientLife", true);

    let profile = gameplay.ambient_life("BIRD").expect("ambient profile");
    assert_eq!(profile.action_name(), "AmbientLife");
    assert_eq!(profile.max_wander_frequency().to_bits(), 20.0_f32.to_bits());
    assert_eq!(
        profile.predator_check_frequency().to_bits(),
        2.0_f32.to_bits()
    );
    assert_eq!(profile.prey_check_frequency().to_bits(), 0.0_f32.to_bits());
    assert_eq!(
        profile.opportunity_check_radius().to_bits(),
        20.0_f32.to_bits()
    );
    assert_eq!(profile.flee_distance().to_bits(), 40.0_f32.to_bits());
    assert_eq!(
        profile.flee_movement_modifier().to_bits(),
        1.5_f32.to_bits()
    );
    assert_eq!(
        profile.minimum_wander_distance().to_bits(),
        30.0_f32.to_bits()
    );
    assert_eq!(
        profile.maximum_wander_distance().to_bits(),
        200.0_f32.to_bits()
    );
    assert!(profile.starts_disabled());
}

#[test]
fn only_named_persistent_squad_action_is_collected() {
    let database = Database {
        objects: vec![ProtoObject {
            name: "bird".to_owned(),
            tactics: Some("bird.tactics".to_owned()),
            ..ProtoObject::default()
        }],
        ..Database::default()
    };

    assert!(
        catalog(&database, "Move", "AmbientLife", false)
            .ambient_life("bird")
            .is_none()
    );
    assert!(
        catalog(&database, "AmbientLife", "Different", false)
            .ambient_life("bird")
            .is_none()
    );
}

fn catalog(
    database: &Database,
    action_type: &str,
    persistent_name: &str,
    starts_disabled: bool,
) -> GameplayCatalog {
    GameplayCatalog::from_tactics(
        database,
        [(
            "bird".to_owned(),
            TacticData {
                actions: vec![Action {
                    name: "AmbientLife".to_owned(),
                    action_type: Some(action_type.to_owned()),
                    start_disabled: Some(starts_disabled),
                    ..Action::default()
                }],
                tactic: Some(TacticRules {
                    persistent_squad_actions: vec![persistent_name.to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    )
}
