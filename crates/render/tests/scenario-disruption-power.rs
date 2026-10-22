//! Checks that the disruption bomber, field and pulses project from authoritative sim objects.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use render::ugx::UnitScene;
use render_test_support::placement;
use sim::{DisruptionPowerInvocation, EntityId, MS_PER_TICK, power_prototype_id};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-disruption-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn disruption_bomber_field_and_pulses_project_from_authoritative_sim_objects() {
    let loaded = test_support::installed_scenario("blood_gulch");
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = loaded;
    let power_id = power_prototype_id(&content.database, "UnscLeaderDisruption")
        .expect("shipped Disruption power");
    simulation
        .world
        .invoke_disruption_power(
            &content.database,
            DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: power_id,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Disruption execution");
    let bomber_id = simulation.world.active_disruption_powers()[0].bomber_object_id();
    let active_proto_names =
        render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
    content.load_visuals_for(&mut source, active_proto_names.iter().copied());
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert!(placement(&scene, bomber_id).is_some());

    for _ in 0..60 {
        simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(MS_PER_TICK);
        simulation.world.update_entities_with_database_and_gameplay(
            0.05,
            &content.database,
            &simulation.gameplay,
        );
        if simulation.world.active_disruption_powers()[0].is_active() {
            break;
        }
    }
    let execution = &simulation.world.active_disruption_powers()[0];
    assert!(execution.is_active());
    let field_id = execution.disruption_object_id();
    let pulse_id = simulation
        .world
        .objects
        .iter()
        .find_map(|(id, object)| {
            object
                .proto_object_name
                .eq_ignore_ascii_case("fx_disruptionRing")
                .then_some(id)
        })
        .expect("sim-owned pulse attachment");
    let active_proto_names =
        render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
    content.load_visuals_for(&mut source, active_proto_names.iter().copied());
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    assert!(placement(&scene, field_id).is_some());
    assert!(placement(&scene, pulse_id).is_some());
    assert!(scene.roster_matches(&simulation.world));
}
