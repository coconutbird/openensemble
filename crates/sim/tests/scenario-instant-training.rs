use glam::Vec3;
use sim::{
    TrainingKind, load_scenario_from_game_dir, object_prototype_id, spawn_object_at,
    squad_runtime_id,
};

const SENTINEL_STORE: &str = "hook_store_sentinel_01";
const SENTINEL_SQUAD: &str = "for_air_sentinel_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-instant-training -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_sentinel_store_trains_immediately_then_recharges_in_sim_state() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let player_id = *loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base")
        .0;
    let player = loaded.simulation.world.get_player_mut(player_id).unwrap();
    player.resources.amounts = [100_000.0; 4];
    let population_slots = player.population.len();
    for population_type in 0..population_slots {
        assert!(player.set_population_limits(population_type, 1_000.0, 1_000.0));
    }
    let store_prototype_id = object_prototype_id(&loaded.content.database, SENTINEL_STORE)
        .expect("layered database should expose the shipped sentinel store");
    let sentinel_runtime_id = squad_runtime_id(&loaded.content.database, SENTINEL_SQUAD)
        .expect("layered database should expose the shipped sentinel squad");
    let store_id = spawn_object_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        player_id,
        store_prototype_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .expect("shipped sentinel store should be spawnable");
    let squads_before = loaded.simulation.world.squads.len();

    let result = loaded
        .simulation
        .world
        .queue_training(
            player_id,
            store_id,
            &loaded.content.database,
            TrainingKind::Squad,
            sentinel_runtime_id,
            1,
        )
        .expect("shipped instant-training command should be accepted");
    assert_eq!(result.accepted, 1);
    assert_eq!(loaded.simulation.world.squads.len(), squads_before + 1);
    assert!(
        loaded
            .simulation
            .world
            .get_building(store_id)
            .unwrap()
            .production
            .is_idle()
    );
    let recharge = loaded
        .simulation
        .world
        .training_recharge(store_id, TrainingKind::Squad, sentinel_runtime_id)
        .expect("instant training should install the authored lockout");
    assert!((recharge.time_remaining() - 15.0).abs() <= f32::EPSILON);

    let rejected = loaded
        .simulation
        .world
        .queue_training(
            player_id,
            store_id,
            &loaded.content.database,
            TrainingKind::Squad,
            sentinel_runtime_id,
            1,
        )
        .expect("recharging is an unavailable command, not malformed data");
    assert_eq!(rejected.accepted, 0);
    let _update = loaded
        .simulation
        .world
        .update_production(1.0, &loaded.content.database);
    let recharge = loaded
        .simulation
        .world
        .training_recharge(store_id, TrainingKind::Squad, sentinel_runtime_id)
        .unwrap();
    assert!((recharge.time_remaining() - 14.0).abs() <= f32::EPSILON);
}
