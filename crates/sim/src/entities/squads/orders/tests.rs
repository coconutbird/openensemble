use super::*;
use crate::entity_id::EntityClass;
use crate::player::PlayerId;

fn squad() -> Squad {
    Squad::new(EntityId::new(EntityClass::Squad, 0), PlayerId::from(1))
}

#[test]
fn queued_move_starts_after_active_destination_finishes() {
    let mut squad = squad();
    let first = Vec3::X;
    let second = Vec3::new(2.0, 0.0, 0.0);

    assert!(squad.issue_scripted_move(first, false, false));
    assert!(squad.issue_scripted_move(second, false, true));
    assert_eq!(squad.move_target, Some(Vec3::X));

    squad.finish_current_movement();
    assert_eq!(squad.move_target, Some(Vec3::new(2.0, 0.0, 0.0)));
    assert_eq!(squad.state, SquadState::Moving);

    squad.finish_current_movement();
    assert_eq!(squad.move_target, None);
    assert_eq!(squad.state, SquadState::Idle);
}

#[test]
fn multi_waypoint_command_keeps_each_path_point_in_order() {
    let mut squad = squad();
    let path = [Vec3::X, Vec3::new(2.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0)];

    assert!(squad.issue_scripted_path(&path, true, false));
    for target in path {
        assert_eq!(squad.move_target, Some(target));
        assert!(squad.is_executing_attack_move());
        squad.finish_current_movement();
    }
    assert_eq!(squad.state, SquadState::Idle);
}

#[test]
fn automatic_engagement_resumes_the_suspended_attack_move() {
    let mut squad = squad();
    let destination = Vec3::new(20.0, 0.0, 0.0);
    let target = EntityId::new(EntityClass::Unit, 0);

    assert!(squad.issue_scripted_move(destination, true, false,));
    assert!(squad.begin_attack_move_engagement(target));
    assert_eq!(squad.state, SquadState::Attacking);
    assert_eq!(squad.attack_target, Some(target));

    squad.clear_attack_order();
    assert_eq!(squad.state, SquadState::Moving);
    assert_eq!(squad.move_target, Some(destination));
    assert_eq!(squad.attack_target, None);
}

#[test]
fn non_finite_destination_is_rejected_atomically() {
    let mut squad = squad();
    assert!(!squad.issue_scripted_move(Vec3::splat(f32::NAN), false, false));
    assert_eq!(squad.move_target, None);
    assert_eq!(squad.state, SquadState::Idle);
}
