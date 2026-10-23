//! Checks the death throw of a weapon kill on Blood Gulch: a Scorpion kills Marines, each dead
//! Marine becomes a thrown clamshell physics replacement, and the replacement stays until its
//! body comes to rest.

use glam::Vec3;
use pipeline::database::hw1::Database;
use sim::{EntityId, LoadedGameScenario, World, spawn_squad_at, squad_prototype_id};

const MARINES: &str = "unsc_inf_marine_01";
/// The Scorpion cannon has `ThrowUnits` (force 800 to 1200, launch angle 5 to 15 degrees).
const SCORPION: &str = "unsc_veh_scorpion_01";
const TICK_MS: u32 = 50;
const TICK_SECONDS: f32 = 0.05;
/// The ticks that the Scorpion gets to kill a Marine.
const KILL_TICKS: usize = 1_200;
/// The ticks that the thrown replacement gets to come to rest and go.
const REST_TICKS: usize = 1_200;

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-death-throw -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_weapon_kill_throws_a_marine_clamshell_that_rests_and_goes() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let replacement = loaded
        .simulation
        .gameplay
        .physics_replacement(MARINES)
        .expect("shipped Marine PhysicsReplacementInfo");
    assert!(replacement.is_clamshell());
    assert!(replacement.info().thrown_by_projectiles());
    remove_scenario_squads(&mut loaded.simulation.world);
    configure_enemies(&mut loaded.simulation.world);
    let center = playable_center(&loaded.simulation.world);
    let marines = spawn_named_squad(&mut loaded, 2, MARINES, center);
    let scorpion = spawn_named_squad(&mut loaded, 1, SCORPION, center - Vec3::X * 25.0);
    loaded.simulation.world.create_pending_physics_objects();
    assert!(
        loaded
            .simulation
            .world
            .issue_attack_order(1, scorpion, marines, 0.0)
    );

    let thrown = (0..KILL_TICKS)
        .find_map(|_| {
            tick(&mut loaded);
            thrown_marine(&loaded.simulation.world)
        })
        .expect("the Scorpion should kill a Marine and throw its replacement");
    let world = &loaded.simulation.world;
    let unit = world.get_unit(thrown).unwrap();
    let state = unit.physics_object.as_ref().expect("the replacement body");
    assert!(state.clamshell_bodies().is_some());
    assert_eq!(unit.hitpoints.to_bits(), 0.0_f32.to_bits());
    assert!(!unit.base.is_selectable());
    let velocity = world.unit_body_linear_velocity(thrown).unwrap();
    assert!(
        velocity.length() > 1.0,
        "the replacement should fly after the throw: {velocity}"
    );

    // The replacement stays while its body moves and goes once its physics action waits.
    let mut rested = false;
    let removed = (0..REST_TICKS).any(|_| {
        rested = loaded.simulation.world.unit_body_is_active(thrown) == Some(false);
        tick(&mut loaded);
        loaded.simulation.world.get_unit(thrown).is_none()
    });
    assert!(removed, "the thrown replacement should go once it rests");
    assert!(
        rested,
        "the replacement should go only after its body rests"
    );
}

/// A dead Marine physics replacement with a body.
fn thrown_marine(world: &World) -> Option<EntityId> {
    world.units.iter().find_map(|(id, unit)| {
        (unit.proto_object_name.eq_ignore_ascii_case(MARINES)
            && unit.is_physics_replacement()
            && unit.hitpoints <= 0.0
            && unit.physics_object.is_some())
        .then_some(id)
    })
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
