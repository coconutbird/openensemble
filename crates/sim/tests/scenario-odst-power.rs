use glam::Vec3;
use sim::{
    EntityId, NativePowerInput, OdstPowerInvocation, PowerUserId, load_scenario_from_game_dir,
    power_prototype_id,
};

const ODST_POWER: &str = "UnscOdstDrop";
const ODST_TECH: &str = "unsc_odst_upgrade1";
const ODST_SQUAD: &str = "unsc_inf_odst_01";
const ODST_POD: &str = "unsc_air_odstPod_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-odst-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_odst_drop_uses_layered_profiles_transformed_members_and_delayed_reveal() {
    let loaded = load_installed_scenario();
    let sim::LoadedGameScenario {
        mut simulation,
        content,
        ..
    } = loaded;
    remove_scenario_squads(&mut simulation.world);
    let database = &content.database;
    let power_id = power_prototype_id(database, ODST_POWER).expect("shipped ODST power");
    assert!(
        simulation
            .world
            .activate_technology(1, database, ODST_TECH)
            .unwrap()
    );
    let execution_id = simulation
        .world
        .invoke_odst_power(database, invocation(power_id))
        .expect("shipped ODST profile should fully resolve");
    let execution = &simulation.world.active_odst_powers()[0];
    assert_eq!(execution.squad_spawn_delay().to_bits(), 0.75_f32.to_bits());
    assert_eq!(execution.projectile_prototype(), ODST_POD);
    assert_eq!(execution.odst_squad_prototype(), ODST_SQUAD);

    assert!(simulation.world.submit_odst_power_input(
        database,
        execution_id,
        NativePowerInput::Confirm(Vec3::ZERO),
        false,
    ));
    assert!(simulation.world.submit_odst_power_input(
        database,
        execution_id,
        NativePowerInput::Shutdown,
        false,
    ));
    let squad_id = simulation.world.active_odst_powers()[0].active_drops()[0].squad_id();
    assert_hidden_shipped_squad(&simulation.world, squad_id);

    advance(&mut simulation, database, 0.05);
    let projectile_id = simulation.world.active_odst_powers()[0].active_drops()[0].projectile_id();
    let projectile = simulation
        .world
        .get_projectile(projectile_id)
        .expect("live ODST pod");
    assert_eq!(projectile.proto_object_name, ODST_POD);
    assert_eq!(
        projectile.target_position,
        simulation.world.get_squad(squad_id).unwrap().base.position
    );
    advance(&mut simulation, database, 0.70);
    assert_hidden_shipped_squad(&simulation.world, squad_id);

    advance(&mut simulation, database, 0.001);
    assert!(simulation.world.active_odst_powers().is_empty());
    assert_revealed_squad(&simulation.world, squad_id);
}

fn assert_hidden_shipped_squad(world: &sim::World, squad_id: EntityId) {
    let squad = world.get_squad(squad_id).expect("spawned ODST squad");
    assert_eq!(squad.proto_squad_name, ODST_SQUAD);
    assert_eq!(squad.unit_ids.len(), 6);
    assert_eq!(world.entity_is_selectable(squad_id), Some(false));
    assert!(squad.unit_ids.iter().all(|unit_id| {
        world.entity_is_render_enabled(*unit_id) == Some(false)
            && world
                .get_unit(*unit_id)
                .is_some_and(sim::Unit::is_invulnerable)
    }));
    assert!(squad.unit_ids.iter().any(|unit_id| {
        world
            .get_unit(*unit_id)
            .is_some_and(|unit| unit.proto_object_name.eq_ignore_ascii_case(ODST_SQUAD))
    }));
}

fn assert_revealed_squad(world: &sim::World, squad_id: EntityId) {
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(world.entity_is_selectable(squad_id), Some(true));
    assert!(squad.unit_ids.iter().all(|unit_id| {
        world.entity_is_render_enabled(*unit_id) == Some(true)
            && world
                .get_unit(*unit_id)
                .is_some_and(|unit| !unit.is_invulnerable())
    }));
}

fn advance(
    simulation: &mut sim::LoadedScenario,
    database: &pipeline::database::hw1::Database,
    dt: f32,
) {
    simulation
        .world
        .update_entities_with_database_and_gameplay(dt, database, &simulation.gameplay);
}

fn invocation(proto_power_id: i32) -> OdstPowerInvocation {
    OdstPowerInvocation {
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
