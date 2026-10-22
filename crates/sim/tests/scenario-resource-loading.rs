//! Checks that Blood Gulch players seed lifetime totals from layered starting resources.

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-resource-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_players_seed_lifetime_totals_from_layered_starting_resources() {
    // The default 1v1 settings give player 1 Cutter and player 2 the Arbiter.
    let loaded = test_support::installed_scenario("blood_gulch");
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
