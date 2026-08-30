use super::*;
use pipeline::database::hw1::tactics::{TacticData, TacticRules};

#[test]
fn persistent_bomb_uses_retail_default_and_primary_physics() {
    let catalog = GameplayCatalog::from_tactics(
        &database(Some("egg"), Some("fallback")),
        [("bomb".to_owned(), tactic(Some("Bomb"), None))],
    );

    let profile = catalog.bomb("BOMB").expect("persistent Bomb profile");
    assert_eq!(profile.action_name(), "Bomb");
    assert_eq!(profile.roll_chance().to_bits(), 0.1_f32.to_bits());
    assert!(!profile.starts_disabled());
    assert_eq!(profile.physics_info(), Some("egg"));
    assert!(profile.physics_body().is_none());
    assert!(profile.physics_load_issue().is_none());
    assert!(!profile.release_physics_on_completion());
}

#[test]
fn collector_requires_exact_persistent_action_and_preserves_authored_threshold() {
    let missing_persistent = GameplayCatalog::from_tactics(
        &database(None, Some("fallback")),
        [("bomb".to_owned(), tactic(None, Some(-1.0)))],
    );
    assert!(missing_persistent.bomb("bomb").is_none());

    let wrong_type = GameplayCatalog::from_tactics(
        &database(None, Some("fallback")),
        [("bomb".to_owned(), tactic(Some("Bomb"), Some(-1.0)))],
    );
    let profile = wrong_type.bomb("bomb").expect("exact Bomb action type");
    assert_eq!(profile.roll_chance().to_bits(), (-1.0_f32).to_bits());
    assert_eq!(profile.physics_info(), Some("fallback"));
    assert!(profile.release_physics_on_completion());

    let mut wrong = tactic(Some("Bomb"), None);
    wrong.actions[0].action_type = Some("Detonate".to_owned());
    let catalog =
        GameplayCatalog::from_tactics(&database(None, None), [("bomb".to_owned(), wrong)]);
    assert!(catalog.bomb("bomb").is_none());
}

fn database(physics: Option<&str>, replacement: Option<&str>) -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "bomb".to_owned(),
        tactics: Some("bomb.tactics".to_owned()),
        physics_info: physics.map(str::to_owned),
        physics_replacement_info: replacement.map(str::to_owned),
        ..ProtoObject::default()
    });
    database
}

fn tactic(persistent: Option<&str>, work_range: Option<f32>) -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "Bomb".to_owned(),
            action_type: Some("Bomb".to_owned()),
            work_range,
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_actions: persistent.into_iter().map(str::to_owned).collect(),
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}
