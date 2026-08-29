use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    EntityId, MS_PER_TICK, NativePowerInput, RagePowerInvocation, RagePowerPhase, UnitDataScalar,
    load_scenario_from_game_dir, power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const RAGE_POWER: &str = "CovLeaderRage";
const ARBITER: &str = "cov_inf_arbiter_01";
const MARINES: &str = "unsc_inf_marine_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-rage-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_rage_profile_controls_squad_and_executes_a_landing_attack() {
    let mut loaded = load_installed_scenario();
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_hostile_players(&mut loaded.simulation.world);
    let database = &loaded.content.database;
    let rage_id = power_prototype_id(database, RAGE_POWER).expect("shipped Rage power");
    assert_shipped_impact_tactic(&loaded.simulation.gameplay);
    let owner_id = spawn_named_squad(
        &mut loaded.simulation.world,
        database,
        1,
        ARBITER,
        Vec3::ZERO,
    );
    let target_id = spawn_named_squad(
        &mut loaded.simulation.world,
        database,
        2,
        MARINES,
        Vec3::X * 20.0,
    );
    let target_unit_id = loaded
        .simulation
        .world
        .get_squad(target_id)
        .unwrap()
        .unit_ids[0];
    let target_hitpoints = loaded
        .simulation
        .world
        .get_unit(target_unit_id)
        .unwrap()
        .hitpoints;

    let execution_id = loaded
        .simulation
        .world
        .invoke_rage_power(
            database,
            RagePowerInvocation {
                player_id: 1,
                proto_power_id: rage_id,
                power_level: 0,
                squad_id: owner_id,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Rage profile should fully resolve");
    assert_shipped_profile(&loaded.simulation, owner_id);
    assert!(loaded.simulation.world.submit_rage_power_input(
        database,
        execution_id,
        NativePowerInput::Direction(Vec3::X),
    ));
    assert_eq!(
        loaded.simulation.world.active_rage_powers()[0].target_squad_id(),
        Some(target_id)
    );
    assert_eq!(
        loaded.simulation.world.active_rage_powers()[0].phase(),
        RagePowerPhase::Jumping
    );

    advance_through_landing(&mut loaded);
    let owner_position = loaded
        .simulation
        .world
        .get_squad(owner_id)
        .unwrap()
        .base
        .position;
    assert!(owner_position.x > 10.0);
    assert!(
        loaded
            .simulation
            .world
            .get_unit(target_unit_id)
            .is_none_or(|unit| unit.hitpoints < target_hitpoints)
    );
    assert_eq!(loaded.simulation.world.active_rage_powers().len(), 1);
}

fn assert_shipped_impact_tactic(gameplay: &sim::GameplayCatalog) {
    let tactic = gameplay
        .object("pow_gp_rage_impact")
        .expect("scenario-layered Rage impact tactic should parse")
        .tactics();
    let weapon = tactic
        .weapons
        .iter()
        .find(|weapon| weapon.name.eq_ignore_ascii_case("Rage"))
        .expect("shipped Rage impact weapon");
    assert_eq!(weapon.damage_per_second, Some(1_655.0));
    assert_eq!(weapon.aoe_radius, Some(10.0));
}

fn assert_shipped_profile(simulation: &sim::LoadedScenario, owner_id: EntityId) {
    let execution = &simulation.world.active_rage_powers()[0];
    assert_close(execution.tick_length(), 0.2);
    assert_close(execution.supplies_per_tick(), 1.25);
    assert_close(execution.supplies_per_tick_attacking(), 5.0);
    assert_close(execution.supplies_per_jump(), 40.0);
    assert_close(execution.damage_taken_multiplier(), 1.25);
    assert_close(execution.speed_multiplier(), 2.0);
    assert_close(execution.nudge_multiplier(), 1.0);
    assert_close(execution.scan_radius(), 40.0);
    assert_close(execution.teleport_time(), 0.3);
    assert_eq!(execution.projectile_prototype(), "pow_gp_rage_impact");
    assert_eq!(
        execution.hand_attachment_prototype(),
        "fx_arbiterragehands_01"
    );
    assert_eq!(
        execution.teleport_attachment_prototype(),
        "fx_rage_teleport_01"
    );
    assert_eq!(execution.aura_filter_type(), "Military");
    assert_eq!(execution.hand_attachment_ids().len(), 2);
    let owner = simulation.world.get_squad(owner_id).unwrap();
    assert!(owner.is_raging() && owner.is_sprinting());
    let leader = simulation.world.get_unit(owner.unit_ids[0]).unwrap();
    assert_close(
        leader.data_scalar(UnitDataScalar::DamageTaken),
        execution.damage_taken_multiplier(),
    );
    assert_close(
        leader.data_scalar(UnitDataScalar::Velocity),
        execution.speed_multiplier(),
    );
}

fn advance_through_landing(loaded: &mut sim::LoadedGameScenario) {
    for _ in 0..10 {
        loaded.simulation.world.game_time_ms = loaded
            .simulation
            .world
            .game_time_ms
            .wrapping_add(MS_PER_TICK);
        loaded
            .simulation
            .world
            .update_entities_with_database_and_gameplay(
                0.05,
                &loaded.content.database,
                &loaded.simulation.gameplay,
            );
        if loaded.simulation.world.active_rage_powers()[0].phase() != RagePowerPhase::Jumping {
            return;
        }
    }
    panic!("shipped Rage jump never landed");
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

fn remove_scenario_squads(world: &mut sim::World) {
    let ids = world.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        world.remove_squad(id).unwrap();
    }
}

fn configure_hostile_players(world: &mut sim::World) {
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.000_1,
        "{actual} != {expected}"
    );
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
