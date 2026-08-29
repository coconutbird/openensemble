use glam::Vec3;
use sim::{
    AttackQuery, AttackQueryFlags, CommandEntry, CommandExecutor, QueuedCommand, SquadMode,
    WorkCommand, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id,
};

const BRUTE_CHIEF: &str = "cov_inf_bruteChief_01";
const HAMMER_ATTACK: &str = "HammerAttackAction";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-hand-attacks -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_brute_chief_hand_attack_executes_without_a_projectile() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    let mut loaded = load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed scenario should load");
    assert_shipped_hand_attack_profile(&loaded);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    loaded.simulation.world.get_player_mut(1).unwrap().team_id = 1;
    loaded.simulation.world.get_player_mut(2).unwrap().team_id = 2;
    loaded.simulation.world.configure_standard_team_relations();
    let attacker_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        1,
        squad_prototype_id(database, BRUTE_CHIEF).expect("shipped Brute Chief squad"),
        Vec3::ZERO,
        Vec3::X,
    )
    .expect("Brute Chief should spawn");
    let target_squad = spawn_squad_at(
        &mut loaded.simulation.world,
        database,
        2,
        squad_prototype_id(database, BRUTE_CHIEF).expect("shipped Brute Chief squad"),
        Vec3::X * 2.0,
        Vec3::NEG_X,
    )
    .expect("Brute Chief target should spawn");
    let attacker_id = loaded
        .simulation
        .world
        .get_squad(attacker_squad)
        .unwrap()
        .unit_ids[0];
    let initial_target_hitpoints = squad_hitpoints(&loaded.simulation.world, target_squad);

    loaded
        .simulation
        .world
        .get_squad_mut(target_squad)
        .unwrap()
        .mode = SquadMode::Cover;
    CommandExecutor::with_database(database).execute(
        &mut loaded.simulation.world,
        &CommandEntry {
            command: QueuedCommand::Work(WorkCommand::attack_squads(
                1,
                vec![attacker_squad],
                target_squad,
            )),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
    for _ in 0..20 {
        loaded
            .simulation
            .world
            .update_entities_with_gameplay(0.05, gameplay);
    }
    assert_close(
        squad_hitpoints(&loaded.simulation.world, target_squad),
        initial_target_hitpoints,
    );

    loaded
        .simulation
        .world
        .get_squad_mut(target_squad)
        .unwrap()
        .mode = SquadMode::Normal;
    for _ in 0..100 {
        loaded
            .simulation
            .world
            .update_entities_with_gameplay(0.05, gameplay);
        assert!(loaded.simulation.world.projectiles.is_empty());
        if squad_hitpoints(&loaded.simulation.world, target_squad) < initial_target_hitpoints {
            break;
        }
    }

    assert_eq!(
        loaded
            .simulation
            .world
            .get_unit(attacker_id)
            .unwrap()
            .combat
            .action_name(),
        Some(HAMMER_ATTACK)
    );
    assert!(squad_hitpoints(&loaded.simulation.world, target_squad) < initial_target_hitpoints);
}

fn assert_shipped_hand_attack_profile(loaded: &sim::LoadedGameScenario) {
    let gameplay = &loaded.simulation.gameplay;
    let chief = gameplay
        .object(BRUTE_CHIEF)
        .expect("shipped Brute Chief tactics should load");
    let action = chief
        .tactics()
        .actions
        .iter()
        .find(|action| action.name.eq_ignore_ascii_case(HAMMER_ATTACK))
        .expect("Brute Chief hammer action");
    assert_eq!(action.action_type.as_deref(), Some("HandAttack"));
    let profile = chief
        .attack_profile(HAMMER_ATTACK)
        .expect("HandAttack should share retail attack timing");
    assert_eq!(profile.projectile, None);
    assert_close(profile.max_range, 3.0);
    assert!(!profile.animations.is_empty());
    assert!(
        profile
            .animations
            .iter()
            .any(|animation| !animation.attack_positions.is_empty())
    );

    let query = AttackQuery {
        target_proto_object_name: Some(BRUTE_CHIEF),
        ..AttackQuery::default()
    };
    assert_eq!(
        gameplay
            .select_attack_profile(BRUTE_CHIEF, &query, |action| {
                action.start_disabled != Some(true)
            })
            .map(|profile| profile.action_name.as_str()),
        Some(HAMMER_ATTACK)
    );
    let mut covered = query;
    covered.flags.insert(AttackQueryFlags::TARGET_IN_COVER);
    assert!(
        gameplay
            .select_attack_profile(BRUTE_CHIEF, &covered, |action| {
                action.start_disabled != Some(true)
            })
            .is_none()
    );
}

fn squad_hitpoints(world: &sim::World, squad_id: sim::EntityId) -> f32 {
    world.get_squad(squad_id).map_or(0.0, |squad| {
        squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| world.get_unit(*unit_id))
            .map(|unit| unit.hitpoints)
            .sum()
    })
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
}
