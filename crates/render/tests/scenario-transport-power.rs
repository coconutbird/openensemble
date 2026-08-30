use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::source::{AssetSource, StdFileProvider};
use render::ugx::{UnitScene, simulation_entity_transform};
use sim::{
    EntityId, NativePowerInput, PowerTransportPhase, PowerUserId, TransportPowerInvocation,
    configure_player_leader, load_scenario_from_game_dir, power_prototype_id, spawn_squad_at,
    squad_prototype_id,
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
    } = load_installed_scenario();
    remove_scenario_squads(&mut simulation.world);
    configure_cutter(&mut simulation.world, &content.database);
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
            .all(|unit_id| has_placement(&scene, *unit_id))
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
    assert!(!has_placement(&scene, carrier_unit_id));
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

fn load_active_visuals(
    content: &mut pipeline::hw1::World,
    source: &mut AssetSource<StdFileProvider>,
    world: &sim::World,
) {
    let active = render::ugx::simulation_proto_names(world).collect::<Vec<_>>();
    content.load_visuals_for(source, active.iter().copied());
}

fn assert_visual_loaded(scene: &UnitScene, entity_id: EntityId, prototype: &str) {
    assert!(
        placement(scene, entity_id).is_some_and(|placement| placement.proto_name() == prototype),
        "{prototype} was not projected; issues: {:?}",
        scene
            .issues()
            .iter()
            .map(|issue| (issue.proto_name(), issue.reason()))
            .collect::<Vec<_>>()
    );
}

fn placement(scene: &UnitScene, entity_id: EntityId) -> Option<&render::ugx::UnitPlacement> {
    scene
        .placements()
        .iter()
        .find(|placement| placement.entity_id() == entity_id)
}

fn has_placement(scene: &UnitScene, entity_id: EntityId) -> bool {
    placement(scene, entity_id).is_some()
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

fn configure_cutter(world: &mut sim::World, database: &Database) {
    let leader_id = database
        .leaders
        .iter()
        .position(|leader| leader.name.eq_ignore_ascii_case("Cutter"))
        .and_then(|index| i32::try_from(index).ok())
        .expect("shipped Cutter leader");
    assert!(configure_player_leader(world, database, 1, leader_id));
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

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
