use sim::{ScenarioAssetLoadError, load_scenario_from_game_dir};

const DATABASE_PATHS: [&str; 10] = [
    "data\\objects.xml.xmb",
    "data\\squads.xml.xmb",
    "data\\techs.xml.xmb",
    "data\\abilities.xml.xmb",
    "data\\powers.xml.xmb",
    "data\\civs.xml.xmb",
    "data\\leaders.xml.xmb",
    "data\\weapontypes.xml.xmb",
    "data\\damagetypes.xml.xmb",
    "data\\gamedata.xml.xmb",
];

#[test]
fn missing_scenario_archive_is_reported_before_database_loading() {
    let missing_dir = std::env::temp_dir().join(format!(
        "openensemble-missing-game-assets-{}",
        std::process::id()
    ));
    let result = load_scenario_from_game_dir(
        &missing_dir.to_string_lossy(),
        "definitely_not_a_real_scenario",
    );

    assert!(matches!(
        result,
        Err(ScenarioAssetLoadError::ScenarioArchiveNotFound { .. })
    ));
}

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-asset-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn loads_real_scenario_database_and_simulation_together() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let scenario =
        std::env::var("OPENENSEMBLE_TEST_SCENARIO").unwrap_or_else(|_| "blood_gulch".to_owned());

    let loaded = load_scenario_from_game_dir(&game_dir, &scenario)
        .expect("scenario, layered database, and simulation should load");

    let archives = loaded.source.files_per_archive();
    let (scenario_archive, scenario_files) = archives
        .last()
        .expect("the loaded scenario should be the final archive layer");
    assert!(
        scenario_files
            .iter()
            .any(|file| file.to_ascii_lowercase().ends_with(".scn.xmb")),
        "{scenario_archive} should contain the active SCN"
    );
    for path in DATABASE_PATHS {
        assert!(
            loaded.source.provenance(path).is_some(),
            "database table {path} should resolve alongside {scenario_archive}"
        );
    }

    assert!(!loaded.content.database.objects.is_empty());
    assert!(!loaded.content.database.squads.is_empty());
    assert!(
        loaded
            .content
            .database
            .objects
            .iter()
            .any(|object| { object.name.eq_ignore_ascii_case("unsc_veh_warthog_01") })
    );
    assert!(
        loaded
            .content
            .database
            .objects
            .iter()
            .any(|object| { object.name.eq_ignore_ascii_case("unsc_inf_marine_01") })
    );
    assert!(loaded.content.scenario_data.is_some());
    assert!(loaded.simulation.world.player_count() > 1);
    let scenario_data = loaded
        .content
        .scenario_data
        .as_ref()
        .expect("scenario data");
    let max_players = loaded
        .content
        .scenario
        .as_ref()
        .map(|descriptor| descriptor.max_players);
    let expected_start_count = scenario_data
        .positions()
        .iter()
        .filter(|position| {
            u32::try_from(position.number).is_ok_and(|number| {
                number > 0 && max_players.is_none_or(|maximum| number <= maximum)
            })
        })
        .count()
        .min(loaded.simulation.world.active_players().count());
    assert_eq!(
        loaded.simulation.initial_base_ids.len(),
        expected_start_count
    );
    for (&player_id, &base_id) in &loaded.simulation.initial_base_ids {
        assert_eq!(
            loaded.simulation.get_initial_base_id(player_id),
            Some(base_id)
        );
        let base = loaded
            .simulation
            .world
            .get_base(base_id)
            .expect("initial base should be registered in the sim world");
        let anchor = loaded
            .simulation
            .world
            .get_building(base.anchor_building_id)
            .expect("initial base should have a building anchor");
        assert_eq!(base.player_id, player_id);
        assert_eq!(anchor.base.player_id, player_id);
        assert!(!anchor.proto_object_name.is_empty());
    }
    assert!(
        !loaded.simulation.world.units.is_empty() || !loaded.simulation.world.squads.is_empty()
    );
}
