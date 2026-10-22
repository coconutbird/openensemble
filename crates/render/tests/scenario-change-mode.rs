//! Checks that the renderer plays the change-mode clip that the sim selects.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use render::ugx::{UnitPlacement, UnitScene, simulation_proto_names};
use sim::{
    CommandEntry, CommandExecutor, QueuedCommand, WorkCommand, load_scenario_from_game_dir,
    spawn_squad_at, squad_prototype_id,
};

const COBRA: &str = "unsc_veh_cobra_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-change-mode -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_the_exact_sim_selected_change_mode_clip() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load");
    let origin = scenario_center(&loaded.simulation.world);
    let cobra_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        squad_prototype_id(&loaded.content.database, COBRA).expect("installed Cobra squad"),
        origin,
        Vec3::X,
    )
    .expect("Cobra squad should spawn");
    let cobra_id = loaded
        .simulation
        .world
        .get_squad(cobra_squad)
        .unwrap()
        .unit_ids[0];

    load_active_visuals(&mut loaded);
    let mut scene = UnitScene::load_world_with_gameplay(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.simulation.gameplay,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    );
    issue_change_mode(&mut loaded, cobra_squad);
    loaded
        .simulation
        .world
        .update_entities_with_gameplay(0.05, &loaded.simulation.gameplay);
    let selected = loaded
        .simulation
        .world
        .entity_scripted_animation(cobra_id)
        .expect("sim-selected Cobra mode animation");
    assert_eq!(selected.animation_type(), "AnchorDown");
    let animation_asset = selected
        .asset_path()
        .expect("scenario-loaded mode clip")
        .to_owned();
    let checksum = loaded.simulation.world.checksum();

    assert!(
        !scene.roster_matches_with_gameplay(&loaded.simulation.world, &loaded.simulation.gameplay)
    );
    assert!(scene.sync_world_with_gameplay(
        &mut loaded.source,
        &loaded.simulation.world,
        &loaded.simulation.gameplay,
        &loaded.content.visuals,
        &loaded.content.database.objects,
    ));
    assert_eq!(loaded.simulation.world.checksum(), checksum);

    let placement = find_placement(&scene, cobra_id);
    assert_eq!(placement.animation_type(), Some("AnchorDown"));
    assert_eq!(placement.animation_asset(), Some(animation_asset.as_str()));
    assert!(placement.animation_uses_simulation_clock());
    assert!(placement.unit().has_scripted_animation());
}

fn load_active_visuals(loaded: &mut sim::LoadedGameScenario) {
    let names = simulation_proto_names(&loaded.simulation.world).collect::<Vec<_>>();
    loaded
        .content
        .load_visuals_for(&mut loaded.source, names.iter().copied());
}

fn issue_change_mode(loaded: &mut sim::LoadedGameScenario, squad_id: sim::EntityId) {
    let command = WorkCommand::change_mode_squads(
        1,
        vec![squad_id],
        None,
        loaded.simulation.gameplay.command_ability_id(),
    );
    CommandExecutor::with_database_and_gameplay(
        &loaded.content.database,
        &loaded.simulation.gameplay,
    )
    .execute(
        &mut loaded.simulation.world,
        &CommandEntry {
            command: QueuedCommand::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn find_placement(scene: &UnitScene, entity_id: sim::EntityId) -> &UnitPlacement {
    scene
        .placements()
        .iter()
        .find(|placement| placement.entity_id() == entity_id)
        .expect("Cobra placement")
}

fn scenario_center(world: &sim::World) -> Vec3 {
    let bounds = world.terrain_bounds().expect("scenario terrain bounds");
    let mut center = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    center.y = world.terrain_height(center, true).unwrap_or_default();
    center
}
