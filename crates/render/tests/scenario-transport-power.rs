//! Checks that the renderer shows transport visibility and motion from live sim state.
//!
//! The test needs a local Halo Wars DE installation.

use glam::Vec3;
use pipeline::database::hw1::Database;
use render::ugx::{UnitScene, simulation_entity_transform};
use render_test_support::{assert_visual_loaded, load_active_visuals, placement};
use sim::{
    EntityId, NativePowerInput, PowerTransportPhase, PowerUserId, TransportPowerInvocation,
    power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const TRANSPORT_POWER: &str = "UnscCivTransport";
const MARINES: &str = "unsc_inf_marine_01";
const PELICAN: &str = "unsc_air_pelican_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p render --test scenario-transport-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn renderer_projects_transport_visibility_and_motion_from_live_sim_state() {
    let sim::LoadedGameScenario {
        mut simulation,
        mut content,
        mut source,
    } = test_support::installed_scenario("blood_gulch");
    remove_scenario_squads(&mut simulation.world);
    assert_player_one_is_cutter(&simulation.world, &content.database);
    let marine_id = spawn_named_squad(
        &mut simulation.world,
        &content.database,
        1,
        MARINES,
        Vec3::ZERO,
    );
    let marine_units = simulation
        .world
        .get_squad(marine_id)
        .unwrap()
        .unit_ids
        .clone();
    let power_id = power_prototype_id(&content.database, TRANSPORT_POWER).unwrap();
    let execution_id = simulation
        .world
        .invoke_transport_power(&content.database, invocation(power_id))
        .expect("shipped Transport execution");
    assert!(simulation.world.submit_transport_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert!(simulation.world.submit_transport_power_input(
        &content.database,
        execution_id,
        NativePowerInput::Confirm(Vec3::X * 60.0),
        false,
    ));
    let carrier_id = simulation
        .world
        .squads
        .iter()
        .find_map(|(squad_id, squad)| squad.power_transport().is_some().then_some(squad_id))
        .expect("sim-owned Pelican carrier");
    let carrier_unit_id = simulation.world.get_squad(carrier_id).unwrap().unit_ids[0];

    load_active_visuals(&mut content, &mut source, &simulation.world);
    let mut scene = UnitScene::load_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    );
    assert_visual_loaded(&scene, carrier_unit_id, PELICAN);
    assert!(
        marine_units
            .iter()
            .all(|unit_id| placement(&scene, *unit_id).is_some())
    );
    assert_rendered(&simulation.world, carrier_unit_id, true);
    assert_members_rendered(&simulation.world, &marine_units, true);

    advance_until(&mut simulation, &content.database, 500, |world| {
        world.get_squad(carrier_id).is_some_and(|carrier| {
            carrier
                .power_transport()
                .is_some_and(|action| action.phase() == PowerTransportPhase::Transporting)
        })
    });
    assert!(scene.roster_matches(&simulation.world));
    assert_rendered(&simulation.world, carrier_unit_id, true);
    assert_members_rendered(&simulation.world, &marine_units, false);

    advance_until(&mut simulation, &content.database, 500, |world| {
        world.get_squad(carrier_id).is_some_and(|carrier| {
            carrier
                .power_transport()
                .is_some_and(|action| action.phase() == PowerTransportPhase::Outgoing)
        })
    });
    assert_members_rendered(&simulation.world, &marine_units, true);
    assert!(scene.roster_matches(&simulation.world));

    advance_until(&mut simulation, &content.database, 500, |world| {
        world.get_squad(carrier_id).is_none()
    });
    load_active_visuals(&mut content, &mut source, &simulation.world);
    assert!(scene.sync_world(
        &mut source,
        &simulation.world,
        &content.visuals,
        &content.database.objects,
    ));
    assert!(placement(&scene, carrier_unit_id).is_none());
    assert!(scene.roster_matches(&simulation.world));
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
        simulation.world.update_entities_with_database_and_gameplay(
            0.1,
            database,
            &simulation.gameplay,
        );
    }
    assert!(
        complete(&simulation.world),
        "transport state did not converge"
    );
}

fn assert_rendered(world: &sim::World, entity_id: EntityId, expected: bool) {
    assert_eq!(
        simulation_entity_transform(world, entity_id).is_some(),
        expected
    );
}

fn assert_members_rendered(world: &sim::World, members: &[EntityId], expected: bool) {
    assert!(
        members
            .iter()
            .all(|unit_id| { simulation_entity_transform(world, *unit_id).is_some() == expected })
    );
}

/// The default 1v1 game settings give player 1 the Cutter leader.
fn assert_player_one_is_cutter(world: &sim::World, database: &Database) {
    let leader_id = database
        .leaders
        .iter()
        .position(|leader| leader.name.eq_ignore_ascii_case("Cutter"))
        .and_then(|index| i32::try_from(index).ok())
        .expect("shipped Cutter leader");
    assert_eq!(world.get_player(1).expect("player 1").leader_id, leader_id);
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
