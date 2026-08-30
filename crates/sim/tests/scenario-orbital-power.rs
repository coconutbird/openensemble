use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{
    EntityId, NativePowerInput, OrbitalPowerInvocation, PowerUserId, load_scenario_from_game_dir,
    power_prototype_id, spawn_squad_at, squad_prototype_id,
};

const ORBITAL_POWER: &str = "UnscLeaderOrbitalBombard";
const MARINES: &str = "unsc_inf_marine_01";
const TARGET_BEAM: &str = "fx_proj_maccannontargetbeam_01";
const PROJECTILE: &str = "pow_gp_macCannonSmall";
const EFFECT: &str = "pow_gp_macCannonVisualSmall";
const ROCKS: [&str; 3] = [
    "pow_gp_macblast_rocks_small",
    "pow_gp_macblast_rocks_medium",
    "pow_gp_macblast_rocks_large",
];

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-orbital-power -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn shipped_orbital_profile_uses_layered_tactics_and_throws_synchronized_debris() {
    let mut loaded = load_installed_scenario();
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    assert_shipped_profile(&loaded);
    let database = &loaded.content.database;
    let target_squad = spawn_named_squad(
        &mut loaded.simulation.world,
        database,
        2,
        MARINES,
        Vec3::ZERO,
    );
    let target = loaded
        .simulation
        .world
        .get_squad(target_squad)
        .unwrap()
        .base
        .position;
    let target_units = loaded
        .simulation
        .world
        .get_squad(target_squad)
        .unwrap()
        .unit_ids
        .clone();
    let health_before = total_health(&loaded.simulation.world, &target_units);
    let power_id = power_prototype_id(database, ORBITAL_POWER).expect("shipped Orbital power");
    let execution_id = loaded
        .simulation
        .world
        .invoke_orbital_power(database, invocation(power_id, target))
        .expect("shipped Orbital profile should fully resolve");
    let beam_id = loaded.simulation.world.active_orbital_powers()[0].real_targeting_laser_id();
    assert_proto(&loaded.simulation.world, beam_id, TARGET_BEAM);

    assert!(loaded.simulation.world.submit_orbital_power_input(
        database,
        execution_id,
        NativePowerInput::Confirm(target),
    ));
    let execution = &loaded.simulation.world.active_orbital_powers()[0];
    assert_eq!(execution.shots_remaining(), 0);
    assert_eq!(execution.pending_shots().len(), 1);
    assert!(loaded.simulation.world.get_object(beam_id).is_none());
    advance(&mut loaded, 1);
    let shot_laser =
        loaded.simulation.world.active_orbital_powers()[0].pending_shots()[0].laser_object_id();
    assert_proto(&loaded.simulation.world, shot_laser, TARGET_BEAM);

    advance_until(&mut loaded, 20, |world| {
        !world.active_orbital_powers()[0]
            .active_projectile_ids()
            .is_empty()
    });
    assert_eq!(count_objects(&loaded.simulation.world, EFFECT), 1);
    assert!(
        loaded
            .simulation
            .world
            .projectiles
            .iter()
            .any(|(_, projectile)| projectile
                .proto_object_name
                .eq_ignore_ascii_case(PROJECTILE))
    );
    advance_until(&mut loaded, 20, |world| {
        world.active_orbital_powers().is_empty()
    });
    assert!(total_health(&loaded.simulation.world, &target_units) < health_before);
    let debris_ids = debris_ids(&loaded.simulation.world);
    assert!((13..=23).contains(&debris_ids.len()));
    assert!(debris_ids.iter().all(|id| {
        loaded
            .simulation
            .world
            .get_object(*id)
            .is_some_and(|object| object.base.velocity.length_squared() > 0.0)
    }));
    let moving_id = debris_ids[0];
    let old_position = loaded
        .simulation
        .world
        .get_object(moving_id)
        .unwrap()
        .base
        .position;
    advance(&mut loaded, 1);
    assert_ne!(
        loaded
            .simulation
            .world
            .get_object(moving_id)
            .unwrap()
            .base
            .position,
        old_position
    );
}

fn advance(loaded: &mut sim::LoadedGameScenario, ticks: usize) {
    for _ in 0..ticks {
        loaded.simulation.world.game_time_ms =
            loaded.simulation.world.game_time_ms.wrapping_add(100);
        loaded
            .simulation
            .world
            .update_entities_with_database_and_gameplay(
                0.1,
                &loaded.content.database,
                &loaded.simulation.gameplay,
            );
    }
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
        advance(loaded, 1);
    }
    assert!(
        complete(&loaded.simulation.world),
        "Orbital state did not converge"
    );
}

fn assert_shipped_profile(loaded: &sim::LoadedGameScenario) {
    let database = &loaded.content.database;
    let power_id = power_prototype_id(database, ORBITAL_POWER).unwrap();
    let power = &database.powers[usize::try_from(power_id).unwrap()];
    let attributes = power.attributes.as_ref().expect("power attributes");
    assert_eq!(attributes.power_type.as_deref(), Some("Orbital"));
    let level = attributes
        .data_levels
        .iter()
        .find(|level| level.level == Some(0))
        .expect("level zero");
    assert!(
        level.entries.iter().any(|entry| {
            entry.name == "NumShots" && entry.value.trim().parse::<i32>() == Ok(1)
        })
    );
    let tactics = loaded
        .simulation
        .gameplay
        .object(PROJECTILE)
        .expect("layered Orbital tactics")
        .tactics();
    let weapon = tactics
        .weapons
        .iter()
        .find(|weapon| weapon.name == "OrbitalBombardment")
        .expect("Orbital tactic weapon");
    assert_eq!(weapon.damage_per_second, Some(30_000.0));
    assert_eq!(weapon.aoe_radius, Some(25.0));
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

fn debris_ids(world: &sim::World) -> Vec<EntityId> {
    world
        .objects
        .iter()
        .filter(|(_, object)| {
            ROCKS
                .iter()
                .any(|name| object.proto_object_name.eq_ignore_ascii_case(name))
        })
        .map(|(id, _)| id)
        .collect()
}

fn count_objects(world: &sim::World, name: &str) -> usize {
    world
        .objects
        .iter()
        .filter(|(_, object)| object.proto_object_name.eq_ignore_ascii_case(name))
        .count()
}

fn assert_proto(world: &sim::World, id: EntityId, name: &str) {
    assert_eq!(
        world
            .get_object(id)
            .map(|object| object.proto_object_name.as_str()),
        Some(name)
    );
}

fn invocation(proto_power_id: i32, target_location: Vec3) -> OrbitalPowerInvocation {
    OrbitalPowerInvocation {
        player_id: 1,
        proto_power_id,
        power_level: 0,
        squad_id: EntityId::INVALID,
        target_location,
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
