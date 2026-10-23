//! Checks the weapon effect families with shipped weapons on Blood Gulch: the damage over time
//! of the Flame Marine flamethrower, the daze of the Brute Chief stun hammer, and the stasis of
//! the Vampire beams with their smart-target references and their release.

use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    EntityId, LoadedGameScenario, SmartTargetType, World, spawn_squad_at, squad_prototype_id,
};

const MARINES: &str = "unsc_inf_marine_01";
/// `Flamethrower1`: `DOTrate` 6.8 for `DOTduration` 1 s.
const FLAME_MARINES: &str = "unsc_inf_flameMarine_01";
/// `stunHammer`: `Daze` 3 s at `TargetType` `unit`.
const BRUTE_CHIEF: &str = "cov_inf_bruteChief_01";
/// `StasisDrainBeam` and `StasisBombBeam`: `Stasis` with `SmartTargeting`.
const VAMPIRE: &str = "cov_air_vampire_01";
const VAMPIRE_UNIT: &str = "cov_air_vampire_01";
const TICK_MS: u32 = 50;
const TICK_SECONDS: f32 = 0.05;
/// The ticks that an attacker gets to apply its effect.
const EFFECT_TICKS: usize = 1_200;
/// The ticks after the attacker goes: longer than every effect duration of the test.
const END_TICKS: usize = 240;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-weapon-effects -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn flame_marine_flamethrower_adds_damage_over_time_that_ends() {
    let (mut loaded, center) = prepared_scenario();
    let marines = spawn_named_squad(&mut loaded, 2, MARINES, center);
    let flamers = spawn_named_squad(&mut loaded, 1, FLAME_MARINES, center - Vec3::X * 8.0);
    assert!(
        loaded
            .simulation
            .world
            .issue_attack_order(1, flamers, marines, 0.0)
    );

    let burning = (0..EFFECT_TICKS).any(|_| {
        tick(&mut loaded);
        squad_units(&loaded.simulation.world, marines)
            .iter()
            .any(|&unit_id| dot_stacks(&loaded.simulation.world, unit_id) > 0)
    });
    assert!(
        burning,
        "the flamethrower should add a damage over time stack"
    );

    loaded.simulation.world.remove_squad(flamers).unwrap();
    for _ in 0..END_TICKS {
        tick(&mut loaded);
    }
    let world = &loaded.simulation.world;
    assert!(
        world
            .units
            .iter()
            .all(|(_, unit)| unit.damage_over_time_stack_count() == 0),
        "every stack should end after its duration"
    );
}

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-weapon-effects -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn brute_chief_stun_hammer_dazes_and_blocks_attacks_until_the_daze_ends() {
    let (mut loaded, center) = prepared_scenario();
    let marines = spawn_named_squad(&mut loaded, 2, MARINES, center);
    let brute = spawn_named_squad(&mut loaded, 1, BRUTE_CHIEF, center - Vec3::X * 4.0);
    // The Brute Chief upgrade swaps the hammer for the stun hammer.
    swap_action(
        &mut loaded,
        brute,
        "HammerAttackAction",
        "StunHammerAttackAction",
    );
    assert!(issue_command_attack(&mut loaded, brute, marines));

    let dazed = (0..EFFECT_TICKS).any(|_| {
        tick(&mut loaded);
        loaded
            .simulation
            .world
            .get_squad(marines)
            .is_some_and(|squad| squad.is_dazed() && squad.is_attack_blocked())
    });
    assert!(dazed, "the stun hammer should daze the Marines");
    let remaining = loaded
        .simulation
        .world
        .get_squad(marines)
        .and_then(sim::Squad::daze_remaining_ms)
        .unwrap();
    assert!(
        remaining <= 3_000,
        "the daze lasts at most 3 s, got {remaining} ms"
    );

    loaded.simulation.world.remove_squad(brute).unwrap();
    for _ in 0..END_TICKS {
        tick(&mut loaded);
    }
    let squad = loaded.simulation.world.get_squad(marines).unwrap();
    assert!(!squad.is_dazed());
    assert!(!squad.is_attack_blocked());
}

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-weapon-effects -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn vampire_stasis_slows_the_target_claims_it_and_releases_at_the_attack_end() {
    let (mut loaded, center) = prepared_scenario();
    let bomb = loaded
        .simulation
        .gameplay
        .stasis_bomb(VAMPIRE_UNIT, "StasisBombAction")
        .expect("shipped StasisBombAction");
    assert!(bomb.damage > 0.0);
    assert!(bomb.area_damage.radius > 0.0);

    let marines = spawn_named_squad(&mut loaded, 2, MARINES, center);
    let vampire = spawn_named_squad(&mut loaded, 1, VAMPIRE, center - Vec3::X * 15.0);
    loaded.simulation.world.create_pending_physics_objects();
    // The Vampire upgrade swaps the plain stasis beam for the drain beam.
    swap_action(
        &mut loaded,
        vampire,
        "StasisBeamAttackAction",
        "StasisDrainBeamAttackAction",
    );
    assert!(issue_command_attack(&mut loaded, vampire, marines));

    let held = (0..EFFECT_TICKS).any(|_| {
        tick(&mut loaded);
        loaded
            .simulation
            .world
            .get_squad(marines)
            .is_some_and(|squad| squad.stasis_effect_count() > 0)
    });
    assert!(held, "the Vampire beam should put the Marines in stasis");
    tick(&mut loaded);
    let world = &loaded.simulation.world;
    let squad = world.get_squad(marines).unwrap();
    assert!(squad.stasis_speed_multiplier() < 1.0);
    assert_eq!(squad.smart_target_references(SmartTargetType::Stasis), 1);
    assert!(squad_units(world, marines).iter().any(|&unit_id| {
        world
            .get_unit(unit_id)
            .is_some_and(sim::Unit::has_stasis_action)
    }));

    loaded
        .simulation
        .world
        .get_squad_mut(vampire)
        .unwrap()
        .clear_attack_order();
    for _ in 0..4 {
        tick(&mut loaded);
    }
    let squad = loaded.simulation.world.get_squad(marines).unwrap();
    assert_eq!(squad.stasis_effect_count(), 0);
    assert_eq!(squad.smart_target_references(SmartTargetType::Stasis), 0);
    assert_eq!(squad.stasis_speed_multiplier().to_bits(), 1.0_f32.to_bits());
}

