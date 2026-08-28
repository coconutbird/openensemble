use glam::Vec3;
use sim::{
    Simulation, SquadContainmentState, WorkCommand, load_scenario_from_game_dir, spawn_squad_at,
    squad_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-teleporter-traversal -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_marine_squad_traverses_the_authored_teleporter_link() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("Blood Gulch and its layered database should load");
    let mut simulation = Simulation::with_seed(0xB100_D6A1);
    simulation.start();
    let endpoints = prepare_endpoints(&mut loaded, &mut simulation);
    let (passenger_squad, passenger_units) = spawn_marine_passenger(&mut loaded, endpoints);
    enqueue_garrison(&mut simulation, passenger_squad, endpoints.source_squad);
    let completed_exit = traverse(
        &mut loaded,
        &mut simulation,
        passenger_squad,
        &passenger_units,
        endpoints,
    );
    assert_completed_traversal(&loaded, &passenger_units, endpoints, completed_exit);
}

#[derive(Debug, Clone, Copy)]
struct Endpoints {
    source_squad: sim::EntityId,
    source_unit: sim::EntityId,
    destination_unit: sim::EntityId,
    source_position: Vec3,
    destination_position: Vec3,
}

fn prepare_endpoints(
    loaded: &mut sim::LoadedGameScenario,
    simulation: &mut Simulation,
) -> Endpoints {
    simulation.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
    let source_squad = scenario_squad(loaded, 87);
    let destination_squad = scenario_squad(loaded, 5);
    let source_unit = loaded
        .simulation
        .world
        .get_squad(source_squad)
        .unwrap()
        .unit_ids[0];
    let destination_unit = loaded
        .simulation
        .world
        .get_squad(destination_squad)
        .unwrap()
        .unit_ids[0];
    let source = loaded.simulation.world.get_unit(source_unit).unwrap();
    assert!(source.garrison.can_contain());
    assert!(source.garrison.is_teleporter());
    assert!(source.garrison.one_squad_containment());
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(source_squad)
            .unwrap()
            .teleporter_destination,
        Some(destination_squad)
    );
    assert_eq!(
        loaded
            .simulation
            .gameplay
            .teleporter_work_range(&source.proto_object_name),
        Some(10.0)
    );
    Endpoints {
        source_squad,
        source_unit,
        destination_unit,
        source_position: source.base.position,
        destination_position: loaded
            .simulation
            .world
            .get_unit(destination_unit)
            .unwrap()
            .base
            .position,
    }
}

fn spawn_marine_passenger(
    loaded: &mut sim::LoadedGameScenario,
    endpoints: Endpoints,
) -> (sim::EntityId, Vec<sim::EntityId>) {
    let marine_proto = squad_prototype_id(&loaded.content.database, "unsc_inf_marine_01")
        .expect("retail Marine squad prototype");
    let squad_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        marine_proto,
        endpoints.source_position + Vec3::new(20.0, 0.0, 0.0),
        Vec3::Z,
    )
    .expect("database-backed Marine squad spawn");
    let unit_ids = loaded
        .simulation
        .world
        .get_squad(squad_id)
        .unwrap()
        .unit_ids
        .clone();
    (squad_id, unit_ids)
}

fn enqueue_garrison(
    simulation: &mut Simulation,
    passenger_squad: sim::EntityId,
    source_squad: sim::EntityId,
) {
    simulation.command_queue.enqueue_work(
        WorkCommand::garrison_squads(1, vec![passenger_squad], source_squad),
        simulation.game_time_ms + sim::MS_PER_TICK,
        1,
    );
}

fn traverse(
    loaded: &mut sim::LoadedGameScenario,
    simulation: &mut Simulation,
    passenger_squad: sim::EntityId,
    passenger_units: &[sim::EntityId],
    endpoints: Endpoints,
) -> Vec3 {
    let mut saw_garrisoned = false;
    let mut saw_teleporter_exit = false;
    for _ in 0..400 {
        simulation.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
        let squad = loaded
            .simulation
            .world
            .get_squad(passenger_squad)
            .expect("live Marine passenger squad");
        match squad.garrison.state() {
            SquadContainmentState::Garrisoned { container, .. } => {
                saw_garrisoned = true;
                assert_eq!(container, endpoints.source_unit);
                assert!(passenger_units.iter().all(|unit_id| {
                    loaded
                        .simulation
                        .world
                        .get_unit(*unit_id)
                        .is_some_and(sim::Unit::is_garrisoned)
                }));
            }
            SquadContainmentState::Ungarrisoning {
                container,
                destination,
                exit_position,
                ..
            } => {
                saw_teleporter_exit = true;
                assert_eq!(container, endpoints.source_unit);
                assert_eq!(destination, endpoints.destination_unit);
                assert_near_destination(exit_position, endpoints);
            }
            SquadContainmentState::Free if saw_teleporter_exit => {
                assert!(saw_garrisoned, "Marine squad skipped containment phase");
                return squad.position();
            }
            _ => {}
        }
    }
    panic!("Marine squad did not finish teleporter traversal");
}

fn assert_completed_traversal(
    loaded: &sim::LoadedGameScenario,
    passenger_units: &[sim::EntityId],
    endpoints: Endpoints,
    completed_exit: Vec3,
) {
    assert_near_destination(completed_exit, endpoints);
    assert!(passenger_units.iter().all(|unit_id| {
        loaded
            .simulation
            .world
            .get_unit(*unit_id)
            .is_some_and(|unit| !unit.is_garrisoned())
    }));
    assert!(
        loaded
            .simulation
            .world
            .get_unit(endpoints.source_unit)
            .unwrap()
            .garrison
            .contained_unit_ids()
            .is_empty()
    );
}

fn assert_near_destination(position: Vec3, endpoints: Endpoints) {
    assert!(position.distance(endpoints.destination_position) < 30.0);
    assert!(position.distance(endpoints.source_position) > 100.0);
}

fn scenario_squad(loaded: &sim::LoadedGameScenario, scenario_id: i32) -> sim::EntityId {
    loaded
        .simulation
        .get_entity_id(scenario_id)
        .unwrap_or_else(|| panic!("scenario object {scenario_id} should map to a squad"))
}
