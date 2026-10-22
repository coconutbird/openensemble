//! Checks that retail right-click Work commands move and attack with installed Blood Gulch data.

use sim::entities::squads::marine::MARINE_SQUAD_NAME;
use sim::{
    DefaultWorkAction, EntityId, LoadedGameScenario, MS_PER_TICK, PlayerId, Simulation,
    UserWorkTarget, WorkCommand, default_work_action, spawn_squad_at,
    spawn_squad_from_base_by_name, squad_prototype_id,
};

/// Run with:
/// `OPENENSEMBLE_GAME_DIR=<HaloWarsDE> cargo test -p sim --test scenario-user-work -- --ignored`
#[test]
#[ignore = "requires a local Halo Wars DE installation"]
fn blood_gulch_marines_move_and_attack_with_user_work() {
    let mut loaded = test_support::installed_scenario("blood_gulch");
    let mut clock = Simulation::new();
    clock.start();
    let (player_id, squad_id) = spawn_player_marines(&mut loaded);

    let (start, forward) = squad_pose(&loaded, squad_id);
    let waypoint = start + forward * 20.0;
    enqueue(
        &mut clock,
        WorkCommand::user_work(i32::from(player_id), vec![squad_id], waypoint, None, false),
    );
    clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
    let squad = loaded.simulation.world.get_squad(squad_id).unwrap();
    assert_eq!(squad.move_target, Some(waypoint));
    assert_ne!(squad.base.position, start);

    let (enemy_id, enemy_member_id) = spawn_enemy_marines(&mut loaded, player_id, squad_id);
    assert_eq!(
        default_work_action(
            &loaded.simulation.world,
            &loaded.simulation.gameplay,
            squad_id,
            enemy_member_id,
        ),
        Some(DefaultWorkAction::Attack)
    );
    let enemy_position = loaded
        .simulation
        .world
        .get_unit(enemy_member_id)
        .unwrap()
        .base
        .position;
    let target = UserWorkTarget {
        entity: enemy_member_id,
        hit_zone: -1,
    };
    enqueue(
        &mut clock,
        WorkCommand::user_work(
            i32::from(player_id),
            vec![squad_id],
            enemy_position,
            Some(target),
            false,
        ),
    );
    clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
    assert_eq!(
        loaded
            .simulation
            .world
            .get_squad(squad_id)
            .unwrap()
            .attack_target,
        Some(enemy_id)
    );

    let initial_hitpoints = squad_hitpoints(&loaded, enemy_id);
    let mut hitpoints = initial_hitpoints;
    for _ in 0..600 {
        clock.tick_with_scenario(&mut loaded.simulation, &loaded.content.database);
        hitpoints = squad_hitpoints(&loaded, enemy_id);
        if hitpoints < initial_hitpoints {
            break;
        }
    }
    assert!(
        hitpoints < initial_hitpoints,
        "the user Work attack should damage the enemy Marines"
    );
}

fn spawn_player_marines(loaded: &mut LoadedGameScenario) -> (PlayerId, EntityId) {
    let (&player_id, &base_id) = loaded
        .simulation
        .initial_base_ids
        .first_key_value()
        .expect("the skirmish scenario should assign a player base");
    let squad_id = spawn_squad_from_base_by_name(
        &mut loaded.simulation.world,
        &loaded.content.database,
        base_id,
        MARINE_SQUAD_NAME,
    )
    .expect("the real layered database should spawn Marines from a base");
    (player_id, squad_id)
}

fn spawn_enemy_marines(
    loaded: &mut LoadedGameScenario,
    player_id: PlayerId,
    squad_id: EntityId,
) -> (EntityId, EntityId) {
    let world = &loaded.simulation.world;
    let enemy_player_id = world
        .active_players()
        .map(|player| player.id)
        .find(|&other| world.players_are_enemies(player_id, other))
        .expect("the skirmish scenario should contain an enemy player");
    let (position, forward) = squad_pose(loaded, squad_id);
    let marine_id = squad_prototype_id(&loaded.content.database, MARINE_SQUAD_NAME)
        .expect("the real database should contain the Marine squad");
    let enemy_id = spawn_squad_at(
        &mut loaded.simulation.world,
        &loaded.content.database,
        enemy_player_id,
        marine_id,
        position + forward * 10.0,
        -forward,
    )
    .expect("the real database should spawn an enemy Marine squad");
    let member_id = loaded
        .simulation
        .world
        .get_squad(enemy_id)
        .unwrap()
        .unit_ids[0];
    (enemy_id, member_id)
}

fn squad_pose(loaded: &LoadedGameScenario, squad_id: EntityId) -> (glam::Vec3, glam::Vec3) {
    let squad = loaded.simulation.world.get_squad(squad_id).unwrap();
    (squad.base.position, squad.base.forward)
}

fn squad_hitpoints(loaded: &LoadedGameScenario, squad_id: EntityId) -> f32 {
    let world = &loaded.simulation.world;
    world.get_squad(squad_id).map_or(0.0, |squad| {
        squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| world.get_unit(*unit_id))
            .map(|unit| unit.hitpoints)
            .sum()
    })
}

fn enqueue(clock: &mut Simulation, command: WorkCommand) {
    let exec_time = clock.game_time_ms.saturating_add(MS_PER_TICK);
    clock.command_queue.enqueue_work(command, exec_time, 1);
}
