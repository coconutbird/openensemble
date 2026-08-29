use glam::{Vec3, Vec4};
use sim::{
    Command, CommandEntry, CommandExecutor, EntityId, PowerCommand, PowerCommandType,
    QueuedCommand, load_scenario_from_game_dir, power_prototype_id,
};

const DISRUPTION_POWER: &str = "UnscLeaderDisruption";
const CRYO_POWER: &str = "UnscLeaderCryo";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-disruption-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_disruption_blocks_cryo_through_the_native_power_command_path() {
    let mut loaded = load_installed_scenario();
    let disruption_id = power_prototype_id(&loaded.content.database, DISRUPTION_POWER)
        .expect("shipped Disruption power");
    let cryo_id =
        power_prototype_id(&loaded.content.database, CRYO_POWER).expect("shipped Cryo power");
    execute_power(&mut loaded, disruption_id, Vec3::ZERO);

    let execution = &loaded.simulation.world.active_disruption_powers()[0];
    assert_eq!(execution.radius().to_bits(), 70.0_f32.to_bits());
    assert_eq!(
        execution.time_remaining_seconds().to_bits(),
        60.0_f32.to_bits()
    );
    assert_eq!(execution.start_time_seconds().to_bits(), 2.6_f32.to_bits());
    assert_eq!(execution.bomber_prototype(), "pow_gp_shortsword_01");
    assert_eq!(
        execution.disruption_object_prototype(),
        "fx_disruptionPower"
    );
    assert_eq!(execution.pulse_object_prototype(), "fx_disruptionRing");
    assert_eq!(
        execution.strike_object_prototype(),
        "fx_disruptionLightningBeam"
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(execution.bomber_object_id())
            .unwrap()
            .proto_object_name,
        "pow_gp_shortsword_01"
    );

    advance_until_active(&mut loaded);
    let execution = &loaded.simulation.world.active_disruption_powers()[0];
    let field_id = execution.disruption_object_id();
    assert!(execution.is_active());
    assert_eq!(execution.pulse_count(), 1);
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(field_id)
            .unwrap()
            .proto_object_name,
        "fx_disruptionPower"
    );
    let pulse = loaded
        .simulation
        .world
        .objects
        .iter()
        .find(|(_, object)| object.proto_object_name == "fx_disruptionRing")
        .expect("first authored disruption pulse");
    assert_eq!(pulse.1.object_state.attached_to(), Some(field_id));

    execute_power(&mut loaded, cryo_id, Vec3::ZERO);
    assert!(
        loaded.simulation.world.active_cryo_powers().is_empty(),
        "NO_COST must not bypass disruption"
    );
    execute_power(&mut loaded, cryo_id, Vec3::X * 70.0);
    assert_eq!(
        loaded.simulation.world.active_cryo_powers().len(),
        1,
        "retail's disruption radius comparison is strict"
    );
}

fn execute_power(loaded: &mut sim::LoadedGameScenario, proto_power_id: i32, target: Vec3) {
    let mut command = PowerCommand {
        base: Command {
            player_id: 1,
            ..Command::default()
        },
        power_type: PowerCommandType::InvokePower2,
        proto_power_id,
        power_level: 0,
        target_location: Vec4::new(target.x, target.y, target.z, 0.0),
        squad_id: EntityId::INVALID,
        ..PowerCommand::default()
    };
    command
        .base
        .set_flag(sim::commands::power_command_flags::NO_COST, true);
    CommandExecutor::with_database(&loaded.content.database).execute(
        &mut loaded.simulation.world,
        &CommandEntry {
            command: QueuedCommand::Power(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}

fn advance_until_active(loaded: &mut sim::LoadedGameScenario) {
    for _ in 0..60 {
        loaded.simulation.world.game_time_ms = loaded
            .simulation
            .world
            .game_time_ms
            .wrapping_add(sim::MS_PER_TICK);
        loaded
            .simulation
            .world
            .update_entities_with_database_and_gameplay(
                0.05,
                &loaded.content.database,
                &loaded.simulation.gameplay,
            );
        if loaded
            .simulation
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

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
