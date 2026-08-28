use sim::{ConstructionKind, load_scenario_from_game_dir, object_runtime_id};

const COMMAND_CENTER: &str = "unsc_bldg_command_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-construction-loading -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn builds_real_command_center_through_blood_gulch_power_socket() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("scenario archive and layered database should load");
    let (&player_id, &base_id) = loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("Blood Gulch should assign a player base");
    let builder_id = loaded
        .simulation
        .world
        .get_base(base_id)
        .expect("initial base")
        .anchor_building_id;
    assert_eq!(
        loaded
            .simulation
            .world
            .get_building(builder_id)
            .unwrap()
            .proto_object_name,
        "game_base_Socket_01"
    );
    loaded
        .simulation
        .world
        .get_player_mut(player_id)
        .unwrap()
        .resources
        .amounts = [100_000.0; 4];
    let target_id = object_runtime_id(&loaded.content.database, COMMAND_CENTER)
        .expect("real database should expose the command-center runtime ID");
    let bases_before = loaded.simulation.world.bases().count();

    loaded
        .simulation
        .world
        .queue_build_other(player_id, builder_id, &loaded.content.database, target_id)
        .expect("the authored PowerSocketBase should accept its command center");
    let promoted = loaded
        .simulation
        .world
        .update_production(0.05, &loaded.content.database);
    assert_eq!(promoted.completed_construction, 0);
    let building_id = loaded
        .simulation
        .world
        .units
        .iter()
        .find_map(|(id, unit)| {
            (unit.built_by == Some(builder_id)
                && unit.proto_object_name.eq_ignore_ascii_case(COMMAND_CENTER))
            .then_some(id)
        })
        .expect("promotion should create the unfinished command center");
    let building = loaded.simulation.world.get_building(building_id).unwrap();
    assert!(!building.built);
    assert_eq!(building.base_id, Some(base_id));
    assert_eq!(loaded.simulation.world.bases().count(), bases_before);
    let progress = loaded
        .simulation
        .world
        .construction_progress(
            player_id,
            builder_id,
            ConstructionKind::BuildOther,
            target_id,
        )
        .unwrap()
        .expect("socket worker should expose child progress");
    assert_eq!(progress.building_id, Some(building_id));
    assert!((progress.total_points - 30.0).abs() < f32::EPSILON);

    let completed = loaded
        .simulation
        .world
        .update_production(30.0, &loaded.content.database);
    assert_eq!(completed.completed_construction, 1);
    assert!(
        loaded
            .simulation
            .world
            .get_building(building_id)
            .unwrap()
            .built
    );
    let _released = loaded
        .simulation
        .world
        .update_production(0.05, &loaded.content.database);
    assert!(
        loaded
            .simulation
            .world
            .get_building(builder_id)
            .unwrap()
            .production
            .is_idle()
    );
}
