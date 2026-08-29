//! Retail synchronized projectile hit and miss distribution.

use crate::gameplay::AttackAccuracyProfile;
use crate::random::SimRandom;
use glam::{Quat, Vec3};

const DEVIATION_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone, Copy)]
pub(super) struct LaunchAccuracy {
    chance: f32,
    maximum_deviation: f32,
    distance_factor: f32,
    deviation_factor: f32,
}

impl LaunchAccuracy {
    pub(super) fn new(
        profile: AttackAccuracyProfile,
        moving_at_full_speed: bool,
        unit_accuracy_scalar: f32,
        unit_dodge_scalar: f32,
    ) -> Self {
        let chance = if moving_at_full_speed {
            profile.moving_accuracy
        } else {
            profile.accuracy
        } * unit_accuracy_scalar
            * unit_dodge_scalar;
        let mut maximum_deviation = if moving_at_full_speed {
            profile.moving_max_deviation
        } else {
            profile.max_deviation
        };
        if unit_accuracy_scalar != 0.0 {
            maximum_deviation /= unit_accuracy_scalar;
            maximum_deviation /= unit_dodge_scalar;
        }
        Self {
            chance,
            maximum_deviation,
            distance_factor: profile.distance_factor,
            deviation_factor: profile.deviation_factor,
        }
    }
}

pub(super) fn projectile_deviation(
    rng: &mut SimRandom,
    source: Vec3,
    target: Vec3,
    targeting_lead: Vec3,
    max_range: f32,
    accuracy: LaunchAccuracy,
) -> Vec3 {
    let accuracy_roll = rng.distribution();
    if accuracy_roll <= accuracy.chance {
        return Vec3::ZERO;
    }
    let deviation_roll = rng.distribution();
    let straight_trajectory = target + targeting_lead - source;
    let range = straight_trajectory.length();
    let factor = distribution_factor(
        deviation_roll,
        accuracy.distance_factor,
        accuracy.deviation_factor,
    );
    let deviation = factor * accuracy.maximum_deviation * range / max_range;
    let rotation_roll = rng.distribution();
    if range <= DEVIATION_EPSILON || max_range.abs() <= DEVIATION_EPSILON {
        return Vec3::ZERO;
    }
    let trajectory = straight_trajectory / range;
    let deviation_axis = if trajectory == Vec3::Y {
        trajectory.cross(Vec3::X)
    } else {
        trajectory.cross(Vec3::Y)
    };
    let rotation = Quat::from_axis_angle(trajectory, rotation_roll * std::f32::consts::TAU);
    let result = rotation * deviation_axis * deviation;
    if result.is_finite() {
        result
    } else {
        Vec3::ZERO
    }
}

fn distribution_factor(roll: f32, distance_factor: f32, deviation_factor: f32) -> f32 {
    if roll <= distance_factor {
        if distance_factor.abs() <= DEVIATION_EPSILON {
            0.0
        } else {
            deviation_factor * roll / distance_factor
        }
    } else {
        let denominator = 1.0 - distance_factor;
        if denominator.abs() <= DEVIATION_EPSILON {
            1.0
        } else {
            deviation_factor + (1.0 - deviation_factor) * ((roll - distance_factor) / denominator)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_shot_still_consumes_one_synchronized_roll() {
        let mut actual_rng = SimRandom::new();
        let mut expected_rng = actual_rng;
        let _roll = expected_rng.distribution();
        let deviation = projectile_deviation(
            &mut actual_rng,
            Vec3::ZERO,
            Vec3::X * 10.0,
            Vec3::ZERO,
            10.0,
            LaunchAccuracy::new(AttackAccuracyProfile::default(), false, 1.0, 1.0),
        );
        assert_eq!(deviation, Vec3::ZERO);
        assert_eq!(actual_rng.seed(), expected_rng.seed());
    }

    #[test]
    fn miss_consumes_three_rolls_and_stays_on_the_deviation_plane() {
        let mut rng = SimRandom::new();
        let mut expected_rng = rng;
        for _ in 0..3 {
            let _roll = expected_rng.distribution();
        }
        let accuracy = LaunchAccuracy::new(
            AttackAccuracyProfile {
                accuracy: -1.0,
                max_deviation: 4.0,
                ..AttackAccuracyProfile::default()
            },
            false,
            1.0,
            1.0,
        );
        let deviation = projectile_deviation(
            &mut rng,
            Vec3::ZERO,
            Vec3::X * 10.0,
            Vec3::ZERO,
            10.0,
            accuracy,
        );
        assert_eq!(rng.seed(), expected_rng.seed());
        assert!(deviation.length() > 0.0);
        assert!(deviation.dot(Vec3::X).abs() < 0.000_1);
        assert!(deviation.length() <= 4.0 + f32::EPSILON);
    }

    #[test]
    fn unit_accuracy_scalar_changes_chance_and_inverse_spread() {
        let accuracy = LaunchAccuracy::new(
            AttackAccuracyProfile {
                accuracy: 0.5,
                max_deviation: 4.0,
                ..AttackAccuracyProfile::default()
            },
            false,
            2.0,
            0.5,
        );
        assert!((accuracy.chance - 0.5).abs() < f32::EPSILON);
        assert!((accuracy.maximum_deviation - 4.0).abs() < f32::EPSILON);
    }
}