/// The action overrides of an upgrade technology on every unit of the squad.
fn swap_action(loaded: &mut LoadedGameScenario, squad_id: EntityId, old: &str, new: &str) {
    let world = &mut loaded.simulation.world;
    for unit_id in squad_units(world, squad_id) {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.actions.set_enabled(old, false);
        unit.actions.set_enabled(new, true);
    }
}

/// An attack order with the generic `Command` ability, which the tactic maps to the
/// `AbilityCommand` attack of the squad.
fn issue_command_attack(
    loaded: &mut LoadedGameScenario,
    squad_id: EntityId,
    target: EntityId,
) -> bool {
    let ability_id = loaded.simulation.gameplay.command_ability_id();
    loaded
        .simulation
        .world
        .issue_attack_order_with_context(1, squad_id, target, 0.0, None, ability_id)
}

fn prepared_scenario() -> (LoadedGameScenario, Vec3) {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    let center = playable_center(&loaded.simulation.world);
    (loaded, center)
}

fn dot_stacks(world: &World, unit_id: EntityId) -> usize {
    world
        .get_unit(unit_id)
        .map_or(0, sim::Unit::damage_over_time_stack_count)
}

fn squad_units(world: &World, squad_id: EntityId) -> Vec<EntityId> {
    world
        .get_squad(squad_id)
        .map(|squad| squad.unit_ids.clone())
        .unwrap_or_default()
}

fn tick(loaded: &mut LoadedGameScenario) {
    let world = &mut loaded.simulation.world;
    world.game_time_ms = world.game_time_ms.wrapping_add(TICK_MS);
    world.update_entities_with_database_and_gameplay(
        TICK_SECONDS,
        &loaded.content.database,
        &loaded.simulation.gameplay,
    );
}

fn spawn_named_squad(
    loaded: &mut LoadedGameScenario,
    player_id: u8,
    name: &str,
    mut position: Vec3,
) -> EntityId {
    let database: &Database = &loaded.content.database;
    let world = &mut loaded.simulation.world;
    let prototype_id = squad_prototype_id(database, name).expect("shipped squad prototype");
    if let Some(height) = world.terrain_height(position, true) {
        position.y = height;
    }
    spawn_squad_at(world, database, player_id, prototype_id, position, Vec3::Z)
        .expect("spawn shipped squad")
}

fn configure_enemies(world: &mut World) {
    world.get_player_mut(1).expect("player one").team_id = 1;
    world.get_player_mut(2).expect("player two").team_id = 2;
    world.configure_standard_team_relations();
}

fn playable_center(world: &World) -> Vec3 {
    let bounds = world
        .effective_playable_bounds()
        .expect("scenario terrain bounds");
    let mut center = Vec3::new(
        f32::midpoint(bounds.min_x(), bounds.max_x()),
        0.0,
        f32::midpoint(bounds.min_z(), bounds.max_z()),
    );
    center.y = world
        .terrain_height(center, true)
        .expect("terrain at playable center");
    center
}

fn remove_scenario_squads(world: &mut World) {
    let ids = world.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        world.remove_squad(id).unwrap();
    }
}
