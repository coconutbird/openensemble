use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    EntityId, NativePowerInput, PowerTransportPhase, PowerUserId, TransportPowerInvocation,
    configure_player_leader, load_scenario_from_game_dir, power_prototype_id, spawn_squad_at,
    squad_prototype_id,
};

const TRANSPORT_POWER: &str = "UnscCivTransport";
const MARINES: &str = "unsc_inf_marine_01";
const PELICAN: &str = "unsc_air_pelican_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-transport-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_transport_profile_carries_a_layered_marine_squad_to_dropoff() {
    let mut loaded = load_installed_scenario();
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_cutter(&mut loaded);
    let database = &loaded.content.database;
    assert_shipped_transport_data(database);
    let marine_id = spawn_named_squad(
        &mut loaded.simulation.world,
        database,
        1,
        MARINES,
        Vec3::ZERO,
    );
    let power_id = power_prototype_id(database, TRANSPORT_POWER).expect("shipped Transport power");
    let execution_id = loaded
        .simulation
        .world
        .invoke_transport_power(database, invocation(power_id))
        .expect("shipped Transport profile should fully resolve");
    let execution = &loaded.simulation.world.active_transport_powers()[0];
    assert_close(execution.ui_radius(), 30.0);
    assert_close(execution.min_transport_distance(), 45.0);
    assert_eq!(execution.transport_prototype(), PELICAN);

    assert!(loaded.simulation.world.submit_transport_power_input(
        database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert_eq!(
        loaded.simulation.world.active_transport_powers()[0].selected_squad_ids(),
        &[marine_id]
    );
    let dropoff = Vec3::X * 60.0;
    assert!(loaded.simulation.world.submit_transport_power_input(
        database,
        execution_id,
        NativePowerInput::Confirm(dropoff),
        false,
    ));
    assert!(loaded.simulation.world.active_transport_powers().is_empty());

    let carrier_id = loaded
        .simulation
        .world
        .squads
        .iter()
        .find_map(|(squad_id, squad)| squad.power_transport().is_some().then_some(squad_id))
        .expect("sim-owned Pelican carrier");
    let carrier = loaded.simulation.world.get_squad(carrier_id).unwrap();
    assert_eq!(carrier.proto_squad_name, PELICAN);
    assert_eq!(
        carrier.power_transport().unwrap().passenger_squad_ids(),
        &[marine_id]
    );

    advance_until(&mut loaded, 500, |world| {
        world.get_squad(carrier_id).is_some_and(|carrier| {
            carrier
                .power_transport()
                .is_some_and(|action| action.phase() == PowerTransportPhase::Transporting)
        })
    });
    assert!(
        loaded
            .simulation
            .world
            .get_squad(marine_id)
            .unwrap()
            .garrison
            .is_garrisoned()
    );
    assert!(marine_units(&loaded.simulation.world, marine_id).all(sim::Unit::is_garrisoned));

    advance_until(&mut loaded, 500, |world| {
        world.get_squad(carrier_id).is_some_and(|carrier| {
            carrier
                .power_transport()
                .is_some_and(|action| action.phase() == PowerTransportPhase::Outgoing)
        })
    });
    let marine = loaded.simulation.world.get_squad(marine_id).unwrap();
    assert!(!marine.garrison.is_garrisoned());
    assert_close(marine.base.position.x, dropoff.x);
    assert!(marine_units(&loaded.simulation.world, marine_id).all(|unit| !unit.is_garrisoned()));

    advance_until(&mut loaded, 500, |world| {
        world.get_squad(carrier_id).is_none()
    });
}

fn advance_until(
    loaded: &mut sim::LoadedGameScenario,
    maximum_ticks: usize,
    complete: impl Fn(&sim::World) -> bool,
) {
    for _ in 0..maximum_ticks {
        if complete(&loaded.simulation.world) {
            return;
        }
        loaded
            .simulation
            .world
            .update_entities_with_database_and_gameplay(
                0.1,
                &loaded.content.database,
                &loaded.simulation.gameplay,
            );
    }
    assert!(
        complete(&loaded.simulation.world),
        "transport state did not converge"
    );
}

fn marine_units(world: &sim::World, squad_id: EntityId) -> impl Iterator<Item = &sim::Unit> {
    world
        .get_squad(squad_id)
        .into_iter()
        .flat_map(|squad| &squad.unit_ids)
        .filter_map(|unit_id| world.get_unit(*unit_id))
}

fn assert_shipped_transport_data(database: &Database) {
    let game_data = database.game_data.as_ref().expect("layered GameData");
    assert_eq!(game_data.transport_max, Some(3));
    assert_eq!(game_data.transport_incoming_height, Some(40.0));
    assert_eq!(game_data.transport_incoming_offset, Some(60.0));
    assert_eq!(game_data.transport_outgoing_height, Some(120.0));
    assert_eq!(game_data.transport_outgoing_offset, Some(60.0));
    assert_eq!(game_data.transport_pickup_height, Some(8.0));
    assert_eq!(game_data.transport_dropoff_height, Some(15.0));
}

fn configure_cutter(loaded: &mut sim::LoadedGameScenario) {
    let leader_id = loaded
        .content
        .database
        .leaders
        .iter()
        .position(|leader| leader.name.eq_ignore_ascii_case("Cutter"))
        .and_then(|index| i32::try_from(index).ok())
        .expect("shipped Cutter leader");
    assert!(configure_player_leader(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        leader_id,
    ));
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

fn invocation(proto_power_id: i32) -> TransportPowerInvocation {
    TransportPowerInvocation {
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

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
