use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::{UnitScene, simulation_entity_transform};
use sim::{
    EntityId, NativePowerInput, PowerUserId, WavePowerInvocation, load_scenario_from_game_dir,
    power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const WAVE_POWER: &str = "CovLeaderWave";
const BRUTE_CHIEFTAIN: &str = "cov_inf_bruteChief_01";
const MARINES: &str = "unsc_inf_marine_01";
const BALL: &str = "pow_gp_wave_01";
const LIGHTNING_BEAM: &str = "pow_proj_wave_lightning_beam_01";
const DEBRIS: &str = "pow_proj_wave_debris_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-wave-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_wave_and_marines_from_live_sim_state() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = load_installed_scenario();
    remove_scenario_squads(&mut simulation.world);
    configure_enemies(&mut simulation.world);
    let center = playable_center(&simulation.world);
    let owner_id = spawn_named_squad(
        &mut simulation.world,
        &content.database,
        1,
        BRUTE_CHIEFTAIN,
        center - Vec3::X * 20.0,
    );
    let target_id = spawn_named_squad(&mut simulation.world, &content.database, 2, MARINES, center);
    let marine_id = simulation.world.get_squad(target_id).unwrap().unit_ids[0];
    let target = simulation.world.get_squad(target_id).unwrap().base.position;
    let power_id = power_prototype_id(&content.database, WAVE_POWER).expect("shipped Wave power");
    let execution_id = simulation
        .world
        .invoke_wave_power(&content.database, invocation(power_id, owner_id, target))
        .expect("shipped Wave execution");
    let ball_id = simulation.world.active_wave_powers()[0].ball_object_id();
    assert_eq!(ball_id.class(), Some(sim::EntityClass::Unit));

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert_visual_loaded(&scene, ball_id, BALL);
    assert_visual_loaded(&scene, marine_id, MARINES);
    assert!(scene.roster_matches(&simulation.world));
    let initial_transform = simulation_entity_transform(&simulation.world, ball_id).unwrap();

    assert!(simulation.world.submit_wave_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Position(target + Vec3::X * 8.0),
    ));
    advance(&mut simulation, &content.database, 1);
    let beam_id = find_prototype(&simulation.world, LIGHTNING_BEAM).expect("sim lightning beam");
    let moved_transform = simulation_entity_transform(&simulation.world, ball_id).unwrap();
    assert_ne!(moved_transform, initial_transform);
    assert_eq!(
        moved_transform.w_axis.truncate(),
        simulation.world.get_unit(ball_id).unwrap().base.position
    );
    assert!(!scene.roster_matches(&simulation.world));
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert_eq!(
        placement(&scene, ball_id).unwrap().transform(),
        moved_transform
    );
    assert_visual_loaded(&scene, beam_id, LIGHTNING_BEAM);
    assert_eq!(
        placement(&scene, beam_id).unwrap().transform(),
        simulation_entity_transform(&simulation.world, beam_id).unwrap()
    );

    assert!(simulation.world.submit_wave_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Confirm(target),
    ));
    advance(&mut simulation, &content.database, 17);
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(!has_placement(&scene, ball_id));
    assert!(!has_placement(&scene, beam_id));
    assert!(simulation.world.active_wave_powers().is_empty());
    assert!(scene.roster_matches(&simulation.world));
}

#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_captured_marine_visual_from_debris_sim_state() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = load_installed_scenario();
    remove_scenario_squads(&mut simulation.world);
    configure_enemies(&mut simulation.world);
    let center = playable_center(&simulation.world);
    let owner_id = spawn_named_squad(
        &mut simulation.world,
        &content.database,
        1,
        BRUTE_CHIEFTAIN,
        center - Vec3::X * 20.0,
    );
    let target_id = spawn_named_squad(&mut simulation.world, &content.database, 2, MARINES, center);
    let marine_id = simulation.world.get_squad(target_id).unwrap().unit_ids[0];
    simulation.world.get_unit_mut(marine_id).unwrap().kill();
    let power_id = power_prototype_id(&content.database, WAVE_POWER).expect("shipped Wave power");
    let execution_id = simulation
        .world
        .invoke_wave_power(&content.database, invocation(power_id, owner_id, center))
        .expect("shipped Wave execution");

    advance(&mut simulation, &content.database, 4);
    let [captured] = simulation.world.active_wave_powers()[0].captured_objects() else {
        panic!("dead Marine should be captured before explosion");
    };
    let captured_id = captured.unit_id();
    let captured_unit = simulation.world.get_unit(captured_id).unwrap();
    assert_eq!(captured_unit.proto_object_name, MARINES);
    let source_center_offset = captured_unit
        .physics
        .as_ref()
        .unwrap()
        .collider()
        .center_offset;
    assert!(source_center_offset.length_squared() > 0.0);
    assert!(simulation.world.submit_wave_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Confirm(center),
    ));
    advance(&mut simulation, &content.database, 1);
    let debris_id = find_projectile(&simulation.world, DEBRIS).expect("Wave debris projectile");
    let debris = simulation.world.get_projectile(debris_id).unwrap();
    assert_eq!(debris.visual_proto_object_name(), MARINES);
    assert_eq!(debris.visual_center_offset(), -source_center_offset);
    let transform = simulation_entity_transform(&simulation.world, debris_id).unwrap();
    let expected_visual_position =
        debris.base.position + transform.transform_vector3(debris.visual_center_offset());
    assert!(
        transform
            .transform_point3(Vec3::ZERO)
            .abs_diff_eq(expected_visual_position, 0.000_1)
    );

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert_visual_loaded(&scene, debris_id, MARINES);
    assert_eq!(placement(&scene, debris_id).unwrap().transform(), transform);
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

fn find_prototype(world: &sim::World, prototype: &str) -> Option<EntityId> {
    world.objects.iter().find_map(|(id, object)| {
        object
            .proto_object_name
            .eq_ignore_ascii_case(prototype)
            .then_some(id)
    })
}

fn find_projectile(world: &sim::World, prototype: &str) -> Option<EntityId> {
    world.projectiles.iter().find_map(|(id, projectile)| {
        projectile
            .proto_object_name
            .eq_ignore_ascii_case(prototype)
            .then_some(id)
    })
}

fn invocation(
    proto_power_id: i32,
    squad_id: EntityId,
    target_location: Vec3,
) -> WavePowerInvocation {
    WavePowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id,
        target_location,
        ignore_requirements: true,
        power_user_id: PowerUserId::INVALID,
    }
}

fn spawn_named_squad(
    world: &mut sim::World,
    database: &Database,
    player_id: u8,
    name: &str,
    mut position: Vec3,
) -> EntityId {
    let prototype_id = squad_prototype_id(database, name).expect("shipped squad prototype");
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    spawn_squad_at(world, database, player_id, prototype_id, position, Vec3::Z)
        .expect("spawn shipped squad")
}

fn configure_enemies(world: &mut sim::World) {
    world.get_player_mut(1).expect("player one").team_id = 1;
    world.get_player_mut(2).expect("player two").team_id = 2;
    world.configure_standard_team_relations();
}

fn playable_center(world: &sim::World) -> Vec3 {
    let bounds = world
        .effective_playable_bounds()
        .expect("scenario terrain bounds");
    let mut center = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    center.y = world
        .terrain_height(center, true)
        .expect("terrain at playable center");
    center
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
