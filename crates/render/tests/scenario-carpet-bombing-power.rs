//! Checks that the renderer shows carpet bombing impacts and explosions only from live sim state.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use pipeline::database::hw1::Database;
use render::ugx::UnitScene;
use render_test_support::{assert_visual_loaded, load_active_visuals, placement, sync_scene};
use sim::{
    CarpetBombingPowerInvocation, EntityId, NativePowerInput, PowerUserId, power_prototype_id,
};

const CARPET_BOMBING_POWER: &str = "UnscLeaderCarpetBombing";
const BOMBER: &str = "pow_gp_shortsword_01";
const EXPLOSION: &str = "pow_gp_carpetbomb_explosion";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-carpet-bombing-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_carpet_bomber_impacts_and_explosions_only_from_live_sim_state() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = test_support::installed_scenario("blood_gulch");
    let power_id = power_prototype_id(&content.database, CARPET_BOMBING_POWER)
        .expect("shipped Carpet Bombing power");
    let execution_id = simulation
        .world
        .invoke_carpet_bombing_power(&content.database, invocation(power_id))
        .expect("shipped Carpet Bombing execution");
    assert!(simulation.world.submit_carpet_bombing_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Position(Vec3::ZERO),
    ));
    assert!(simulation.world.submit_carpet_bombing_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Direction(Vec3::X),
    ));
    let bomber_id = simulation.world.active_carpet_bombing_powers()[0]
        .bomber_object_id()
        .expect("sim-owned bomber");
    assert_eq!(
        simulation
            .world
            .get_object(bomber_id)
            .map(|object| object.proto_object_name.as_str()),
        Some(BOMBER)
    );

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert_visual_loaded(&scene, bomber_id, BOMBER);

    advance_until(&mut simulation, &content.database, 40, |world| {
        world.active_carpet_bombing_powers()[0].bomb_clusters_dropped() == 4
    });
    let impact_ids = simulation.world.active_carpet_bombing_powers()[0]
        .pending_bombs()
        .iter()
        .map(sim::CarpetBomb::impact_object_id)
        .collect::<Vec<_>>();
    assert_eq!(impact_ids.len(), 8);
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(
        impact_ids
            .iter()
            .all(|impact_id| placement(&scene, *impact_id).is_some())
    );

    advance_until(&mut simulation, &content.database, 40, |world| {
        world
            .objects
            .iter()
            .any(|(_, object)| object.proto_object_name.eq_ignore_ascii_case(EXPLOSION))
    });
    let explosion_ids = simulation
        .world
        .objects
        .iter()
        .filter_map(|(id, object)| {
            object
                .proto_object_name
                .eq_ignore_ascii_case(EXPLOSION)
                .then_some(id)
        })
        .collect::<Vec<_>>();
    assert_ne!(explosion_ids, Vec::<EntityId>::new());
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(
        explosion_ids
            .iter()
            .all(|explosion_id| placement(&scene, *explosion_id).is_some())
    );

    advance_until(&mut simulation, &content.database, 20, |world| {
        world.active_carpet_bombing_powers().is_empty()
    });
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(placement(&scene, bomber_id).is_none());
    assert!(scene.roster_matches(&simulation.world));
}

fn advance_until(
    simulation: &mut sim::LoadedScenario,
    database: &Database,
    maximum_ticks: usize,
    complete: impl Fn(&sim::World) -> bool,
) {
    for _ in 0..maximum_ticks {
        if complete(&simulation.world) {
            return;
        }
        simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(100);
        simulation.world.update_entities_with_database_and_gameplay(
            0.1,
            database,
            &simulation.gameplay,
        );
    }
    assert!(
        complete(&simulation.world),
        "Carpet Bombing state did not converge"
    );
}

fn invocation(proto_power_id: i32) -> CarpetBombingPowerInvocation {
    CarpetBombingPowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
        ignore_requirements: true,
        power_user_id: PowerUserId::INVALID,
    }
}
