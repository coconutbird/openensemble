use super::*;
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules, TargetRule, Weapon};
use pipeline::database::hw1::{Database, GameData, ProtoObject};

#[test]
fn wander_uses_retail_initial_delay_and_circular_rng() {
    let gameplay = ambient_catalog(settings(0.0, 100.0, 0.0, 10.0, 20.0), false);
    let mut world = World::new();
    world.init_players(1);
    let (bird_squad, _) = add_squad(&mut world, GAIA_PLAYER, Vec3::new(8.0, 3.0, 12.0), "bird");
    let mut oracle = World::new();

    world.update_entities_with_gameplay(0.05, &gameplay);
    let bird = world.get_squad(bird_squad).unwrap();
    assert!(bird.has_ambient_life());
    assert_eq!(
        bird.ambient_life_behavior(),
        Some(AmbientLifeBehavior::Wander)
    );
    assert_eq!(bird.ambient_life_target(), None);

    let expected = oracle.random_circular_position(Vec3::new(8.0, 3.0, 12.0), 20.0, 10.0);
    world.update_entities_with_gameplay(0.05, &gameplay);

    let bird = world.get_squad(bird_squad).unwrap();
    assert_eq!(
        bird.ambient_life_behavior(),
        Some(AmbientLifeBehavior::Idle)
    );
    assert_eq!(bird.ambient_life_target(), Some(expected));
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );
}

#[test]
fn square_opportunity_query_flees_from_diagonal_non_gaia_squad() {
    let gameplay = ambient_catalog(settings(0.0, 0.0, 20.0, 30.0, 200.0), false);
    let mut world = World::new();
    world.init_players(1);
    let (bird_squad, _) = add_squad(&mut world, GAIA_PLAYER, Vec3::ZERO, "bird");
    let (danger_squad, _) = add_squad(&mut world, 1, Vec3::new(19.0, 0.0, 19.0), "danger");
    world.get_squad_mut(bird_squad).unwrap().speed = 10.0;

    world.update_entities_with_gameplay(0.05, &gameplay);
    world.update_entities_with_gameplay(0.05, &gameplay);

    let bird = world.get_squad(bird_squad).unwrap();
    assert_eq!(bird.ambient_life_dangerous_squad(), Some(danger_squad));
    assert_eq!(
        bird.ambient_life_behavior(),
        Some(AmbientLifeBehavior::Idle)
    );
    assert!(bird.is_ambient_life_fleeing());
    assert!((bird.base.velocity.length() - 15.0).abs() < 0.000_1);
}

#[test]
fn source_nearest_bug_leaves_last_qualifying_prey_selected() {
    let gameplay = ambient_catalog(settings(0.0, 0.0, 20.0, 30.0, 200.0), true);
    let mut world = World::new();
    world.init_players(1);
    let (bird_squad, _) = add_squad(&mut world, 1, Vec3::ZERO, "bird");
    let (_first_prey, _) = add_squad(&mut world, GAIA_PLAYER, Vec3::X * 2.0, "prey");
    let (last_prey, _) = add_squad(&mut world, GAIA_PLAYER, Vec3::Z * 3.0, "prey");

    world.update_entities_with_gameplay(0.05, &gameplay);
    world.update_entities_with_gameplay(0.05, &gameplay);

    let bird = world.get_squad(bird_squad).unwrap();
    assert_eq!(bird.ambient_life_prey_squad(), Some(last_prey));
    assert_eq!(
        bird.ambient_life_behavior(),
        Some(AmbientLifeBehavior::Hunt)
    );
}

#[test]
fn attributed_damage_selects_the_attacker_squad_for_fleeing() {
    let gameplay = ambient_catalog(settings(0.0, 100.0, 1.0, 30.0, 200.0), false);
    let mut world = World::new();
    world.init_players(1);
    let (bird_squad, bird_unit) = add_squad(&mut world, GAIA_PLAYER, Vec3::ZERO, "bird");
    let (attacker_squad, attacker_unit) = add_squad(&mut world, 1, Vec3::X * 10.0, "danger");
    world.update_entities_with_gameplay(0.05, &gameplay);

    let dealt =
        world.apply_reflected_collision_damage(attacker_unit, 1, bird_unit, 10.0, &gameplay);
    assert_eq!(dealt.to_bits(), 10.0_f32.to_bits());
    world.update_entities_with_gameplay(0.05, &gameplay);

    let bird = world.get_squad(bird_squad).unwrap();
    assert_eq!(bird.ambient_life_dangerous_squad(), Some(attacker_squad));
    assert!(bird.is_ambient_life_fleeing());
}

