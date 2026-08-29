use glam::Vec3;
use render::ugx::UnitScene;
use sim::{
    DisruptionPowerInvocation, EntityId, MS_PER_TICK, load_scenario_from_game_dir,
    power_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-disruption-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn disruption_bomber_field_and_pulses_project_from_authoritative_sim_objects() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and database");
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = loaded;
    let power_id = power_prototype_id(&content.database, "UnscLeaderDisruption")
        .expect("shipped Disruption power");
    simulation
        .world
        .invoke_disruption_power(
            &content.database,
            DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: power_id,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Disruption execution");
    let bomber_id = simulation.world.active_disruption_powers()[0].bomber_object_id();
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

    for _ in 0..60 {
        simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(MS_PER_TICK);
        simulation.world.update_entities_with_database_and_gameplay(
            0.05,
            &content.database,
            &simulation.gameplay,
        );
        if simulation.world.active_disruption_powers()[0].is_active() {
            break;
        }
    }
    let execution = &simulation.world.active_disruption_powers()[0];
    assert!(execution.is_active());
    let field_id = execution.disruption_object_id();
    let pulse_id = simulation
        .world
        .objects
        .iter()
        .find_map(|(id, object)| {
            object
                .proto_object_name
                .eq_ignore_ascii_case("fx_disruptionRing")
                .then_some(id)
        })
        .expect("sim-owned pulse attachment");
    let active_proto_names =
        render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
    content.load_visuals_for(&mut source, active_proto_names.iter().copied());
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    assert!(has_placement(&scene, field_id));
    assert!(has_placement(&scene, pulse_id));
    assert!(scene.roster_matches(&simulation.world));
}

fn has_placement(scene: &UnitScene, entity_id: EntityId) -> bool {
    scene
        .placements()
        .iter()
        .any(|placement| placement.entity_id() == entity_id)
}
