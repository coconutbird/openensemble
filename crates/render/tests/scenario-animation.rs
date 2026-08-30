use render::ugx::{UnitScene, simulation_proto_names};
use sim::load_scenario_from_game_dir;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-animation -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn phx03_renderer_loads_only_the_animation_selected_by_authoritative_sim_state() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "PHXscn03").expect("load PHXscn03");
    let proto_names = simulation_proto_names(&loaded.simulation.world).collect::<Vec<_>>();
    let _loaded_visuals = loaded
        .content
        .load_visuals_for(&mut loaded.source, proto_names.iter().copied());
    let mut scene = UnitScene::load_world(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    );
    assert!(scene.roster_matches(&loaded.simulation.world));
    assert!(
        scene.particle_effect_count() > 0,
        "PHXscn03 should decode at least one authored PFX graph"
    );
    assert_eq!(
        scene.particle_effect_issue_count(),
        0,
        "{:#?}",
        scene.particle_effect_issues()
    );

    let update = loaded
        .simulation
        .world
        .update_triggers_with_gameplay(&loaded.content.database, &loaded.simulation.gameplay);
    assert!(update.unsupported_effect_types.is_empty());
    assert!(!scene.roster_matches(&loaded.simulation.world));
    assert!(scene.sync_world(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    ));
    assert!(scene.particle_effect_count() > 0);
    assert_eq!(
        scene.particle_effect_issue_count(),
        0,
        "{:#?}",
        scene.particle_effect_issues()
    );

    for scenario_id in [1958, 616] {
        let entity_id = loaded
            .simulation
            .get_entity_id(scenario_id)
            .unwrap_or_else(|| panic!("PHXscn03 scenario object {scenario_id}"));
        let placement = scene
            .placements()
            .iter()
            .find(|placement| placement.entity_id() == entity_id)
            .unwrap_or_else(|| panic!("PHXscn03 rendered object {scenario_id}"));
        assert_eq!(placement.animation_revision(), 1);
        assert!(placement.unit().has_scripted_animation());
    }
}
