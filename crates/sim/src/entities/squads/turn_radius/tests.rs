use glam::Vec3;

use super::{TurnRadiusLeader, TurnRadiusProgress, yaw_from_direction};
use crate::entities::Squad;
use crate::entity_id::EntityId;

fn squad() -> Squad {
    let mut squad = Squad::new(EntityId::default(), 1);
    squad.base.forward = Vec3::Z;
    squad.start_turn_radius();
    squad
}

const LEADER: TurnRadiusLeader = TurnRadiusLeader {
    turn_rate_degrees: 90.0,
    desired_velocity: 10.0,
};

#[test]
fn the_yaw_grows_from_z_toward_x_in_zero_to_two_pi() {
    use std::f32::consts::{FRAC_PI_2, PI};
    assert!(yaw_from_direction(Vec3::Z).abs() < 1e-6);
    assert!((yaw_from_direction(Vec3::X) - FRAC_PI_2).abs() < 1e-6);
    assert!((yaw_from_direction(-Vec3::Z) - PI).abs() < 1e-6);
    assert!((yaw_from_direction(-Vec3::X) - 3.0 * FRAC_PI_2).abs() < 1e-5);
}

#[test]
fn the_point_catches_up_at_up_to_twice_the_desired_velocity() {
    let mut squad = squad();
    squad.base.position = Vec3::new(0.0, 0.0, 20.0);
    let progress = squad.update_turning(0.05, Some(LEADER), false);
    assert_eq!(progress, TurnRadiusProgress::Working);
    let (position, forward) = squad.turn_radius().expect("point");
    // Far behind: the smooth step gives the full speedup, 2 · 10 · 0.05.
    assert!((position.z - 1.0).abs() < 1e-5, "{position}");
    assert!((forward - Vec3::Z).length() < 1e-6);
}

#[test]
fn the_point_turns_at_most_the_turn_rate_and_reports_done_when_caught_up() {
    let mut turning = squad();
    turning.base.position = Vec3::new(20.0, 0.0, 0.0);
    turning.update_turning(0.05, Some(LEADER), false);
    let (_, forward) = turning.turn_radius().expect("point");
    // 90° away: the turn speed doubles to 180°/s, so one update turns 9°.
    let yaw = yaw_from_direction(forward).to_degrees();
    assert!((yaw - 9.0).abs() < 0.01, "yaw {yaw}");
    let mut caught_up = squad();
    assert_eq!(
        caught_up.update_turning(0.05, Some(LEADER), false),
        TurnRadiusProgress::Pathing
    );
    assert_eq!(
        caught_up.update_turning(0.05, Some(LEADER), true),
        TurnRadiusProgress::Done
    );
}
