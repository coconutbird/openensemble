use glam::Vec3;
use sim::{
    EntityId, GAIA_PLAYER, load_scenario_from_game_dir, object_prototype_id, spawn_object_at,
    spawn_squad_at, squad_prototype_id,
};
use std::collections::BTreeSet;

const SPAWNER: &str = "env_harvest_treepineicy_spawner_01";
const BIRD: &str = "env_creatures_bird_01";
const MARINES: &str = "unsc_inf_marine_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-ambient-life-spawner -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_layered_spawner_creates_one_fleeing_bird_squad() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let profile = loaded
        .simulation
        .gameplay
        .ambient_life_spawner(SPAWNER)
        .expect("installed tree spawner profile");
    assert_eq!(profile.action_name(), "AmbientLifeSpawner");
    assert_eq!(profile.squad_type(), BIRD);
    assert!(nearly_equal(profile.check_frequency(), 1.0));
    assert!(nearly_equal(profile.opportunity_check_radius(), 20.0));

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let bounds = world
        .terrain_bounds()
        .expect("authoritative terrain bounds");
    let mut origin = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    origin.y = world.terrain_height(origin, true).unwrap_or_default();
    let target_id = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, MARINES).expect("installed Marine squad"),
        origin,
        Vec3::Z,
    )
    .expect("Marine opportunity target should spawn");
    let spawner_id = spawn_object_at(
        world,
        database,
        1,
        object_prototype_id(database, SPAWNER).expect("installed spawner object"),
        origin,
        Vec3::Z,
    )
    .expect("class-zero installed spawner should spawn");
    let birds_before = bird_ids(world);

    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
    assert!(
        !world
            .get_object(spawner_id)
            .unwrap()
            .ambient_life_spawn_complete()
    );
    world.update_entities_with_database_and_gameplay(1.0, database, gameplay);

    let birds_after = bird_ids(world);
    let bird_id = birds_after
        .difference(&birds_before)
        .next()
        .copied()
        .expect("installed action should create its authored bird squad");
    let bird = world.get_squad(bird_id).unwrap();
    assert_eq!(
        world.get_object(spawner_id).unwrap().base.player_id,
        GAIA_PLAYER
    );
    assert_eq!(bird.base.player_id, GAIA_PLAYER);
    assert!(bird.has_ambient_life());
    assert_eq!(bird.ambient_life_dangerous_squad(), Some(target_id));
    assert!((4.0..=8.0).contains(&bird.base.position.distance(origin)));
    assert!(
        world
            .get_object(spawner_id)
            .unwrap()
            .ambient_life_spawn_complete()
    );
    assert_eq!(birds_after.len(), birds_before.len() + 1);

    world.update_entities_with_database_and_gameplay(2.0, database, gameplay);
    assert!(world.get_squad(bird_id).unwrap().is_ambient_life_fleeing());
    assert_eq!(
        bird_ids(world).len(),
        birds_before.len() + 1,
        "spawner must remain done"
    );
}

fn bird_ids(world: &sim::World) -> BTreeSet<EntityId> {
    world
        .squads
        .iter()
        .filter_map(|(id, squad)| {
            squad
                .proto_squad_name
                .eq_ignore_ascii_case(BIRD)
                .then_some(id)
        })
        .collect()
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.000_1
}
