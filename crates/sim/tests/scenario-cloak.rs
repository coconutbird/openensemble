use glam::Vec3;
use sim::{
    RecoveryType, UnitDataScalar, load_scenario_from_game_dir, spawn_squad_at, squad_prototype_id,
};

const ARBITER: &str = "cov_inf_arbiter_01";
const ARBITER_CLOAK_TECH: &str = "cov_arbiter_upgrade3";
const BRUTE_CHIEF: &str = "cov_inf_bruteChief_01";
const ELITE_COMMANDO: &str = "cov_inf_elitecommando_01";

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-cloak -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_cloak_profiles_drive_authoritative_visibility_and_lifecycle() {
    let mut loaded = load_installed_scenario();
    assert_shipped_profiles(&loaded.simulation.gameplay);

    let database = &loaded.content.database;
    let gameplay = &loaded.simulation.gameplay;
    let world = &mut loaded.simulation.world;
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    let attacker = spawn(database, world, 1, BRUTE_CHIEF, Vec3::ZERO);
    let commando = spawn(database, world, 2, ELITE_COMMANDO, Vec3::X * 5.0);
    let commando_unit = world.get_squad(commando).unwrap().unit_ids[0];
    assert!(world.issue_cloak_order(2, commando, gameplay.command_ability_id()));

    advance(world, database, gameplay, 1.0);

    assert!(world.get_squad(commando).unwrap().is_cloaked());
    assert_close(
        world
            .get_unit(commando_unit)
            .unwrap()
            .data_scalar(UnitDataScalar::DamageTaken),
        0.5,
    );
    world.set_fog_of_war_enabled(false);
    assert!(!world.is_entity_visible_to_team(1, commando_unit));
    assert!(!world.issue_attack_order(1, attacker, commando, 20.0));
    assert!(world.detect_cloaked_squad(commando));
    assert!(world.is_entity_visible_to_team(1, commando_unit));
    assert!(world.issue_attack_order(1, attacker, commando, 20.0));
    world.get_squad_mut(attacker).unwrap().clear_attack_order();

    advance(world, database, gameplay, 30.0);

    let commando_state = world.get_squad(commando).unwrap();
    assert!(!commando_state.is_cloaked());
    assert_eq!(
        commando_state.recovery.recovery_type(),
        Some(RecoveryType::Ability)
    );
    assert!(commando_state.recovery.remaining() > 50.0);
    assert_close(
        world
            .get_unit(commando_unit)
            .unwrap()
            .data_scalar(UnitDataScalar::DamageTaken),
        1.0,
    );

    let arbiter = spawn(database, world, 2, ARBITER, Vec3::X * 30.0);
    assert!(!world.get_squad(arbiter).unwrap().is_cloaked());
    assert_eq!(
        world.activate_technology(2, database, ARBITER_CLOAK_TECH),
        Ok(true)
    );
    advance(world, database, gameplay, 0.05);
    assert!(world.get_squad(arbiter).unwrap().is_permanently_cloaked());
}

fn assert_shipped_profiles(gameplay: &sim::GameplayCatalog) {
    let arbiter = gameplay.cloak(ARBITER).expect("Arbiter Cloak profile");
    assert_eq!(arbiter.action_name(), "Cloak");
    assert_eq!(arbiter.effect_proto_object(), Some("fx_cloakElite"));
    assert!(arbiter.starts_disabled());
    assert!(arbiter.permanent());
    assert!(arbiter.auto_cloak());
    assert!(arbiter.move_while_cloaked());
    assert_close(arbiter.cloaking_delay(), 1.0);
    assert_close(arbiter.recloak_delay(), 5.0);

    let commando = gameplay
        .cloak(ELITE_COMMANDO)
        .expect("Elite Commando Cloak profile");
    assert_eq!(commando.action_name(), "Cloak");
    assert!(!commando.starts_disabled());
    assert!(!commando.permanent());
    assert!(commando.move_while_cloaked());
    assert!(commando.attack_while_cloaked());
    let ability = gameplay
        .resolve_order_ability(
            ELITE_COMMANDO,
            gameplay.command_ability_id().expect("Command ability"),
        )
        .expect("Elite Commando CovCloak ability");
    assert_eq!(ability.name(), "CovCloak");
    assert_close(ability.duration(), 30.0);
    assert_close(ability.recovery_time(), 60.0);
    assert_close(ability.damage_taken_modifier(), 0.5);
}

fn spawn(
    database: &pipeline::database::hw1::Database,
    world: &mut sim::World,
    player_id: u8,
    name: &str,
    position: Vec3,
) -> sim::EntityId {
    spawn_squad_at(
        world,
        database,
        player_id,
        squad_prototype_id(database, name).expect("shipped squad prototype"),
        position,
        Vec3::X,
    )
    .expect("shipped squad should spawn")
}

fn advance(
    world: &mut sim::World,
    database: &pipeline::database::hw1::Database,
    gameplay: &sim::GameplayCatalog,
    seconds: f32,
) {
    world.update_entities_with_database_and_gameplay(seconds, database, gameplay);
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
        "{actual} != {expected}"
    );
}
