use super::*;
use crate::gameplay::GameplayCatalog;
use crate::player::GAIA_PLAYER;
use crate::spawn::{object_prototype_id, spawn_object_at};
use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
use pipeline::database::hw1::tactics::{Action, TacticData, TacticRules};
use pipeline::database::hw1::{GameData, Squad as ProtoSquad};

const SPAWNER: &str = "tree_spawner";
const BIRD_SQUAD: &str = "bird_squad";
const BIRD_UNIT: &str = "bird_unit";
const TARGET_SQUAD: &str = "target_squad";

#[test]
fn one_shot_spawner_uses_retail_timer_transform_rng_and_flee_handoff() {
    let (database, gameplay) = fixture();
    let mut world = World::new();
    world.init_players(1);
    let mut oracle = World::new();
    let origin = Vec3::new(10.0, 3.0, 20.0);
    let spawner_id = spawn_spawner(&mut world, &database, origin);
    let target_id = spawn_named_squad(&mut world, &database, TARGET_SQUAD, origin);
    let angle = oracle.trigger_random_float(0.0, std::f32::consts::TAU);
    let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
    let distance_scalar = oracle.trigger_random_float(2.0, 4.0);
    let expected_position = origin + direction * (1.0 + 2.0 * distance_scalar);
    let _initial_wander_timer = oracle.trigger_random_index(20_000);

    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert!(
        world
            .get_object(spawner_id)
            .unwrap()
            .has_ambient_life_spawner()
    );
    assert!(
        !world
            .get_object(spawner_id)
            .unwrap()
            .ambient_life_spawn_complete()
    );
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert!(bird_squad(&world).is_none());
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);

    let bird_id = bird_squad(&world).expect("due opportunity should create one bird squad");
    let bird = world.get_squad(bird_id).unwrap();
    assert_eq!(
        world.get_object(spawner_id).unwrap().base.player_id,
        GAIA_PLAYER
    );
    assert_eq!(bird.base.player_id, GAIA_PLAYER);
    assert!(bird.base.position.abs_diff_eq(expected_position, 0.000_1));
    assert!(bird.base.forward.abs_diff_eq(direction, 0.000_1));
    assert!(bird.has_ambient_life());
    assert_eq!(bird.ambient_life_dangerous_squad(), Some(target_id));
    assert!(
        world
            .get_object(spawner_id)
            .unwrap()
            .ambient_life_spawn_complete()
    );
    assert_eq!(
        world.trigger_random_index(100),
        oracle.trigger_random_index(100)
    );

    world.update_entities_with_database_and_gameplay(1.0, &database, &gameplay);
    assert!(world.get_squad(bird_id).unwrap().is_ambient_life_fleeing());
    assert_eq!(world.squads.len(), 2, "the completed action is one-shot");
}

#[test]
fn empty_opportunity_scan_resets_interval_and_retries() {
    let (database, gameplay) = fixture();
    let mut world = World::new();
    world.init_players(1);
    let spawner_id = spawn_spawner(&mut world, &database, Vec3::ZERO);

    for _ in 0..3 {
        world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    }
    assert!(
        !world
            .get_object(spawner_id)
            .unwrap()
            .ambient_life_spawn_complete()
    );
    assert!(bird_squad(&world).is_none());

    let _target_id = spawn_named_squad(&mut world, &database, TARGET_SQUAD, Vec3::ZERO);
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert!(bird_squad(&world).is_none());
    world.update_entities_with_database_and_gameplay(0.05, &database, &gameplay);
    assert!(bird_squad(&world).is_some());
    assert!(
        world
            .get_object(spawner_id)
            .unwrap()
            .ambient_life_spawn_complete()
    );
}

fn spawn_spawner(world: &mut World, database: &Database, position: Vec3) -> EntityId {
    spawn_object_at(
        world,
        database,
        1,
        object_prototype_id(database, SPAWNER).unwrap(),
        position,
        Vec3::Z,
    )
    .expect("class-zero spawner should be runtime-spawnable")
}

fn spawn_named_squad(
    world: &mut World,
    database: &Database,
    name: &str,
    position: Vec3,
) -> EntityId {
    spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, name).unwrap(),
        position,
        Vec3::Z,
    )
    .unwrap()
}

fn bird_squad(world: &World) -> Option<EntityId> {
    world.squads.iter().find_map(|(id, squad)| {
        squad
            .proto_squad_name
            .eq_ignore_ascii_case(BIRD_SQUAD)
            .then_some(id)
    })
}

fn fixture() -> (Database, GameplayCatalog) {
    let database = Database {
        objects: vec![
            proto_object(SPAWNER, "Object", Some("spawner.tactics"), 1.0),
            proto_object(BIRD_UNIT, "Unit", Some("bird.tactics"), 2.0),
            proto_object("target_unit", "Unit", None, 1.0),
        ],
        squads: vec![
            proto_squad(BIRD_SQUAD, BIRD_UNIT),
            proto_squad(TARGET_SQUAD, "target_unit"),
        ],
        game_data: Some(GameData {
            al_spawner_check_frequency: Some(0.1),
            al_opp_check_radius: Some(20.0),
            al_max_wander_frequency: Some(20.0),
            al_predator_check_frequency: Some(1.0),
            al_prey_check_frequency: Some(1.0),
            al_flee_distance: Some(40.0),
            al_flee_movement_modifier: Some(1.5),
            al_min_wander_distance: Some(30.0),
            al_max_wander_distance: Some(200.0),
            ..GameData::default()
        }),
        ..Database::default()
    };
    let gameplay = GameplayCatalog::from_tactics(
        &database,
        [
            (SPAWNER.to_owned(), spawner_tactic()),
            (BIRD_UNIT.to_owned(), bird_tactic()),
        ],
    );
    (database, gameplay)
}

fn proto_object(name: &str, class: &str, tactics: Option<&str>, radius: f32) -> ProtoObject {
    ProtoObject {
        name: name.to_owned(),
        object_class: Some(class.to_owned()),
        tactics: tactics.map(str::to_owned),
        hitpoints: Some(10.0),
        max_velocity: Some(10.0),
        obstruction_radius_x: Some(radius),
        obstruction_radius_z: Some(radius),
        flags: (name == SPAWNER)
            .then(|| "ForceToGaiaPlayer".to_owned())
            .into_iter()
            .collect(),
        ..ProtoObject::default()
    }
}

fn proto_squad(name: &str, member: &str) -> ProtoSquad {
    ProtoSquad {
        name: name.to_owned(),
        units: Some(UnitsWrapper {
            entries: vec![UnitEntry {
                proto_object: member.to_owned(),
                count: 1,
                ..UnitEntry::default()
            }],
        }),
        ..ProtoSquad::default()
    }
}

fn spawner_tactic() -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "SpawnBird".to_owned(),
            action_type: Some("AmbientLifeSpawner".to_owned()),
            squad_type: Some(BIRD_SQUAD.to_owned()),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_actions: vec!["SpawnBird".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}

fn bird_tactic() -> TacticData {
    TacticData {
        actions: vec![Action {
            name: "AmbientLife".to_owned(),
            action_type: Some("AmbientLife".to_owned()),
            ..Action::default()
        }],
        tactic: Some(TacticRules {
            persistent_squad_actions: vec!["AmbientLife".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    }
}
