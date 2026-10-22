//! Checks that the installed scenario maps the primary Scarab targeting exception.

use sim::load_scenario_from_game_dir;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-targeting-lead -- --ignored --nocapture`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn installed_scenario_maps_the_primary_scarab_targeting_exception() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let loaded = load_scenario_from_game_dir(&game_dir, "PHXscn07")
        .expect("installed Scenario 7 and its layered database should load");
    let mapping = loaded
        .content
        .database
        .game_data
        .as_ref()
        .and_then(|data| data.code_proto_objects.as_ref())
        .and_then(|objects| {
            objects
                .entries
                .iter()
                .find(|entry| entry.object_type.trim().eq_ignore_ascii_case("Scn07Scarab"))
        })
        .expect("retail game data should map Scn07Scarab");

    assert!(
        loaded
            .simulation
            .gameplay
            .object(mapping.proto_name.trim())
            .is_some(),
        "mapped Scenario 7 Scarab should have layered tactic data: {}",
        mapping.proto_name
    );
    eprintln!("Scn07Scarab={}", mapping.proto_name.trim());
}
