//! Checks that the repair field attachment and the reinforced member project from sim state.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use pipeline::database::hw1::Database;
use render::ugx::UnitScene;
use render_test_support::{load_active_visuals, placement};
use sim::{
    EntityId, MS_PER_TICK, RepairPowerInvocation, power_prototype_id, spawn_squad_at,
    squad_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-repair-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn repair_field_attachment_and_reinforced_member_project_from_sim_state() {
    let loaded = test_support::installed_scenario("blood_gulch");
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
    assert!(placement(&scene, field_id).is_some());

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
    assert!(placement(&scene, field_id).is_some());
    assert!(placement(&scene, attachment_id).is_some());
    assert!(scene.roster_matches(&simulation.world));
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
