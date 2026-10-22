use crate::command_queue::CommandEntry;
use crate::commands::{CommandPayload, UserWorkTarget, WorkCommand};
use crate::entities::{SquadContainmentState, SquadState, UnitGarrison};
use crate::work_action::tests::{
    marine_catalog, marine_tactics, squad_with_unit, two_player_world,
};
use crate::{CommandExecutor, EntityId, World};
use glam::Vec3;
use pipeline::database::hw1::Database;

const WAYPOINT: Vec3 = Vec3::new(30.0, 0.0, 40.0);

#[test]
fn ground_click_moves_squads_to_the_waypoint() {
    let mut world = two_player_world();
    let first = world.create_squad(1);
    let second = world.create_squad(1);

    execute(
        &CommandExecutor::new(),
        &mut world,
        WorkCommand::user_work(1, vec![first, second], WAYPOINT, None, false),
    );

    for squad_id in [first, second] {
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.state, SquadState::Moving);
        assert_eq!(squad.move_target, Some(WAYPOINT));
    }
}

#[test]
fn queued_ground_click_waits_for_the_active_move() {
    let mut world = two_player_world();
    let squad_id = world.create_squad(1);
    let first_target = Vec3::X;
    assert!(world.issue_move_order(1, squad_id, first_target));

    execute(
        &CommandExecutor::new(),
        &mut world,
        WorkCommand::user_work(1, vec![squad_id], WAYPOINT, None, true),
    );
    assert_eq!(
        world.get_squad(squad_id).unwrap().move_target,
        Some(first_target)
    );

    world
        .get_squad_mut(squad_id)
        .unwrap()
        .finish_current_movement();
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.move_target, Some(WAYPOINT));
    assert!(!squad.is_executing_attack_move());
}

#[test]
fn enemy_squad_member_and_building_targets_are_attacked() {
    let mut world = two_player_world();
    let enemy = squad_with_unit(&mut world, 2, "enemy");
    let enemy_member = world.get_squad(enemy).unwrap().unit_ids[0];
    let building = world.create_building(2);
    let gameplay = marine_catalog(marine_tactics());
    let database = Database::new();
    let executor = CommandExecutor::with_database_and_gameplay(&database, &gameplay);

    for (target, expected) in [(enemy, enemy), (enemy_member, enemy), (building, building)] {
        let attacker = squad_with_unit(&mut world, 1, "marine");
        execute(
            &executor,
            &mut world,
            WorkCommand::user_work(1, vec![attacker], WAYPOINT, Some(hovered(target)), false),
        );
        let squad = world.get_squad(attacker).unwrap();
        assert_eq!(squad.attack_target, Some(expected), "target {target:?}");
        assert_ne!(squad.move_target, Some(WAYPOINT));
    }
}

#[test]
fn own_target_without_action_moves_to_the_waypoint() {
    let mut world = two_player_world();
    let squad_id = squad_with_unit(&mut world, 1, "marine");
    let own = squad_with_unit(&mut world, 1, "enemy");
    let gameplay = marine_catalog(marine_tactics());
    let database = Database::new();

    execute(
        &CommandExecutor::with_database_and_gameplay(&database, &gameplay),
        &mut world,
        WorkCommand::user_work(1, vec![squad_id], WAYPOINT, Some(hovered(own)), false),
    );

    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.attack_target, None);
    assert_eq!(squad.move_target, Some(WAYPOINT));
}

#[test]
fn own_container_target_starts_a_garrison() {
    let mut world = two_player_world();
    let squad_id = squad_with_unit(&mut world, 1, "marine");
    let bunker = squad_with_unit(&mut world, 1, "bunker");
    let bunker_unit = world.get_squad(bunker).unwrap().unit_ids[0];
    world.get_unit_mut(bunker_unit).unwrap().garrison =
        UnitGarrison::container(0.0, false, false, Vec::new());
    let gameplay = marine_catalog(marine_tactics());
    let database = Database::new();

    execute(
        &CommandExecutor::with_database_and_gameplay(&database, &gameplay),
        &mut world,
        WorkCommand::user_work(
            1,
            vec![squad_id],
            WAYPOINT,
            Some(hovered(bunker_unit)),
            false,
        ),
    );

    assert!(matches!(
        world.get_squad(squad_id).unwrap().garrison.state(),
        SquadContainmentState::Garrisoning { target, .. } if target == bunker_unit
    ));
}

#[test]
fn recipient_filter_drops_foreign_and_capturing_squads() {
    let mut world = two_player_world();
    let free = squad_with_unit(&mut world, 1, "marine");
    let capturing = squad_with_unit(&mut world, 1, "marine");
    let foreign = squad_with_unit(&mut world, 2, "marine");
    let target_squad = squad_with_unit(&mut world, 0, "node");
    let target = world.get_squad(target_squad).unwrap().unit_ids[0];
    let capturing_unit = world.get_squad(capturing).unwrap().unit_ids[0];
    world
        .get_unit_mut(target)
        .unwrap()
        .capture
        .target
        .start_unit(capturing_unit);

    execute(
        &CommandExecutor::new(),
        &mut world,
        WorkCommand::user_work(
            1,
            vec![free, capturing, foreign],
            WAYPOINT,
            Some(hovered(target)),
            false,
        ),
    );

    assert_eq!(world.get_squad(free).unwrap().move_target, Some(WAYPOINT));
    assert_eq!(world.get_squad(capturing).unwrap().move_target, None);
    assert_eq!(world.get_squad(foreign).unwrap().move_target, None);
}

#[test]
fn waypoint_outside_the_simulation_bounds_issues_no_order() {
    let mut world = two_player_world();
    assert!(world.configure_terrain_bounds(Vec3::ZERO, Vec3::new(100.0, 0.0, 100.0)));
    let squad_id = world.create_squad(1);
    let executor = CommandExecutor::new();

    let max_edge = Vec3::new(100.0, 0.0, 50.0);
    execute(
        &executor,
        &mut world,
        WorkCommand::user_work(1, vec![squad_id], max_edge, None, false),
    );
    assert_eq!(world.get_squad(squad_id).unwrap().move_target, None);

    let inside = Vec3::new(99.0, 0.0, 50.0);
    execute(
        &executor,
        &mut world,
        WorkCommand::user_work(1, vec![squad_id], inside, None, false),
    );
    assert_eq!(world.get_squad(squad_id).unwrap().move_target, Some(inside));
}

const fn hovered(entity: EntityId) -> UserWorkTarget {
    UserWorkTarget {
        entity,
        hit_zone: -1,
    }
}

fn execute(executor: &CommandExecutor<'_>, world: &mut World, command: WorkCommand) {
    executor.execute(
        world,
        &CommandEntry {
            command: CommandPayload::Work(command),
            exec_time: 0,
            sequence: 0,
            source_client: 1,
        },
    );
}
