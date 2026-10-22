//! Checks that the renderer shows the cleansing power roster and transforms from live sim state.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use pipeline::database::hw1::Database;
use render::ugx::{UnitScene, simulation_entity_transform};
use render_test_support::{assert_visual_loaded, load_active_visuals, placement, sync_scene};
use sim::{CleansingPowerInvocation, EntityId, NativePowerInput, PowerUserId, power_prototype_id};

const CLEANSING_POWER: &str = "CovLeaderGlassing";
const BEAM: &str = "pow_gp_cleansing_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-cleansing-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_cleansing_roster_and_transform_from_live_sim_state() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = test_support::installed_scenario("blood_gulch");
    remove_scenario_squads(&mut simulation.world);
    let power_id =
        power_prototype_id(&content.database, CLEANSING_POWER).expect("shipped Cleansing power");
    let execution_id = simulation
        .world
        .invoke_cleansing_power(&content.database, invocation(power_id))
        .expect("shipped Cleansing execution");
    let beam_id = simulation.world.active_cleansing_powers()[0].beam_object_id();

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert_visual_loaded(&scene, beam_id, BEAM);
    assert!(scene.roster_matches(&simulation.world));
    let initial_transform = simulation_entity_transform(&simulation.world, beam_id).unwrap();

    assert!(simulation.world.submit_cleansing_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Position(Vec3::X * 8.0),
    ));
    advance(&mut simulation, &content.database, 1);
    let moved_transform = simulation_entity_transform(&simulation.world, beam_id).unwrap();
    assert_ne!(moved_transform, initial_transform);
    assert_eq!(
        moved_transform.w_axis.truncate(),
        simulation.world.get_object(beam_id).unwrap().base.position
    );
    assert!(!scene.roster_matches(&simulation.world));
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert_eq!(
        placement(&scene, beam_id).unwrap().transform(),
        moved_transform
    );

    assert!(simulation.world.submit_cleansing_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Shutdown,
    ));
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(placement(&scene, beam_id).is_none());
    assert!(scene.roster_matches(&simulation.world));
}

fn advance(simulation: &mut sim::LoadedScenario, database: &Database, ticks: usize) {
    for _ in 0..ticks {
        simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(100);
        simulation.world.update_entities_with_database_and_gameplay(
            0.1,
            database,
            &simulation.gameplay,
        );
    }
}

fn invocation(proto_power_id: i32) -> CleansingPowerInvocation {
    CleansingPowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
        ignore_requirements: true,
        power_user_id: PowerUserId::INVALID,
    }
}

fn remove_scenario_squads(world: &mut sim::World) {
    let ids = world.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        world.remove_squad(id).unwrap();
    }
}
