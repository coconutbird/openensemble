use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::{UnitScene, simulation_entity_transform};
use sim::{
    EntityId, NativePowerInput, OrbitalPowerInvocation, PowerUserId, load_scenario_from_game_dir,
    power_prototype_id,
};

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
    } = load_installed_scenario();
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
    assert!(!has_placement(&scene, beam_id));

    advance_until(&mut simulation, &content.database, 20, |world| {
        world
            .objects
            .iter()
            .any(|(_, object)| object.proto_object_name.eq_ignore_ascii_case(EFFECT))
    });
    let effect_id = object_ids(&simulation.world, &[EFFECT])[0];
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert_visual_loaded(&scene, effect_id, EFFECT);
    assert!(!has_placement(&scene, shot_laser_id));

    advance_until(&mut simulation, &content.database, 20, |world| {
        !object_ids(world, &ROCKS).is_empty()
    });
    let debris_ids = object_ids(&simulation.world, &ROCKS);
    assert!((13..=23).contains(&debris_ids.len()));
    sync_scene(&mut scene, &mut content, &mut source, &simulation.world);
    assert!(debris_ids.iter().all(|id| has_placement(&scene, *id)));
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
    assert!(debris_ids.iter().all(|id| !has_placement(&scene, *id)));
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

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
