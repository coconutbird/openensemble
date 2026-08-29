use glam::Vec3;
use sim::{
    CryoPowerInvocation, EntityId, load_scenario_from_game_dir, power_prototype_id, spawn_squad_at,
    squad_prototype_id,
};

const CRYO_POWER: &str = "UnscLeaderCryo";
const FROZEN_KILL_TARGET: &str = "unsc_air_hornet_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-cryo-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_cryo_profile_executes_against_the_scenario_layered_database() {
    let mut loaded = load_installed_scenario();
    let database = &loaded.content.database;
    let power_id = power_prototype_id(database, CRYO_POWER).expect("shipped Cryo power");
    let target_prototype =
        squad_prototype_id(database, FROZEN_KILL_TARGET).expect("shipped frozen-kill target");
    let target_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        1,
        target_prototype,
        Vec3::ZERO,
        Vec3::X,
    )
    .expect("spawn shipped target squad");

    loaded
        .simulation
        .world
        .invoke_cryo_power(
            database,
            CryoPowerInvocation {
                player_id: 1,
                proto_power_id: power_id,
                power_level: 0,
                squad_id: EntityId::INVALID,
                target_location: Vec3::ZERO,
                ignore_requirements: true,
            },
        )
        .expect("shipped Cryo profile should fully resolve");
    let execution = &loaded.simulation.world.active_cryo_powers()[0];
    assert_eq!(execution.radius().to_bits(), 45.0_f32.to_bits());
    assert_eq!(execution.minimum_falloff().to_bits(), 0.25_f32.to_bits());
    assert_eq!(
        execution.cryo_amount_per_tick().to_bits(),
        40.0_f32.to_bits()
    );
    assert_eq!(execution.ticks_remaining(), 16);
    assert!(execution.bomber_visible());
    assert!((execution.direction().length() - 1.0).abs() < 1.0e-6);
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(execution.bomber_object_id())
            .unwrap()
            .proto_object_name,
        execution.bomber_prototype()
    );
    let first_tick = execution.next_tick_time_ms();

    while loaded.simulation.world.game_time_ms < first_tick {
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
                database,
                &loaded.simulation.gameplay,
            );
    }

    assert!(
        loaded.simulation.world.get_squad(target_squad).is_none(),
        "target remained with state {:?} and member types {:?}",
        loaded
            .simulation
            .world
            .get_squad(target_squad)
            .map(sim::Squad::cryo_state),
        loaded
            .simulation
            .world
            .get_squad(target_squad)
            .and_then(|squad| squad.unit_ids.first())
            .and_then(|unit_id| loaded.simulation.world.get_unit(*unit_id))
            .map(|unit| unit.object_types.as_slice()),
    );
    assert_eq!(
        loaded.simulation.world.active_cryo_powers()[0].ticks_remaining(),
        15
    );
    let execution = &loaded.simulation.world.active_cryo_powers()[0];
    assert_eq!(
        loaded
            .simulation
            .world
            .get_object(execution.cryo_object_id())
            .unwrap()
            .proto_object_name,
        execution.cryo_object_prototype()
    );
}

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}
