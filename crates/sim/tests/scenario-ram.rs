use glam::Vec3;
use sim::{
    CommandEntry, CommandExecutor, QueuedCommand, RecoveryType, SquadMode, WorkCommand,
    load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id,
};

const WARTHOG: &str = "unsc_veh_warthog_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-ram -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_scenario_database_drives_authoritative_warthog_ram() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point to a Halo Wars DE installation");
    let scenario =
        std::env::var("OPENENSEMBLE_TEST_SCENARIO").unwrap_or_else(|_| "blood_gulch".to_owned());
    let mut loaded =
        load_scenario_from_game_dir(&game_dir, &scenario).expect("real scenario should load");
    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;

    let profile = gameplay
        .collision_attack(WARTHOG)
        .expect("shipped Warthog tactics should expose CollisionAttack");
    assert_eq!(profile.action_name, "PersistentCollisionAttack");
    assert_eq!(profile.weapon_name, "Ram");
    assert_eq!(profile.weapon_type, Some("WarthogRam"));
    assert_close(profile.area_radius, 40.0);
    assert_close(profile.max_damage_per_ram, 10_000.0);

    let command_ability_id = database_ability_id(database, "Command");
    let ram_ability = gameplay
        .resolve_order_ability(WARTHOG, command_ability_id)
        .expect("Command should resolve through Warthog AbilityCommand");
    assert_eq!(ram_ability.name(), "UnscRam");
    assert_eq!(ram_ability.squad_mode(), Some(SquadMode::HitAndRun));
    assert_eq!(ram_ability.recovery_type(), Some(RecoveryType::Ability));
    assert_close(ram_ability.recovery_time(), 10.0);

    loaded.simulation.world.get_player_mut(1).unwrap().team_id = 1;
    loaded.simulation.world.get_player_mut(2).unwrap().team_id = 2;
    loaded.simulation.world.configure_standard_team_relations();
    let warthog_proto = squad_prototype_id(database, WARTHOG).expect("shipped Warthog squad");
    let attacker_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        1,
        warthog_proto,
        Vec3::ZERO,
        Vec3::X,
    )
    .expect("attacking Warthog");
    let target_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        2,
        warthog_proto,
        Vec3::X,
        Vec3::NEG_X,
    )
    .expect("target Warthog");
    let attacker_id = loaded
        .simulation
        .world
        .get_squad(attacker_squad)
        .unwrap()
        .unit_ids[0];
    let target_id = loaded
        .simulation
        .world
        .get_squad(target_squad)
        .unwrap()
        .unit_ids[0];
    let initial_target_hitpoints = loaded
        .simulation
        .world
        .get_unit(target_id)
        .unwrap()
        .hitpoints;

    let mut command = WorkCommand::attack_squads(1, vec![attacker_squad], target_squad);
    command.ability_id = i32::from(command_ability_id);
    CommandExecutor::with_database(database).execute(
        &mut loaded.simulation.world,
        &CommandEntry {
            command: QueuedCommand::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(attacker_squad)
            .unwrap()
            .mode,
        SquadMode::HitAndRun
    );

    loaded
        .simulation
        .world
        .update_entities_with_gameplay(0.05, gameplay);

    let attacker = loaded.simulation.world.get_unit(attacker_id).unwrap();
    let target = loaded.simulation.world.get_unit(target_id).unwrap();
    assert!(target.hitpoints < initial_target_hitpoints);
    assert!(attacker.ammunition.current() < attacker.ammunition.maximum());
    let squad = loaded.simulation.world.get_squad(attacker_squad).unwrap();
    assert_eq!(squad.mode, SquadMode::Normal);
    assert_eq!(squad.attack_target, None);
    assert_eq!(squad.recovery.recovery_type(), Some(RecoveryType::Ability));
    assert_eq!(squad.recovery.ability_id(), Some(ram_ability.database_id()));
}

fn database_ability_id(database: &pipeline::database::hw1::Database, name: &str) -> u8 {
    database
        .abilities
        .iter()
        .position(|ability| ability.name.eq_ignore_ascii_case(name))
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or_else(|| panic!("real database should contain ability {name}"))
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
}
