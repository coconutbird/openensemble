//! Retail projectile launch and in-flight steering calculations.

use super::ProjectileLaunch;
use crate::gameplay::ProjectileProfile;
use glam::{Quat, Vec3};

const MOTION_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone, Copy)]
pub(super) struct LaunchMotion {
    pub velocity: Vec3,
    pub gravity: f32,
    pub affected_by_gravity: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TrackingTarget {
    pub position: Vec3,
    pub intercepting: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TrackingMotion {
    pub velocity: Vec3,
    pub step: Vec3,
    pub facing: Vec3,
}

pub(super) fn launch_motion(
    launch: &ProjectileLaunch,
    profile: &ProjectileProfile,
) -> LaunchMotion {
    let displacement = launch.target_position - launch.source_position;
    let direction = displacement.normalize_or(Vec3::Z);
    let Some((velocity, gravity)) = ballistic_motion(launch, profile) else {
        let speed = if profile.acceleration > MOTION_EPSILON {
            profile.starting_speed
        } else {
            profile.speed
        };
        return LaunchMotion {
            velocity: direction * speed,
            gravity: 0.0,
            affected_by_gravity: false,
        };
    };

    let velocity = if profile.acceleration > MOTION_EPSILON {
        velocity * profile.starting_speed
    } else {
        velocity
    };
    LaunchMotion {
        velocity,
        gravity,
        affected_by_gravity: true,
    }
}

fn ballistic_motion(launch: &ProjectileLaunch, profile: &ProjectileProfile) -> Option<(Vec3, f32)> {
    ballistic_solution(
        launch.source_position,
        launch.source_position,
        launch.target_position,
        launch.target_entity_position,
        launch.target_radius,
        launch.max_range,
        profile,
    )
}

pub(crate) fn ballistic_aim_direction(
    source: Vec3,
    attacker_position: Vec3,
    target: Vec3,
    target_entity_position: Vec3,
    target_radius: f32,
    max_range: f32,
    profile: &ProjectileProfile,
) -> Option<Vec3> {
    ballistic_solution(
        source,
        attacker_position,
        target,
        target_entity_position,
        target_radius,
        max_range,
        profile,
    )
    .and_then(|(velocity, _gravity)| velocity.try_normalize())
}

fn ballistic_solution(
    source: Vec3,
    attacker_position: Vec3,
    target: Vec3,
    target_entity_position: Vec3,
    target_radius: f32,
    max_range: f32,
    profile: &ProjectileProfile,
) -> Option<(Vec3, f32)> {
    if !profile.behavior.affected_by_gravity()
        || profile.max_projectile_height <= 0.0
        || max_range <= 0.0
    {
        return None;
    }
    let displacement = target - source;
    let distance = displacement.length();
    if distance <= MOTION_EPSILON || profile.speed <= MOTION_EPSILON {
        return None;
    }
    let center_offset = target_entity_position - attacker_position;
    let horizontal_offset = Vec3::new(center_offset.x, 0.0, center_offset.z);
    let horizontal_distance = horizontal_offset.length();
    let horizontal_direction = horizontal_offset.try_normalize().unwrap_or(Vec3::ZERO);
    let launch_offset = source - attacker_position;
    let horizontal_launch_offset =
        Vec3::new(launch_offset.x, 0.0, launch_offset.z).dot(horizontal_direction);
    let horizontal_hit_distance = Vec3::new(displacement.x, 0.0, displacement.z).length();
    let target_radius = target_radius.max(0.0);
    let horizontal_target_radius = target_radius * horizontal_hit_distance / distance;
    let range =
        (horizontal_distance - horizontal_target_radius - horizontal_launch_offset).max(0.0);
    let flight_time = distance / profile.speed;
    let half_time = 0.5 * flight_time;
    if half_time <= MOTION_EPSILON {
        return None;
    }
    let height_scale = ((range * range) / (max_range * max_range)).min(1.0);
    let scaled_height = profile.max_projectile_height * height_scale;
    let acceleration = -2.0 * scaled_height / (half_time * half_time);
    let vertical_velocity = scaled_height / half_time - 0.5 * acceleration * half_time;
    let mut velocity = displacement * (profile.speed / distance);
    velocity.y += vertical_velocity;
    let gravity = -acceleration;
    (velocity.is_finite() && gravity.is_finite()).then_some((velocity, gravity))
}

pub(crate) fn launch_target_position(
    source: Vec3,
    target: Vec3,
    target_velocity: Vec3,
    projectile_speed: f32,
    maximum_velocity_lead: f32,
) -> Vec3 {
    let distance = source.distance(target);
    let target_speed = target_velocity.length();
    if distance <= MOTION_EPSILON
        || projectile_speed <= MOTION_EPSILON
        || target_speed <= MOTION_EPSILON
        || maximum_velocity_lead <= 0.0
    {
        return target;
    }
    let capped_velocity =
        target_velocity * (target_speed.min(maximum_velocity_lead) / target_speed);
    let lead = capped_velocity * (distance / projectile_speed);
    let led_target = target + lead;
    target + lead * (source.distance(led_target) / distance)
}

pub(super) fn tracking_target(
    projectile_position: Vec3,
    target_position: Vec3,
    target_velocity: Vec3,
    projectile_speed: f32,
    intercept_distance: f32,
) -> TrackingTarget {
    let distance = projectile_position.distance(target_position);
    if distance <= MOTION_EPSILON
        || intercept_distance <= 0.0
        || distance > intercept_distance
        || projectile_speed <= MOTION_EPSILON
    {
        return TrackingTarget {
            position: target_position,
            intercepting: false,
        };
    }
    let weight = ((distance / intercept_distance) * std::f32::consts::FRAC_PI_2).cos();
    let lead = target_velocity * (distance / projectile_speed);
    let led_target = target_position + lead;
    TrackingTarget {
        position: target_position
            + lead * (weight * projectile_position.distance(led_target) / distance),
        intercepting: true,
    }
}

#[cfg(test)]
fn tracking_target_position(
    projectile_position: Vec3,
    target_position: Vec3,
    target_velocity: Vec3,
    projectile_speed: f32,
    intercept_distance: f32,
) -> Vec3 {
    tracking_target(
        projectile_position,
        target_position,
        target_velocity,
        projectile_speed,
        intercept_distance,
    )
    .position
}

#[cfg(test)]
pub(super) fn tracking_velocity(
    velocity: Vec3,
    target_direction: Vec3,
    turn_rate_radians: f32,
    dt: f32,
) -> Vec3 {
    tracking_motion(
        velocity,
        target_direction,
        Vec3::ZERO,
        turn_rate_radians,
        dt,
    )
    .velocity
}

pub(super) fn tracking_motion(
    velocity: Vec3,
    target_direction: Vec3,
    perturbance: Vec3,
    turn_rate_radians: f32,
    dt: f32,
) -> TrackingMotion {
    let speed = velocity.length();
    let perturbed_velocity = velocity + perturbance;
    let perturbed_speed = perturbed_velocity.length();
    let current = perturbed_velocity.try_normalize().unwrap_or(velocity);
    let desired = target_direction.normalize_or(current);
    let target_angle = (1.0 - current.dot(desired).clamp(-1.0, 1.0)) * std::f32::consts::FRAC_PI_2;
    let maximum_angle = (turn_rate_radians * dt).max(0.0);
    let angle = target_angle.min(maximum_angle);
    if angle <= MOTION_EPSILON {
        return TrackingMotion {
            velocity,
            step: current * (perturbed_speed * dt),
            facing: velocity,
        };
    }
    let axis = current.cross(desired).try_normalize().unwrap_or_else(|| {
        let fallback = if current.x.abs() < 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        current.cross(fallback).normalize_or(Vec3::Z)
    });
    let direction = (Quat::from_axis_angle(axis, angle) * current).normalize_or(current);
    TrackingMotion {
        velocity: direction * speed,
        step: direction * (perturbed_speed * dt),
        facing: velocity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::projectiles::{ProjectileBehavior, ProjectilePerturbanceProfile};

    fn ballistic_profile() -> ProjectileProfile {
        ProjectileProfile {
            proto_object_id: 1,
            proto_object_name: "ballistic".to_owned(),
            speed: 10.0,
            starting_speed: 10.0,
            fuel: 0.0,
            acceleration: 0.0,
            max_projectile_height: 5.0,
            lifespan: 5.0,
            tracking_delay: 0.0,
            turn_rate_degrees: 0.0,
            perturbance: ProjectilePerturbanceProfile::default(),
            behavior: ProjectileBehavior::AFFECTED_BY_GRAVITY,
        }
    }

    #[test]
    fn full_range_ballistic_launch_reaches_authored_apex() {
        let launch = ProjectileLaunch::test(Vec3::ZERO, Vec3::X * 10.0, 10.0);
        let motion = launch_motion(&launch, &ballistic_profile());
        assert!(motion.affected_by_gravity);
        assert!((motion.velocity.x - 10.0).abs() < 0.000_1);
        assert!((motion.velocity.y - 20.0).abs() < 0.000_1);
        assert!((motion.gravity - 40.0).abs() < 0.000_1);
    }

    #[test]
    fn hardpoint_aim_uses_retails_attacker_relative_horizontal_range() {
        let direction = ballistic_aim_direction(
            Vec3::Z * 2.0,
            Vec3::ZERO,
            Vec3::X * 10.0,
            Vec3::X * 10.0,
            0.0,
            20.0,
            &ballistic_profile(),
        )
        .expect("gravity profile should produce an aim direction");

        assert!((direction.y / direction.x - 0.5).abs() < 0.000_1);
        assert!((direction.z / direction.x + 0.2).abs() < 0.000_1);
    }

    #[test]
    fn launch_and_tracking_leads_match_retail_weighting() {
        let launch_target =
            launch_target_position(Vec3::ZERO, Vec3::X * 10.0, Vec3::Z * 4.0, 10.0, 2.0);
        assert!(launch_target.z > 2.0);
        assert!(launch_target.z < 2.1);

        let distant =
            tracking_target_position(Vec3::ZERO, Vec3::X * 10.0, Vec3::Z * 4.0, 10.0, 10.0);
        assert!((distant - Vec3::X * 10.0).length() < 0.000_1);
        let close =
            tracking_target_position(Vec3::X * 9.0, Vec3::X * 10.0, Vec3::Z * 4.0, 10.0, 10.0);
        assert!(close.z > 0.39);
    }

    #[test]
    fn tracking_uses_retails_dot_based_turn_angle() {
        let velocity = tracking_velocity(Vec3::X * 10.0, Vec3::Z, 45.0_f32.to_radians(), 1.0);
        assert!((velocity.angle_between(Vec3::X).to_degrees() - 45.0).abs() < 0.001);
        assert_eq!(tracking_velocity(Vec3::X, Vec3::Z, 0.0, 1.0), Vec3::X);
    }

    #[test]
    fn tracking_perturbance_changes_the_step_but_not_stored_speed() {
        let motion = tracking_motion(
            Vec3::X * 10.0,
            Vec3::X,
            Vec3::Z * 5.0,
            90.0_f32.to_radians(),
            0.1,
        );

        assert!((motion.velocity.length() - 10.0).abs() < 0.000_1);
        assert!((motion.step.length() - 5.0_f32.hypot(10.0) * 0.1).abs() < 0.000_1);
        assert_eq!(motion.facing, Vec3::X * 10.0);
    }
}
