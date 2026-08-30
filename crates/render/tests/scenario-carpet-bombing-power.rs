use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::UnitScene;
use sim::{
    CarpetBombingPowerInvocation, EntityId, NativePowerInput, PowerUserId,
    load_scenario_from_game_dir, power_prototype_id,
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
    } = load_installed_scenario();
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
            .all(|impact_id| has_placement(&scene, *impact_id))
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
    assert!(!explosion_ids.is_empty());
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(
        explosion_ids
            .iter()
            .all(|explosion_id| has_placement(&scene, *explosion_id))
    );

    advance_until(&mut simulation, &content.database, 20, |world| {
        world.active_carpet_bombing_powers().is_empty()
    });
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(!has_placement(&scene, bomber_id));
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

fn sync_scene(
    scene: &mut UnitScene,
    content: &mut pipeline::hw1::World,
    source: &mut AssetSource<StdFileProvider>,
    world: &sim::World,
) {
    load_active_visuals(content, source, world);
    assert!(scene.sync_world(source, world, &content.visuals, &content.database.objects,));
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

fn assert_visual_loaded(scene: &UnitScene, entity_id: EntityId, prototype: &str) {
    assert!(
        placement(scene, entity_id).is_some_and(|placement| placement.proto_name() == prototype),
        "{prototype} was not projected; issues: {:?}",
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

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
