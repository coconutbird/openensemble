//! Checks that the renderer loads only the animation that the sim selects in `PHXscn03`.
//!
//! The test needs a local Halo Wars DE installation.

use render::ugx::{UnitScene, simulation_proto_names};

/// Particle diagnostics for textures that no shipped archive contains.
///
/// The PFX files reference `art\effects\impacts\plasma\plasma_fog_fx` and
/// `art\effects\impacts\flesh\cells_orange_01_df`. No `.era` file of the installed game has these
/// textures, so the renderer reports the fallback and the disabled set.
const SHIPPED_TEXTURE_GAPS: [&str; 2] = [
    "particle effect 'art\\effects\\impacts\\flesh\\hunterflesh_med_01' emitter 'bloodsplat' disabled 1 unavailable optional texture sets",
    "particle effect 'art\\effects\\impacts\\fx_proj_missile_01\\missile_impact_01' emitter 'plasmaParticles' reused a decoded texture for 1 missing stages",
];

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-animation -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn phx03_renderer_loads_only_the_animation_selected_by_authoritative_sim_state() {
    let mut loaded = test_support::installed_scenario("PHXscn03");
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
    assert_only_shipped_texture_gaps(&scene);

    let update = loaded
        .simulation
        .world
        .update_triggers_with_gameplay(&loaded.content.database, &loaded.simulation.gameplay);
    assert_eq!(update.unsupported_effect_types, Vec::<u16>::new());
    assert!(!scene.roster_matches(&loaded.simulation.world));
    assert!(scene.sync_world(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    ));
    assert!(scene.particle_effect_count() > 0);
    assert_only_shipped_texture_gaps(&scene);

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

fn assert_only_shipped_texture_gaps(scene: &UnitScene) {
    let mut issues = scene.particle_effect_issues().to_vec();
    issues.sort();
    assert_eq!(issues, SHIPPED_TEXTURE_GAPS);
}
