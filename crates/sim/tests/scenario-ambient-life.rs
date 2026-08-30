use glam::Vec3;
use sim::{
    AmbientLifeBehavior, GAIA_PLAYER, load_scenario_from_game_dir, spawn_squad_at,
    squad_prototype_id,
};

const BIRD: &str = "env_creatures_bird_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-ambient-life -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_bird_uses_layered_ambient_life_data() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let profile = loaded
        .simulation
        .gameplay
        .ambient_life(BIRD)
        .expect("shipped bird AmbientLife profile");
    assert_eq!(profile.action_name(), "AmbientLife");
    assert!(nearly_equal(profile.max_wander_frequency(), 20.0));
    assert!(nearly_equal(profile.predator_check_frequency(), 2.0));
    assert!(nearly_equal(profile.prey_check_frequency(), 0.0));
    assert!(nearly_equal(profile.opportunity_check_radius(), 20.0));
    assert!(nearly_equal(profile.flee_distance(), 40.0));
    assert!(nearly_equal(profile.flee_movement_modifier(), 1.5));
    assert!(nearly_equal(profile.minimum_wander_distance(), 30.0));
    assert!(nearly_equal(profile.maximum_wander_distance(), 200.0));
    assert!(!profile.starts_disabled());

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let bounds = world
        .terrain_bounds()
        .expect("scenario terrain bounds should be authoritative");
    let origin = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        20.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    let prototype_id = squad_prototype_id(database, BIRD).expect("shipped bird squad prototype");
    let squad_id = spawn_squad_at(world, database, GAIA_PLAYER, prototype_id, origin, Vec3::Z)
        .expect("shipped ambient bird should spawn");

    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);

    let squad = world.get_squad(squad_id).expect("live ambient bird");
    assert!(squad.has_ambient_life());
    assert_eq!(
        squad.ambient_life_behavior(),
        Some(AmbientLifeBehavior::Wander)
    );
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.000_1
}
