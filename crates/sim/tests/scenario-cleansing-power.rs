use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    CleansingPowerInvocation, EntityId, NativePowerInput, PowerUserId, load_scenario_from_game_dir,
    power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const CLEANSING_POWER: &str = "CovLeaderGlassing";
const PROPHET: &str = "cov_inf_prophet_01";
const MARINES: &str = "unsc_inf_marine_01";
const BEAM: &str = "pow_gp_cleansing_01";
const PROJECTILE: &str = "pow_proj_cleansing_01";
const AIR_IMPACT: &str = "fx_cleansing_air_impact_small_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-cleansing-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_cleansing_profile_uses_scenario_layered_database_and_tactics() {
    let mut loaded = load_installed_scenario();
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    let owner_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        1,
        PROPHET,
        -Vec3::X * 20.0,
    );
    let target_id = spawn_named_squad(
        &mut loaded.simulation.world,
        &loaded.content.database,
        2,
        MARINES,
        Vec3::ZERO,
    );
    let target_position = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .base
        .position;
    let target_units = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .unit_ids
        .clone();
    let health_before = total_health(&loaded.simulation.world, &target_units);
    let power_id =
        power_prototype_id(&loaded.content.database, CLEANSING_POWER).expect("shipped power");
    let execution_id = loaded
        .simulation
        .world
        .invoke_cleansing_power(
            &loaded.content.database,
            invocation(power_id, owner_id, target_position),
        )
        .expect("shipped Cleansing profile should fully resolve");
    assert_shipped_profile(&loaded, execution_id);

    let beam_id = loaded.simulation.world.active_cleansing_powers()[0].beam_object_id();
    assert_proto(&loaded.simulation.world, beam_id, BEAM);
    assert_eq!(
        loaded.simulation.world.get_squad(owner_id).unwrap().mode,
        sim::SquadMode::Power
    );
    assert!(loaded.simulation.world.submit_cleansing_power_input(
        &loaded.content.database,
        execution_id,
        NativePowerInput::Position(target_position + Vec3::X * 8.0),
    ));

    advance(&mut loaded, 1);
    let execution = &loaded.simulation.world.active_cleansing_powers()[0];
    assert_close(
        loaded
            .simulation
            .world
            .get_object(beam_id)
            .unwrap()
            .base
            .position
            .x,
        target_position.x + 0.8,
    );
    assert_eq!(execution.active_projectile_ids().len(), 1);
    let projectile = loaded
        .simulation
        .world
        .get_projectile(execution.active_projectile_ids()[0])
        .unwrap();
    assert_eq!(projectile.proto_object_name, PROJECTILE);
    assert_close(projectile.damage, 330.0);

    advance(&mut loaded, 1);
    assert!(total_health(&loaded.simulation.world, &target_units) < health_before);
    assert!(loaded.simulation.world.submit_cleansing_power_input(
        &loaded.content.database,
        execution_id,
        NativePowerInput::Shutdown,
    ));
    assert!(loaded.simulation.world.active_cleansing_powers().is_empty());
    assert!(loaded.simulation.world.get_object(beam_id).is_none());
    assert_eq!(
        loaded.simulation.world.get_squad(owner_id).unwrap().mode,
        sim::SquadMode::Normal
    );
}

fn assert_shipped_profile(loaded: &sim::LoadedGameScenario, execution_id: sim::PowerExecutionId) {
    let execution = loaded
        .simulation
        .world
        .active_cleansing_powers()
        .iter()
        .find(|execution| execution.id() == execution_id)
        .unwrap();
    assert_eq!(execution.beam_prototype(), BEAM);
    assert_eq!(execution.projectile_prototype(), PROJECTILE);
    assert_eq!(execution.air_impact_prototype(), Some(AIR_IMPACT));
    assert_close(execution.tick_length(), 0.2);
    assert_close(execution.supplies_per_tick(), 10.0);
    assert_close(execution.minimum_beam_distance(), 0.0);
    assert_close(execution.maximum_beam_distance(), 70.0);
    assert_eq!(execution.command_interval_ms(), 200);
    assert_close(execution.maximum_beam_speed(), 8.0);

    let tactics = loaded
        .simulation
        .gameplay
        .object(PROJECTILE)
        .expect("scenario-layered Cleansing tactics")
        .tactics();
    let weapon = tactics
        .weapons
        .iter()
        .find(|weapon| weapon.name.eq_ignore_ascii_case("Cleansing"))
        .expect("Cleansing tactic weapon");
    assert_eq!(weapon.damage_per_second, Some(330.0));
    assert_eq!(weapon.aoe_radius, Some(8.0));
}

fn advance(loaded: &mut sim::LoadedGameScenario, ticks: usize) {
    for _ in 0..ticks {
        loaded.simulation.world.game_time_ms =
            loaded.simulation.world.game_time_ms.wrapping_add(100);
        loaded
            .simulation
            .world
            .update_entities_with_database_and_gameplay(
                0.1,
                &loaded.content.database,
                &loaded.simulation.gameplay,
            );
    }
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

fn total_health(world: &sim::World, unit_ids: &[EntityId]) -> f32 {
    unit_ids
        .iter()
        .filter_map(|unit_id| world.get_unit(*unit_id))
        .map(|unit| unit.hitpoints + unit.shields.current)
        .sum()
}

fn assert_proto(world: &sim::World, id: EntityId, name: &str) {
    assert_eq!(
        world
            .get_object(id)
            .map(|object| object.proto_object_name.as_str()),
        Some(name)
    );
}

fn invocation(
    proto_power_id: i32,
    squad_id: EntityId,
    target_location: Vec3,
) -> CleansingPowerInvocation {
    CleansingPowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id,
        target_location,
        ignore_requirements: true,
        power_user_id: PowerUserId::INVALID,
    }
}

fn configure_enemies(world: &mut sim::World) {
    world.get_player_mut(1).expect("player one").team_id = 1;
    world.get_player_mut(2).expect("player two").team_id = 2;
    world.configure_standard_team_relations();
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

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "expected {expected}, got {actual}"
    );
}
