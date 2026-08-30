use glam::Vec3;
use render::ugx::{UnitScene, simulation_entity_secondary_transform};
use sim::{EntityId, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id};

const HUNTER: &str = "cov_inf_hunter_01";
const SPIRIT_BOND: &str = "SpiritBond";
const BOND_BEAM: &str = "fx_proj_hunterSpiritBondBeam_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-spirit-bond -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_the_sim_owned_hunter_bond_and_both_endpoints() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = load_installed_scenario();
    let prototype = squad_prototype_id(&content.database, HUNTER)
        .expect("Hunter squad prototype from the layered scenario database");
    let squad_id = spawn_squad_at(
        &mut simulation.world,
        &content.database,
        1,
        prototype,
        Vec3::new(128.0, 0.0, 128.0),
        Vec3::Z,
    )
    .expect("spawn Hunter pair");
    let leader_id = simulation.world.get_squad(squad_id).unwrap().unit_ids[0];
    simulation
        .world
        .get_unit_mut(leader_id)
        .unwrap()
        .actions
        .set_enabled(SPIRIT_BOND, true);
    advance(&mut simulation, &content.database);

    let beam_id = simulation
        .world
        .get_squad(squad_id)
        .and_then(sim::Squad::spirit_bond_beam)
        .expect("sim-owned SpiritBond beam");
    let projected = simulation_entity_secondary_transform(&simulation.world, beam_id)
        .expect("renderer projection of the second endpoint");
    let authoritative = simulation
        .world
        .get_object(beam_id)
        .and_then(sim::Object::visual_secondary_position)
        .expect("sim-owned second endpoint");
    assert_eq!(projected.w_axis.truncate(), authoritative);

    content.load_visuals_for(&mut source, [BOND_BEAM]);
    let scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    let placement = placement(&scene, beam_id).unwrap_or_else(|| {
        panic!(
            "SpiritBond beam was not projected; issues: {:?}",
            scene
                .issues()
                .iter()
                .map(|issue| (issue.proto_name(), issue.reason()))
                .collect::<Vec<_>>()
        )
    });
    assert_eq!(placement.proto_name(), BOND_BEAM);
    assert_eq!(placement.secondary_transform(), Some(projected));
}

fn advance(simulation: &mut sim::LoadedScenario, database: &pipeline::database::hw1::Database) {
    simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(50);
    simulation.world.update_entities_with_database_and_gameplay(
        0.05,
        database,
        &simulation.gameplay,
    );
}

fn placement(scene: &UnitScene, entity_id: EntityId) -> Option<&render::ugx::UnitPlacement> {
    scene
        .placements()
        .iter()
        .find(|placement| placement.entity_id() == entity_id)
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
