use glam::Vec3;
use render::ugx::UnitScene;
use sim::{
    CryoPowerInvocation, EntityId, MS_PER_TICK, load_scenario_from_game_dir, power_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-cryo-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn cryo_bomber_and_effect_are_projected_from_authoritative_sim_objects() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and database");
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = loaded;
    let power_id =
        power_prototype_id(&content.database, "UnscLeaderCryo").expect("shipped Cryo power");
    simulation
        .world
        .invoke_cryo_power(
            &content.database,
            CryoPowerInvocation {
                player_id: 1,
                proto_power_id: power_id,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Cryo execution");
    let execution = &simulation.world.active_cryo_powers()[0];
    let bomber_id = execution.bomber_object_id();
    let effect_name = execution.cryo_object_prototype().to_owned();
    let active_proto_names =
        render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
    content.load_visuals_for(&mut source, active_proto_names.iter().copied());
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert!(has_placement(&scene, bomber_id));

    while !simulation
        .world
        .objects
        .iter()
        .any(|(_, object)| object.proto_object_name.eq_ignore_ascii_case(&effect_name))
    {
        simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(MS_PER_TICK);
        simulation.world.update_entities_with_database_and_gameplay(
            0.05,
            &content.database,
            &simulation.gameplay,
        );
    }
    let effect_id = simulation
        .world
        .objects
        .iter()
        .find_map(|(object_id, object)| {
            object
                .proto_object_name
                .eq_ignore_ascii_case(&effect_name)
                .then_some(object_id)
        })
        .expect("Cryo effect sim object");
    let active_proto_names =
        render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
    content.load_visuals_for(&mut source, active_proto_names.iter().copied());
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    assert!(has_placement(&scene, effect_id));
    assert!(scene.roster_matches(&simulation.world));
}

fn has_placement(scene: &UnitScene, entity_id: EntityId) -> bool {
    scene
        .placements()
        .iter()
        .any(|placement| placement.entity_id() == entity_id)
}
