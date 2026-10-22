//! Checks that the renderer shows orbital beams, the effect and moving debris from live sim state.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use pipeline::database::hw1::Database;
use render::ugx::{UnitScene, simulation_entity_transform};
use render_test_support::{assert_visual_loaded, load_active_visuals, placement, sync_scene};
use sim::{EntityId, NativePowerInput, OrbitalPowerInvocation, PowerUserId, power_prototype_id};

const ORBITAL_POWER: &str = "UnscLeaderOrbitalBombard";
const TARGET_BEAM: &str = "fx_proj_maccannontargetbeam_01";
const EFFECT: &str = "pow_gp_macCannonVisualSmall";
const ROCKS: [&str; 3] = [
    "pow_gp_macblast_rocks_small",
    "pow_gp_macblast_rocks_medium",
    "pow_gp_macblast_rocks_large",
];

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-orbital-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_orbital_beams_effect_and_moving_debris_from_live_sim_state() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = test_support::installed_scenario("blood_gulch");
    remove_scenario_squads(&mut simulation.world);
    let power_id =
        power_prototype_id(&content.database, ORBITAL_POWER).expect("shipped Orbital power");
    let execution_id = simulation
        .world
        .invoke_orbital_power(&content.database, invocation(power_id))
        .expect("shipped Orbital execution");
    let beam_id = simulation.world.active_orbital_powers()[0].real_targeting_laser_id();

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert_visual_loaded(&scene, beam_id, TARGET_BEAM);
    assert!(scene.roster_matches(&simulation.world));

    assert!(simulation.world.submit_orbital_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
    ));
    assert!(simulation.world.get_object(beam_id).is_none());
    advance(&mut simulation, &content.database, 1);
    let shot_laser_id =
        simulation.world.active_orbital_powers()[0].pending_shots()[0].laser_object_id();
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert_visual_loaded(&scene, shot_laser_id, TARGET_BEAM);
    assert!(placement(&scene, beam_id).is_none());

    advance_until(&mut simulation, &content.database, 20, |world| {
        world
            .objects
            .iter()
            .any(|(_, object)| object.proto_object_name.eq_ignore_ascii_case(EFFECT))
    });
    let effect_id = object_ids(&simulation.world, &[EFFECT])[0];
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert_visual_loaded(&scene, effect_id, EFFECT);
    assert!(placement(&scene, shot_laser_id).is_none());

    advance_until(&mut simulation, &content.database, 20, |world| {
        !object_ids(world, &ROCKS).is_empty()
    });
    let debris_ids = object_ids(&simulation.world, &ROCKS);
    assert!((13..=23).contains(&debris_ids.len()));
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(debris_ids.iter().all(|id| placement(&scene, *id).is_some()));
    assert!(scene.roster_matches(&simulation.world));

    let moving_id = debris_ids[0];
    let old_transform = simulation_entity_transform(&simulation.world, moving_id).unwrap();
    advance(&mut simulation, &content.database, 1);
    let new_transform = simulation_entity_transform(&simulation.world, moving_id).unwrap();
    assert_ne!(new_transform, old_transform);
    assert_eq!(
        new_transform.w_axis.truncate(),
        simulation
            .world
            .get_object(moving_id)
            .unwrap()
            .base
            .position
    );

    advance_until(&mut simulation, &content.database, 110, |world| {
        object_ids(world, &ROCKS).is_empty()
    });
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(debris_ids.iter().all(|id| placement(&scene, *id).is_none()));
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
        advance(simulation, database, 1);
    }
    assert!(
        complete(&simulation.world),
        "Orbital state did not converge"
    );
}

fn object_ids(world: &sim::World, names: &[&str]) -> Vec<EntityId> {
    world
        .objects
        .iter()
        .filter(|(_, object)| {
            names
                .iter()
                .any(|name| object.proto_object_name.eq_ignore_ascii_case(name))
        })
        .map(|(id, _)| id)
        .collect()
}

fn invocation(proto_power_id: i32) -> OrbitalPowerInvocation {
    OrbitalPowerInvocation {
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
