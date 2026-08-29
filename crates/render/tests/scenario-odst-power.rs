use glam::Vec3;
use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::UnitScene;
use sim::{
    EntityId, NativePowerInput, OdstPowerInvocation, PowerUserId, load_scenario_from_game_dir,
    power_prototype_id,
};

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
    } = load_installed_scenario();
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
    assert!(member_ids.iter().all(|id| !has_placement(&scene, *id)));
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
    assert!(member_ids.iter().all(|id| !has_placement(&scene, *id)));

    advance(&mut simulation, &content.database, 0.701);
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(member_ids.iter().all(|id| has_placement(&scene, *id)));
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

fn sync_scene(
    scene: &mut UnitScene,
    content: &mut pipeline::hw1::World,
    source: &mut AssetSource<StdFileProvider>,
    world: &sim::World,
) {
    load_active_visuals(content, source, world);
    assert!(scene.sync_world(source, world, &content.visuals, &content.database.objects));
    assert!(scene.roster_matches(world));
}

fn load_active_visuals(
    content: &mut pipeline::hw1::World,
    source: &mut AssetSource<StdFileProvider>,
    world: &sim::World,
) {
    let active = render::ugx::simulation_proto_names(world).collect::<Vec<_>>();
    content.load_visuals_for(source, active.iter().copied());
}

fn assert_visual_loaded(scene: &UnitScene, entity_id: EntityId, proto_name: &str) {
    assert!(
        placement(scene, entity_id).is_some_and(|placement| placement.proto_name() == proto_name),
        "{proto_name} was not projected; issues: {:?}",
        scene
            .issues()
            .iter()
            .map(|issue| (issue.proto_name(), issue.reason()))
            .collect::<Vec<_>>()
    );
}

fn placement(scene: &UnitScene, entity_id: EntityId) -> Option<&render::ugx::UnitPlacement> {
    scene
        .placements()
        .iter()
        .find(|placement| placement.entity_id() == entity_id)
}

fn has_placement(scene: &UnitScene, entity_id: EntityId) -> bool {
    placement(scene, entity_id).is_some()
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

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
