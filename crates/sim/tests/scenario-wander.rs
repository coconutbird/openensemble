use glam::Vec3;
use sim::{load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id};

const SPORE_CLOUD: &str = "fld_air_sporecloud_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-wander -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_spore_cloud_wanders_from_scenario_layered_tactics() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let profile = loaded
        .simulation
        .gameplay
        .wander(SPORE_CLOUD)
        .expect("shipped spore-cloud Wander profile");
    assert_eq!(profile.action_name(), "WanderAction");
    assert_eq!(profile.work_range().to_bits(), 50.0_f32.to_bits());
    assert!(!profile.starts_disabled());

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    let origin = Vec3::new(100.0, 10.0, 100.0);
    let prototype_id =
        squad_prototype_id(database, SPORE_CLOUD).expect("shipped spore-cloud squad prototype");
    let squad_id = spawn_squad_at(world, database, 1, prototype_id, origin, Vec3::Z)
        .expect("shipped spore-cloud squad should spawn");

    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);

    let squad = world.get_squad(squad_id).expect("live wandering squad");
    assert!(squad.is_wandering());
    assert_eq!(squad.wander_origin(), Some(origin));
    let target = squad
        .wander_target()
        .expect("source-selected wander target");
    assert!(target.distance(origin) <= 50.000_01);
    if let Some(destination) = squad.move_target {
        assert!((destination.distance(target) - 5.0).abs() < 0.000_1);
    } else {
        assert!(target.distance(origin) <= 5.0);
    }
}
