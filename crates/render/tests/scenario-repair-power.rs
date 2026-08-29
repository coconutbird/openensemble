use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::UnitScene;
use sim::{
    EntityId, MS_PER_TICK, RepairPowerInvocation, load_scenario_from_game_dir, power_prototype_id,
    spawn_squad_at, squad_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-repair-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn repair_field_attachment_and_reinforced_member_project_from_sim_state() {
    let loaded = load_installed_scenario();
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = loaded;
    let repair_id =
        power_prototype_id(&content.database, "UnscLeaderRepair").expect("shipped Repair power");
    let marine_id =
        squad_prototype_id(&content.database, "unsc_inf_marine_01").expect("shipped Marines");
    let squad_id = spawn_squad_at(
        &mut simulation.world,
        &content.database,
        1,
        marine_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .expect("spawn Marines");
    let removed_member = simulation.world.get_squad(squad_id).unwrap().unit_ids[3];
    simulation.world.remove_unit(removed_member).unwrap();
    simulation
        .world
        .invoke_repair_power(
            &content.database,
            RepairPowerInvocation {
                player_id: 1,
                proto_power_id: repair_id,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Repair execution");
    let execution = &simulation.world.active_repair_powers()[0];
    let field_id = execution.repair_object_id();
    let first_tick = execution.next_tick_time_ms();

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert!(has_placement(&scene, field_id));

    advance_to(&mut simulation, &content.database, first_tick);
    let attachment_id = simulation
        .world
        .objects
        .iter()
        .find_map(|(id, object)| {
            object
                .proto_object_name
                .eq_ignore_ascii_case("fx_repairing")
                .then_some(id)
        })
        .expect("sim-owned repair attachment");
    assert_eq!(
        simulation.world.get_squad(squad_id).unwrap().unit_ids.len(),
        4
    );
    load_active_visuals(&mut content, &mut source, &simulation.world);
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    assert!(has_placement(&scene, field_id));
    assert!(has_placement(&scene, attachment_id));
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

fn advance_to(simulation: &mut sim::LoadedScenario, database: &Database, target_time: u32) {
    while simulation.world.game_time_ms < target_time {
        simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(MS_PER_TICK);
        simulation.world.update_entities_with_database_and_gameplay(
            0.05,
            database,
            &simulation.gameplay,
        );
    }
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
