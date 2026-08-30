use super::*;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn persistent_actions_preserve_dodge_and_deflect_contracts() {
    let database = database();
    let tactics = TacticData {
        actions: vec![
            Action {
                name: "Evade".to_owned(),
                action_type: Some("Dodge".to_owned()),
                dodge_chance_max: Some(0.8),
                dodge_chance_min: Some(0.2),
                dodge_max_angle: Some(90.0),
                dodge_cooldown: Some(3.0),
                dodge_physics_impulse: Some(4.0),
                wait_for_deflect_cooldown: Some(true),
                ..Action::default()
            },
            Action {
                name: "Block".to_owned(),
                action_type: Some("Unused".to_owned()),
                persistent_action_type: Some("Deflect".to_owned()),
                deflect_chance_max: Some(1.0),
                deflect_chance_min: Some(0.5),
                deflect_max_angle: Some(60.0),
                deflect_cooldown: Some(2.0),
                deflect_max_damage: Some(50.0),
                wait_for_dodge_cooldown: Some(true),
                small_arms: Some(true),
                multi_deflect: Some(true),
                squad_mode: Some("Cover".to_owned()),
                ..Action::default()
            },
        ],
        tactic: Some(TacticRules {
            persistent_actions: vec!["Evade".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("unit".to_owned(), tactics)]);
    let defense = catalog.projectile_defense("UNIT").unwrap();
    let dodge = defense.dodge().unwrap();
    let deflect = defense.deflect().unwrap();

    assert_eq!(dodge.action_name(), "Evade");
    assert_eq!(dodge.chance_max().to_bits(), 0.8_f32.to_bits());
    assert_eq!(dodge.chance_min().to_bits(), 0.2_f32.to_bits());
    assert_eq!(
        dodge.max_angle().to_bits(),
        std::f32::consts::FRAC_PI_2.to_bits()
    );
    assert_eq!(dodge.cooldown().to_bits(), 3.0_f32.to_bits());
    assert_eq!(dodge.physics_impulse().to_bits(), 4.0_f32.to_bits());
    assert!(dodge.waits_for_deflect_cooldown());
    assert_eq!(deflect.action_name(), "Block");
    assert!((deflect.max_angle() - std::f32::consts::FRAC_PI_3).abs() < 0.000_001);
    assert_eq!(deflect.max_damage().to_bits(), 50.0_f32.to_bits());
    assert_eq!(deflect.squad_mode(), Some(SquadMode::Cover));
    assert!(deflect.waits_for_dodge_cooldown());
    assert!(deflect.small_arms());
    assert!(deflect.multi_deflect());
}

#[test]
fn ordinary_nonpersistent_actions_are_not_materialized() {
    let database = database();
    let tactics = TacticData {
        actions: vec![Action {
            name: "Evade".to_owned(),
            action_type: Some("Dodge".to_owned()),
            dodge_chance_max: Some(1.0),
            ..Action::default()
        }],
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("unit".to_owned(), tactics)]);

    assert!(catalog.projectile_defense("unit").is_none());
}

fn database() -> Database {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "unit".to_owned(),
        tactics: Some("unit.tactics".to_owned()),
        ..ProtoObject::default()
    });
    database
}