#[test]
fn killed_prey_enters_exact_ten_second_devour_pause() {
    let gameplay = ambient_catalog(settings(0.0, 100.0, 1.0, 30.0, 200.0), false);
    let mut world = World::new();
    world.init_players(1);
    let (bird_squad, bird_unit) = add_squad(&mut world, 1, Vec3::ZERO, "bird");
    let (_, prey_unit) = add_squad(&mut world, GAIA_PLAYER, Vec3::X, "prey");
    world.update_entities_with_gameplay(0.05, &gameplay);
    if let Some(bird) = world.get_squad_mut(bird_squad) {
        bird.ambient_life.phase = AmbientLifePhase::Attacking;
        bird.ambient_life.current_prey_unit = Some(prey_unit);
        assert!(bird.attack(prey_unit, 0.0, None, None));
    }

    world.notify_ambient_life_killed_unit(bird_unit, prey_unit);
    assert_eq!(
        world.get_squad(bird_squad).unwrap().ambient_life_behavior(),
        Some(AmbientLifeBehavior::Devour)
    );
    for _ in 0..199 {
        world.update_entities_with_gameplay(0.05, &gameplay);
    }
    assert_eq!(
        world.get_squad(bird_squad).unwrap().ambient_life_behavior(),
        Some(AmbientLifeBehavior::Devour)
    );
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert_eq!(
        world.get_squad(bird_squad).unwrap().ambient_life_behavior(),
        Some(AmbientLifeBehavior::Wander)
    );
}

#[test]
fn membership_change_disconnects_owned_action_state() {
    let gameplay = ambient_catalog(settings(0.0, 100.0, 1.0, 30.0, 200.0), false);
    let mut world = World::new();
    world.init_players(1);
    let (bird_squad, bird_unit) = add_squad(&mut world, GAIA_PLAYER, Vec3::ZERO, "bird");
    world.update_entities_with_gameplay(0.05, &gameplay);
    assert!(world.get_squad(bird_squad).unwrap().has_ambient_life());

    assert!(world.detach_unit_from_squad(bird_unit));
    assert!(!world.get_squad(bird_squad).unwrap().has_ambient_life());
}

fn settings(
    max_wander_frequency: f32,
    predator_frequency: f32,
    radius: f32,
    minimum_wander: f32,
    maximum_wander: f32,
) -> GameData {
    GameData {
        al_max_wander_frequency: Some(max_wander_frequency),
        al_predator_check_frequency: Some(predator_frequency),
        al_prey_check_frequency: Some(0.0),
        al_opp_check_radius: Some(radius),
        al_flee_distance: Some(40.0),
        al_flee_movement_modifier: Some(1.5),
        al_min_wander_distance: Some(minimum_wander),
        al_max_wander_distance: Some(maximum_wander),
        ..GameData::default()
    }
}

fn ambient_catalog(game_data: GameData, can_hunt: bool) -> GameplayCatalog {
    let database = Database {
        objects: vec![
            ProtoObject {
                name: "bird".to_owned(),
                tactics: Some("bird.tactics".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "prey".to_owned(),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "danger".to_owned(),
                ..ProtoObject::default()
            },
        ],
        game_data: Some(game_data),
        ..Database::default()
    };
    let mut actions = vec![Action {
        name: "AmbientLife".to_owned(),
        action_type: Some("AmbientLife".to_owned()),
        ..Action::default()
    }];
    let mut weapons = Vec::new();
    let mut target_rules = Vec::new();
    if can_hunt {
        actions.push(Action {
            name: "Bite".to_owned(),
            action_type: Some("HandAttack".to_owned()),
            weapon: Some("bite".to_owned()),
            ..Action::default()
        });
        weapons.push(Weapon {
            name: "bite".to_owned(),
            max_range: Some(2.0),
            ..Weapon::default()
        });
        target_rules.push(TargetRule {
            relation: Some("Any".to_owned()),
            action: Some("Bite".to_owned()),
            ..TargetRule::default()
        });
    }
    GameplayCatalog::from_tactics(
        &database,
        [(
            "bird".to_owned(),
            TacticData {
                weapons,
                actions,
                tactic: Some(TacticRules {
                    target_rules,
                    persistent_squad_actions: vec!["AmbientLife".to_owned()],
                    ..TacticRules::default()
                }),
                ..TacticData::default()
            },
        )],
    )
}

fn add_squad(
    world: &mut World,
    player_id: PlayerId,
    position: Vec3,
    proto_object: &str,
) -> (EntityId, EntityId) {
    let squad_id = world.create_squad_at(player_id, position);
    let unit_id = world.create_unit_at(player_id, position);
    world.get_unit_mut(unit_id).unwrap().proto_object_name = proto_object.to_owned();
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (squad_id, unit_id)
}
