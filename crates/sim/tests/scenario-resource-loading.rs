use sim::{configure_player_leader, load_scenario_from_game_dir};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-resource-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_players_seed_lifetime_totals_from_layered_starting_resources() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario and layered database should load");
    let leader_id = loaded
        .content
        .database
        .leaders
        .iter()
        .position(|leader| leader.resources.iter().any(|entry| entry.amount > 0.0))
        .and_then(|index| i32::try_from(index).ok())
        .expect("layered leaders should contain starting resources");
    assert!(configure_player_leader(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        leader_id,
    ));
    let players = loaded.simulation.world.active_players().collect::<Vec<_>>();

    assert!(players.iter().any(|player| {
        player
            .total_resources
            .amounts
            .iter()
            .any(|amount| *amount > 0.0)
    }));
    assert!(players.iter().all(|player| {
        player
            .resources
            .amounts
            .iter()
            .zip(player.total_resources.amounts)
            .all(|(balance, total)| (*balance - total).abs() < f32::EPSILON)
    }));
}
