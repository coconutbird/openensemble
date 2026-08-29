use super::*;
use pipeline::database::hw1::tactics::{TacticRules, TargetRule};

fn database() -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "test_unit".to_owned(),
        tactics: Some("test_unit.tactics".to_owned()),
        damage_type: Some("Light".to_owned()),
        ..ProtoObject::default()
    });
    database
}

fn action(name: &str, weapon: &str) -> Action {
    Action {
        name: name.to_owned(),
        action_type: Some("RangedAttack".to_owned()),
        weapon: Some(weapon.to_owned()),
        ..Action::default()
    }
}

fn weapon(name: &str, range: f32) -> Weapon {
    Weapon {
        name: name.to_owned(),
        damage_per_second: Some(10.0),
        max_range: Some(range),
        ..Weapon::default()
    }
}

#[test]
fn catalog_lookup_is_case_insensitive_and_preserves_raw_tactics() {
    let tactics = TacticData {
        weapons: vec![weapon("Rifle", 25.0)],
        actions: vec![action("RifleAttack", "Rifle")],
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database(), [("TEST_UNIT".to_owned(), tactics)]);

    let object = catalog.object("Test_Unit").expect("loaded gameplay");
    assert_eq!(object.proto_object_name(), "test_unit");
    assert_eq!(object.damage_type(), Some("Light"));
    assert_eq!(object.tactics().actions.len(), 1);
    assert_eq!(
        catalog
            .initial_ranged_action("TEST_UNIT")
            .and_then(|resolved| resolved.weapon.max_range),
        Some(25.0)
    );
}

#[test]
fn authored_enemy_rule_wins_in_retail_order() {
    let tactics = TacticData {
        weapons: vec![weapon("First", 10.0), weapon("Second", 20.0)],
        actions: vec![
            action("FirstAttack", "First"),
            action("SecondAttack", "Second"),
        ],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Enemy".to_owned()),
                action: Some("SecondAttack".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database(), [("test_unit".to_owned(), tactics)]);

    let selected = catalog
        .initial_ranged_action("test_unit")
        .expect("rule-selected action");
    assert_eq!(selected.action.name, "SecondAttack");
    assert_eq!(selected.weapon.max_range, Some(20.0));
}

#[test]
fn ambiguous_enabled_attacks_are_not_guessed() {
    let tactics = TacticData {
        weapons: vec![weapon("Rifle", 25.0), weapon("Rocket", 50.0)],
        actions: vec![
            action("RifleAttack", "Rifle"),
            action("RocketAttack", "Rocket"),
        ],
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database(), [("test_unit".to_owned(), tactics)]);

    assert!(catalog.initial_ranged_action("test_unit").is_none());
    assert_eq!(
        catalog
            .object("test_unit")
            .unwrap()
            .ranged_actions()
            .count(),
        2
    );
}

#[test]
fn baseline_selection_uses_normal_mode_and_ignores_cover_and_abilities() {
    let tactics = TacticData {
        weapons: vec![
            weapon("CoverRifle", 60.0),
            weapon("Grenade", 35.0),
            weapon("Rifle", 25.0),
        ],
        actions: vec![
            action("CoverAttack", "CoverRifle"),
            action("GrenadeAttack", "Grenade"),
            action("RifleAttack", "Rifle"),
        ],
        tactic: Some(TacticRules {
            target_rules: vec![
                TargetRule {
                    relation: Some("Enemy".to_owned()),
                    squad_mode: Some("Cover".to_owned()),
                    action: Some("CoverAttack".to_owned()),
                    ..TargetRule::default()
                },
                TargetRule {
                    squad_mode: Some("Normal".to_owned()),
                    ability: Some("Command".to_owned()),
                    action: Some("GrenadeAttack".to_owned()),
                    ..TargetRule::default()
                },
                TargetRule {
                    relation: Some("Enemy".to_owned()),
                    squad_mode: Some("Normal".to_owned()),
                    action: Some("RifleAttack".to_owned()),
                    ..TargetRule::default()
                },
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database(), [("test_unit".to_owned(), tactics)]);

    assert_eq!(
        catalog
            .initial_ranged_action("test_unit")
            .map(|action| action.action.name.as_str()),
        Some("RifleAttack")
    );
}
