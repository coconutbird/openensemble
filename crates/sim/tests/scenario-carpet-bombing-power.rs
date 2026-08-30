use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    CarpetBombingPhase, CarpetBombingPowerInvocation, EntityId, NativePowerInput, PowerUserId,
    load_scenario_from_game_dir, power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const CARPET_BOMBING_POWER: &str = "UnscLeaderCarpetBombing";
const MARINES: &str = "unsc_inf_marine_01";
const BOMBER: &str = "pow_gp_shortsword_01";
const IMPACT: &str = "pow_gp_carpetbomb_impact";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-carpet-bombing-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_carpet_bombing_profile_drops_eight_fused_bombs_and_applies_tactic_damage() {
    let mut loaded = load_installed_scenario();
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    let database = &loaded.content.database;
    assert_shipped_profile(database);
    let target_squad = spawn_named_squad(
        &mut loaded.simulation.world,
        database,
        2,
        MARINES,
        Vec3::ZERO,
    );
    let target_units = loaded
        .simulation
        .world
        .get_squad(target_squad)
        .unwrap()
        .unit_ids
        .clone();
    let health_before = total_health(&loaded.simulation.world, &target_units);
    let power_id =
        power_prototype_id(database, CARPET_BOMBING_POWER).expect("shipped Carpet Bombing power");
    let execution_id = loaded
        .simulation
        .world
        .invoke_carpet_bombing_power(database, invocation(power_id))
        .expect("shipped Carpet Bombing profile should resolve");
    assert!(loaded.simulation.world.submit_carpet_bombing_power_input(
        database,
        execution_id,
        NativePowerInput::Position(Vec3::ZERO),
    ));
    assert!(loaded.simulation.world.submit_carpet_bombing_power_input(
        database,
        execution_id,
        NativePowerInput::Direction(Vec3::X),
    ));
    let execution = &loaded.simulation.world.active_carpet_bombing_powers()[0];
    assert_eq!(execution.phase(), CarpetBombingPhase::Active);
    assert_eq!(execution.maximum_bomb_clusters(), 4);
    assert_eq!(execution.bomber_prototype(), Some(BOMBER));
    let bomber_id = execution.bomber_object_id().expect("sim-owned bomber");

    advance_until(&mut loaded, 40, |world| {
        world.active_carpet_bombing_powers().is_empty()
            || world.active_carpet_bombing_powers()[0].bomb_clusters_dropped() == 4
    });
    let execution = &loaded.simulation.world.active_carpet_bombing_powers()[0];
    assert_eq!(execution.pending_bombs().len(), 8);
    assert!(execution.pending_bombs().iter().all(|bomb| {
        loaded
            .simulation
            .world
            .get_object(bomb.impact_object_id())
            .is_some_and(|object| object.proto_object_name == IMPACT)
    }));

    advance_until(&mut loaded, 60, |world| {
        world.active_carpet_bombing_powers().is_empty()
    });
    let health_after = total_health(&loaded.simulation.world, &target_units);
    assert!(
        health_after < health_before,
        "shipped Carpet Bombing tactic damage was not applied"
    );
    assert!(loaded.simulation.world.get_object(bomber_id).is_none());
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
        "Carpet Bombing state did not converge"
    );
}

fn assert_shipped_profile(database: &Database) {
    let power_id = power_prototype_id(database, CARPET_BOMBING_POWER).unwrap();
    let power = &database.powers[usize::try_from(power_id).unwrap()];
    let attributes = power.attributes.as_ref().expect("power attributes");
    assert_eq!(attributes.power_type.as_deref(), Some("CarpetBombing"));
    let level = attributes
        .data_levels
        .iter()
        .find(|level| level.level == Some(0))
        .expect("level zero");
    assert!(
        level.entries.iter().any(|entry| {
            entry.name == "MaxBombs" && entry.value.trim().parse::<i32>() == Ok(4)
        })
    );
    assert!(
        database
            .objects
            .iter()
            .any(|object| object.name.eq_ignore_ascii_case(BOMBER))
    );
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

fn invocation(proto_power_id: i32) -> CarpetBombingPowerInvocation {
    CarpetBombingPowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
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
