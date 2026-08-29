use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::UnitScene;
use sim::{
    EntityId, NativePowerInput, RagePowerInvocation, load_scenario_from_game_dir,
    power_prototype_id, spawn_squad_at, squad_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-rage-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn rage_hand_and_teleport_attachments_project_only_from_sim_objects() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = load_installed_scenario();
    remove_scenario_squads(&mut simulation.world);
    configure_hostile_players(&mut simulation.world);
    let rage_id =
        power_prototype_id(&content.database, "CovLeaderRage").expect("shipped Rage power");
    let owner_id = spawn_named_squad(
        &mut simulation.world,
        &content.database,
        1,
        "cov_inf_arbiter_01",
        Vec3::ZERO,
    );
    let _target_id = spawn_named_squad(
        &mut simulation.world,
        &content.database,
        2,
        "unsc_inf_marine_01",
        Vec3::X * 20.0,
    );
    let execution_id = simulation
        .world
        .invoke_rage_power(
            &content.database,
            RagePowerInvocation {
                player_id: 1,
                proto_power_id: rage_id,
                power_level: 0,
                squad_id: owner_id,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Rage execution");
    let hand_ids = simulation.world.active_rage_powers()[0]
        .hand_attachment_ids()
        .to_vec();

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert!(hand_ids.iter().all(|id| has_placement(&scene, *id)));
    assert!(scene.roster_matches(&simulation.world));

    assert!(simulation.world.submit_rage_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Direction(Vec3::X),
    ));
    let teleport_id = simulation
        .world
        .objects
        .iter()
        .find_map(|(id, object)| {
            object
                .proto_object_name
                .eq_ignore_ascii_case("fx_rage_teleport_01")
                .then_some(id)
        })
        .expect("sim-owned Rage teleport attachment");
    let leader_id = simulation.world.get_squad(owner_id).unwrap().unit_ids[0];
    assert_eq!(
        simulation
            .world
            .entity_object_state(teleport_id)
            .unwrap()
            .attached_to(),
        Some(leader_id)
    );
    load_active_visuals(&mut content, &mut source, &simulation.world);
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    assert!(has_placement(&scene, teleport_id));
    assert!(hand_ids.iter().all(|id| has_placement(&scene, *id)));
    assert!(scene.roster_matches(&simulation.world));
}

fn load_active_visuals(
    content: &mut pipeline::hw1::World,
    source: &mut AssetSource<StdFileProvider>,
    world: &sim::World,
) {
    let active = render::ugx::simulation_proto_names(world).collect::<Vec<_>>();
    content.load_visuals_for(source, active.iter().copied());
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

fn remove_scenario_squads(world: &mut sim::World) {
    let ids = world.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        world.remove_squad(id).unwrap();
    }
}

fn configure_hostile_players(world: &mut sim::World) {
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
}

fn has_placement(scene: &UnitScene, entity_id: EntityId) -> bool {
    scene
        .placements()
        .iter()
        .any(|placement| placement.entity_id() == entity_id)
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
