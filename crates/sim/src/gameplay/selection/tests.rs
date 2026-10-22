use super::*;
use pipeline::database::hw1::tactics::{TacticData, TacticRules, Weapon};
use pipeline::database::hw1::{Ability, ProtoObject};
use std::collections::BTreeMap;

fn action(name: &str, weapon: &str, starts_disabled: bool) -> Action {
    Action {
        name: name.to_owned(),
        action_type: Some("RangedAttack".to_owned()),
        weapon: Some(weapon.to_owned()),
        start_disabled: starts_disabled.then_some(true),
        ..Action::default()
    }
}

fn weapon(name: &str, range: f32) -> Weapon {
    Weapon {
        name: name.to_owned(),
        max_range: Some(range),
        ..Weapon::default()
    }
}

fn marine_catalog() -> GameplayCatalog {
    let mut database = Database::new();
    database.abilities.push(Ability {
        name: "Command".to_owned(),
        ..Ability::default()
    });
    database.objects.extend([
        ProtoObject {
            name: "marine".to_owned(),
            tactics: Some("marine.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "ground_target".to_owned(),
            object_class: Some("Unit".to_owned()),
            object_types: vec!["NonFlying".to_owned(), "Infantry".to_owned()],
            damage_type: Some("Light".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "air_target".to_owned(),
            object_class: Some("Unit".to_owned()),
            object_types: vec!["Flying".to_owned()],
            damage_type: Some("Heavy".to_owned()),
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        weapons: vec![
            weapon("CoverRocket", 60.0),
            weapon("CoverGrenade", 60.0),
            weapon("CoverRifle", 60.0),
            weapon("Rifle", 25.0),
            weapon("Grenade", 35.0),
            weapon("Rocket", 50.0),
        ],
        actions: vec![
            action("CoverRocket", "CoverRocket", true),
            action("CoverGrenade", "CoverGrenade", false),
            action("CoverRifle", "CoverRifle", false),
            action("Rifle", "Rifle", false),
            action("Grenade", "Grenade", false),
            action("Rocket", "Rocket", true),
        ],
        tactic: Some(TacticRules {
            target_rules: vec![
                rule("CoverRocket", Some("Cover"), None, &[]),
                rule("CoverGrenade", Some("Cover"), None, &["NonFlying"]),
                rule("CoverRifle", Some("Cover"), None, &[]),
                rule("Rifle", Some("Normal"), None, &[]),
                rule("Grenade", Some("Normal"), Some("Command"), &["NonFlying"]),
                rule("Rocket", Some("Normal"), Some("Command"), &[]),
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    GameplayCatalog::from_tactics(&database, [("marine".to_owned(), tactics)])
}

fn rule(
    action: &str,
    mode: Option<&str>,
    ability: Option<&str>,
    target_types: &[&str],
) -> TargetRule {
    TargetRule {
        relation: Some("Enemy".to_owned()),
        squad_mode: mode.map(str::to_owned),
        action: Some(action.to_owned()),
        ability: ability.map(str::to_owned),
        target_types: target_types.iter().map(|kind| (*kind).to_owned()).collect(),
        ..TargetRule::default()
    }
}

fn selected<'catalog>(
    catalog: &'catalog GameplayCatalog,
    query: &AttackQuery<'_>,
    overrides: &BTreeMap<&str, bool>,
) -> Option<&'catalog str> {
    catalog
        .select_ranged_action("marine", query, |action| {
            overrides
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(&action.name))
                .map_or(action.start_disabled != Some(true), |entry| *entry.1)
        })
        .map(|action| action.action.name.as_str())
}

#[test]
fn marine_rules_switch_by_mode_ability_target_and_live_enablement() {
    let catalog = marine_catalog();
    let mut query = AttackQuery {
        target_proto_object_name: Some("ground_target"),
        ..AttackQuery::default()
    };
    let mut overrides = BTreeMap::new();

    assert_eq!(selected(&catalog, &query, &overrides), Some("Rifle"));

    query.ability_id = Some(0);
    assert_eq!(selected(&catalog, &query, &overrides), Some("Grenade"));

    overrides.insert("Grenade", false);
    overrides.insert("Rocket", true);
    assert_eq!(selected(&catalog, &query, &overrides), Some("Rocket"));

    query.squad_mode = SquadMode::Cover;
    query.ability_id = None;
    assert_eq!(selected(&catalog, &query, &overrides), Some("CoverGrenade"));

    query.target_proto_object_name = Some("air_target");
    assert_eq!(selected(&catalog, &query, &overrides), Some("CoverRifle"));
}

#[test]
fn ability_request_falls_back_to_first_matching_non_ability_rule() {
    let catalog = marine_catalog();
    let query = AttackQuery {
        ability_id: Some(0),
        target_proto_object_name: Some("ground_target"),
        ..AttackQuery::default()
    };
    let overrides = BTreeMap::from([("Grenade", false), ("Rocket", false)]);

    assert_eq!(selected(&catalog, &query, &overrides), Some("Rifle"));
}

#[test]
fn work_selection_preserves_noncombat_actions_and_authored_range() {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "worker".to_owned(),
            tactics: Some("worker.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "tower".to_owned(),
            object_types: vec!["GaiaGarrison".to_owned()],
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        actions: vec![Action {
            name: "EnterTower".to_owned(),
            action_type: Some("Garrison".to_owned()),
            work_range: Some(5.0),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                relation: Some("Any".to_owned()),
                action: Some("EnterTower".to_owned()),
                target_types: vec!["GaiaGarrison".to_owned()],
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("worker".to_owned(), tactics)]);
    let query = AttackQuery {
        relation: TacticRelation::Neutral,
        target_proto_object_name: Some("tower"),
        ..AttackQuery::default()
    };

    let action = catalog
        .select_work_action("worker", &query, authored_enabled)
        .expect("garrison work action");
    assert_eq!(action.action_type.as_deref(), Some("Garrison"));
    assert_eq!(action.work_range, Some(5.0));
    assert!(
        catalog
            .select_ranged_action("worker", &query, authored_enabled)
            .is_none()
    );
    assert!(
        catalog
            .select_work_action("worker", &query, |_| false)
            .is_none()
    );
}

#[test]
fn damage_object_and_state_predicates_follow_retail_or_and_order_rules() {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "attacker".to_owned(),
            tactics: Some("attacker.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "heavy_vehicle".to_owned(),
            object_class: Some("Unit".to_owned()),
            object_types: vec!["Vehicle".to_owned()],
            damage_type: Some("Heavy".to_owned()),
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        weapons: vec![weapon("Damaged", 10.0), weapon("Special", 20.0)],
        actions: vec![
            action("Damaged", "Damaged", false),
            action("Special", "Special", false),
        ],
        tactic: Some(TacticRules {
            target_rules: vec![
                TargetRule {
                    action: Some("Damaged".to_owned()),
                    target_states: vec!["Damaged".to_owned()],
                    ..TargetRule::default()
                },
                TargetRule {
                    action: Some("Special".to_owned()),
                    damage_types: vec!["Light".to_owned()],
                    target_types: vec!["Vehicle".to_owned()],
                    ..TargetRule::default()
                },
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("attacker".to_owned(), tactics)]);
    let query = AttackQuery {
        target_proto_object_name: Some("heavy_vehicle"),
        ..AttackQuery::default()
    };
    assert_eq!(
        catalog
            .select_ranged_action("attacker", &query, authored_enabled)
            .map(|action| action.action.name.as_str()),
        Some("Special")
    );
    assert_eq!(
        catalog
            .select_ranged_action(
                "attacker",
                &{
                    let mut damaged = query;
                    damaged.flags.insert(AttackQueryFlags::TARGET_DAMAGED);
                    damaged
                },
                authored_enabled,
            )
            .map(|action| action.action.name.as_str()),
        Some("Damaged")
    );
}

#[test]
fn auto_target_squad_mode_is_only_a_gate_for_automatic_selection() {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "unit".to_owned(),
        tactics: Some("unit.tactics".to_owned()),
        ..ProtoObject::default()
    });
    let tactics = TacticData {
        weapons: vec![weapon("Mode", 10.0), weapon("Normal", 10.0)],
        actions: vec![
            action("Mode", "Mode", false),
            action("Normal", "Normal", false),
        ],
        tactic: Some(TacticRules {
            target_rules: vec![
                TargetRule {
                    auto_target_squad_mode: Some("Cover".to_owned()),
                    action: Some("Mode".to_owned()),
                    ..TargetRule::default()
                },
                TargetRule {
                    action: Some("Normal".to_owned()),
                    ..TargetRule::default()
                },
            ],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("unit".to_owned(), tactics)]);
    let manual = AttackQuery::default();
    let mut automatic = AttackQuery::default();
    automatic.flags.insert(AttackQueryFlags::AUTO_TARGET);

    assert_eq!(
        catalog
            .select_ranged_action("unit", &manual, authored_enabled)
            .map(|action| action.action.name.as_str()),
        Some("Mode")
    );
    assert_eq!(
        catalog
            .select_ranged_action("unit", &automatic, authored_enabled)
            .map(|action| action.action.name.as_str()),
        Some("Normal")
    );
}

#[test]
fn hand_attacks_reject_occupied_and_authored_cover_targets() {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "attacker".to_owned(),
            tactics: Some("attacker.tactics".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "infantry".to_owned(),
            object_types: vec!["Infantry".to_owned()],
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "cover_object".to_owned(),
            object_types: vec!["Cover".to_owned()],
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        weapons: vec![weapon("Hammer", 3.0)],
        actions: vec![Action {
            name: "HammerAttack".to_owned(),
            action_type: Some("HandAttack".to_owned()),
            weapon: Some("Hammer".to_owned()),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            target_rules: vec![TargetRule {
                action: Some("HammerAttack".to_owned()),
                ..TargetRule::default()
            }],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let catalog = GameplayCatalog::from_tactics(&database, [("attacker".to_owned(), tactics)]);
    let mut query = AttackQuery {
        target_proto_object_name: Some("infantry"),
        ..AttackQuery::default()
    };

    assert!(
        catalog
            .select_ranged_action("attacker", &query, authored_enabled)
            .is_some()
    );
    query.flags.insert(AttackQueryFlags::TARGET_IN_COVER);
    assert!(
        catalog
            .select_ranged_action("attacker", &query, authored_enabled)
            .is_none()
    );
    query.flags = AttackQueryFlags::empty();
    query.target_proto_object_name = Some("cover_object");
    assert!(
        catalog
            .select_ranged_action("attacker", &query, authored_enabled)
            .is_none()
    );
}
