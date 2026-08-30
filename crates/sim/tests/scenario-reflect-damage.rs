use glam::Vec3;
use sim::{load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id};

const ARBITER: &str = "cov_inf_arbiter_01";
const BRUTE_CHIEF: &str = "cov_inf_bruteChief_01";
const FIENDISH_RETURN: &str = "FiendishReturn";
const FIENDISH_RETURN_TECH: &str = "cov_arbiter_upgrade1";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-reflect-damage -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn arbiter_reflects_real_hand_attack_from_scenario_layered_data() {
    let mut loaded = load_installed_scenario();
    let profile = loaded
        .simulation
        .gameplay
        .reflect_damage(ARBITER)
        .expect("scenario-layered Arbiter ReflectDamage profile");
    assert_eq!(profile.action_name(), FIENDISH_RETURN);
    assert_close(profile.work_rate(), 0.15);
    assert!(profile.starts_disabled());

    let database = &loaded.content.database;
    let world = &mut loaded.simulation.world;
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let attacker_squad = spawn_squad_at(
        world,
        database,
        1,
        squad_prototype_id(database, BRUTE_CHIEF).expect("shipped Brute Chief squad"),
        Vec3::ZERO,
        Vec3::X,
    )
    .expect("Brute Chief squad should spawn");
    let arbiter_squad = spawn_squad_at(
        world,
        database,
        2,
        squad_prototype_id(database, ARBITER).expect("shipped Arbiter squad"),
        Vec3::X * 2.0,
        Vec3::NEG_X,
    )
    .expect("Arbiter squad should spawn");
    assert_eq!(
        world.activate_technology(2, database, FIENDISH_RETURN_TECH),
        Ok(true)
    );
    let attacker_hitpoints = squad_hitpoints(world, attacker_squad);
    let arbiter_hitpoints = squad_hitpoints(world, arbiter_squad);
    assert!(world.issue_attack_order(1, attacker_squad, arbiter_squad, 20.0));

    for _ in 0..150 {
        world.game_time_ms = world.game_time_ms.wrapping_add(50);
        world.update_entities_with_database_and_gameplay(
            0.05,
            database,
            &loaded.simulation.gameplay,
        );
        if squad_hitpoints(world, arbiter_squad) < arbiter_hitpoints {
            break;
        }
    }

    let attacker_state = world.get_squad(attacker_squad).map(|squad| {
        (
            squad.state,
            squad.base.position,
            squad.attack_target,
            squad.unit_ids.clone(),
        )
    });
    let arbiter_state = world.get_squad(arbiter_squad).map(|squad| {
        (
            squad.state,
            squad.base.position,
            squad.attack_target,
            squad.unit_ids.clone(),
        )
    });
    assert!(
        squad_hitpoints(world, arbiter_squad) < arbiter_hitpoints,
        "attacker state {attacker_state:?}; Arbiter state {arbiter_state:?}; projectiles {}",
        world.projectiles.len()
    );
    assert!(
        squad_hitpoints(world, attacker_squad) < attacker_hitpoints,
        "the enabled FiendishReturn action should damage the attacking Brute Chief"
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

fn load_installed_scenario() -> sim::LoadedGameScenario {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\HaloWarsDE".to_owned());
    load_scenario_from_game_dir(&game_dir, "blood_gulch")
        .expect("installed Blood Gulch scenario and layered database should load")
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.000_1);
}
