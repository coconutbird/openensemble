use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    DisruptionPowerInvocation, EntityId, MS_PER_TICK, NativePowerError, RepairPowerInvocation,
    load_scenario_from_game_dir, power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const REPAIR_POWER: &str = "UnscLeaderRepair";
const DISRUPTION_POWER: &str = "UnscLeaderDisruption";
const MARINES: &str = "unsc_inf_marine_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-repair-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_repair_reinforces_from_the_layered_database_and_obeys_disruption() {
    let loaded = load_installed_scenario();
    let sim::LoadedGameScenario {
        mut simulation,
        content,
        ..
    } = loaded;
    let database = &content.database;
    let repair_id = power_prototype_id(database, REPAIR_POWER).expect("shipped Repair power");
    let disruption_id =
        power_prototype_id(database, DISRUPTION_POWER).expect("shipped Disruption power");
    let marine_id = squad_prototype_id(database, MARINES).expect("shipped Marine squad");
    let squad_id = spawn_incomplete_marines(&mut simulation, database, marine_id);

    simulation
        .world
        .invoke_repair_power(database, repair_invocation(repair_id))
        .expect("shipped Repair profile should fully resolve");
    let (field_id, first_tick) = assert_shipped_repair_profile(&simulation);

    advance_to(&mut simulation, database, first_tick);
    assert_reinforced_and_attached(&simulation, squad_id);
    stop_repair_with_disruption(
        &mut simulation,
        database,
        repair_id,
        disruption_id,
        field_id,
    );
}

fn spawn_incomplete_marines(
    simulation: &mut sim::LoadedScenario,
    database: &Database,
    marine_id: i32,
) -> EntityId {
    let squad_id = spawn_squad_at(
        &mut simulation.world,
        database,
        1,
        marine_id,
        Vec3::ZERO,
        Vec3::Z,
    )
    .expect("spawn shipped Marines");
    let removed_member = simulation.world.get_squad(squad_id).unwrap().unit_ids[3];
    simulation.world.remove_unit(removed_member).unwrap();
    squad_id
}

fn assert_shipped_repair_profile(simulation: &sim::LoadedScenario) -> (EntityId, u32) {
    let execution = &simulation.world.active_repair_powers()[0];
    assert_eq!(execution.radius().to_bits(), 35.0_f32.to_bits());
    assert_eq!(execution.tick_duration_ms(), 125);
    assert_eq!(execution.ticks_remaining(), 200);
    assert_eq!(
        execution.repair_combat_value_per_tick().to_bits(),
        0.1_f32.to_bits()
    );
    assert_eq!(execution.cooldown_time_if_damaged_ms(), 5_000);
    assert!(execution.spreads_among_squads());
    assert!(execution.allows_reinforcement());
    assert!(!execution.ignores_placement());
    assert!(!execution.heals_any_relation());
    assert!(!execution.never_stops());
    assert_eq!(execution.repair_object_prototype(), "fx_repairPower");
    assert_eq!(
        execution.repair_attachment_prototype(),
        Some("fx_repairing")
    );
    let field_id = execution.repair_object_id();
    assert_eq!(
        simulation
            .world
            .get_object(field_id)
            .unwrap()
            .proto_object_name,
        "fx_repairPower"
    );
    (field_id, execution.next_tick_time_ms())
}

fn assert_reinforced_and_attached(simulation: &sim::LoadedScenario, squad_id: EntityId) {
    let squad = simulation.world.get_squad(squad_id).unwrap();
    assert_eq!(squad.unit_ids.len(), 4, "missing Marine was not reinforced");
    assert_eq!(squad.repair_regen_source_count(), 1);
    let partial_member = squad
        .unit_ids
        .iter()
        .filter_map(|unit_id| simulation.world.get_unit(*unit_id))
        .find(|unit| unit.hitpoints < unit.max_hitpoints)
        .expect("new Marine should begin at partial health");
    assert!(partial_member.hitpoints > 0.0);
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
        .expect("sim-owned shipped repair attachment");
    assert_eq!(
        simulation
            .world
            .entity_object_state(attachment_id)
            .unwrap()
            .attached_to(),
        squad.unit_ids.first().copied()
    );
}

fn stop_repair_with_disruption(
    simulation: &mut sim::LoadedScenario,
    database: &Database,
    repair_id: i32,
    disruption_id: i32,
    field_id: EntityId,
) {
    simulation
        .world
        .invoke_disruption_power(
            database,
            DisruptionPowerInvocation {
                player_id: 1,
                proto_power_id: disruption_id,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Disruption profile");
    advance_until_disruption_active(simulation, database);
    for _ in 0..4 {
        if simulation.world.active_repair_powers().is_empty() {
            break;
        }
        advance_one_tick(simulation, database);
    }
    assert!(simulation.world.active_repair_powers().is_empty());
    assert!(simulation.world.get_object(field_id).is_none());

    let active_disruption_id = simulation.world.active_disruption_powers()[0].id();
    assert_eq!(
        simulation
            .world
            .invoke_repair_power(database, repair_invocation(repair_id)),
        Err(NativePowerError::Disrupted(active_disruption_id)),
        "NO_COST must not bypass disruption"
    );
}

fn repair_invocation(proto_power_id: i32) -> RepairPowerInvocation {
    RepairPowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location: Vec3::ZERO,
        ignore_requirements: true,
    }
}

fn advance_to(simulation: &mut sim::LoadedScenario, database: &Database, target_time: u32) {
    while simulation.world.game_time_ms < target_time {
        advance_one_tick(simulation, database);
    }
}

fn advance_until_disruption_active(simulation: &mut sim::LoadedScenario, database: &Database) {
    for _ in 0..80 {
        advance_one_tick(simulation, database);
        if simulation
            .world
            .active_disruption_powers()
            .first()
            .is_some_and(sim::DisruptionPowerExecution::is_active)
        {
            return;
        }
    }
    panic!("shipped Disruption never became active");
}

fn advance_one_tick(simulation: &mut sim::LoadedScenario, database: &Database) {
    simulation.world.game_time_ms = simulation.world.game_time_ms.wrapping_add(MS_PER_TICK);
    simulation.world.update_entities_with_database_and_gameplay(
        0.05,
        database,
        &simulation.gameplay,
    );
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
