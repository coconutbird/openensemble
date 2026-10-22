//! Checks that the renderer shows only the live ODST pod and the revealed sim members.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use render::ugx::UnitScene;
use render_test_support::{assert_visual_loaded, load_active_visuals, placement, sync_scene};
use sim::{EntityId, NativePowerInput, OdstPowerInvocation, PowerUserId, power_prototype_id};

const ODST_POWER: &str = "UnscOdstDrop";
const ODST_TECH: &str = "unsc_odst_upgrade1";
const ODST_POD: &str = "unsc_air_odstPod_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-odst-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_only_the_live_odst_pod_and_revealed_sim_members() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = test_support::installed_scenario("blood_gulch");
    remove_scenario_squads(&mut simulation.world);
    let power_id = power_prototype_id(&content.database, ODST_POWER).expect("shipped ODST power");
    assert!(
        simulation
            .world
            .activate_technology(1, &content.database, ODST_TECH)
            .unwrap()
    );
    let execution_id = simulation
        .world
        .invoke_odst_power(&content.database, invocation(power_id))
        .expect("shipped ODST execution");
    assert!(simulation.world.submit_odst_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert!(simulation.world.submit_odst_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Shutdown,
        false,
    ));
    let squad_id = simulation.world.active_odst_powers()[0].active_drops()[0].squad_id();
    let member_ids = simulation
        .world
        .get_squad(squad_id)
        .unwrap()
        .unit_ids
        .clone();

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert!(member_ids.iter().all(|id| placement(&scene, *id).is_none()));
    assert!(scene.roster_matches(&simulation.world));

    advance(&mut simulation, &content.database, 0.05);
    let projectile_id = simulation.world.active_odst_powers()[0].active_drops()[0].projectile_id();
    assert_eq!(
        simulation
            .world
            .get_projectile(projectile_id)
            .unwrap()
            .proto_object_name,
        ODST_POD
    );
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert_visual_loaded(&scene, projectile_id, ODST_POD);
    assert!(member_ids.iter().all(|id| placement(&scene, *id).is_none()));

    advance(&mut simulation, &content.database, 0.701);
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(member_ids.iter().all(|id| placement(&scene, *id).is_some()));
    assert!(member_ids.iter().all(|id| {
        placement(&scene, *id).is_some_and(|placement| {
            simulation
                .world
                .get_unit(*id)
                .is_some_and(|unit| placement.proto_name() == unit.proto_object_name)
        })
    }));
    assert!(scene.roster_matches(&simulation.world));
}

fn advance(
    simulation: &mut sim::LoadedScenario,
    database: &pipeline::database::hw1::Database,
    dt: f32,
) {
    simulation
        .world
        .update_entities_with_database_and_gameplay(dt, database, &simulation.gameplay);
}

fn invocation(proto_power_id: i32) -> OdstPowerInvocation {
    OdstPowerInvocation {
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
