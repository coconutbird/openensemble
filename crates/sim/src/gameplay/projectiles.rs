//! Immutable projectile movement definitions from the layered object database.

use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeMap;

/// Simulation-relevant values for a projectile proto object.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileProfile {
    /// Database ID, or the database index when no explicit ID is authored.
    pub proto_object_id: i32,
    /// Authored proto-object name.
    pub proto_object_name: String,
    /// Desired flight speed in world units per second.
    pub speed: f32,
    /// Initial speed for accelerating projectiles.
    pub starting_speed: f32,
    /// Forward acceleration while fuel remains.
    pub acceleration: f32,
    /// Maximum lifetime in seconds.
    pub lifespan: f32,
    /// Whether the projectile updates its destination from the live target.
    pub tracking: bool,
    /// Delay before tracking begins, in seconds.
    pub tracking_delay: f32,
    /// Maximum authored turn rate in degrees per second.
    pub turn_rate_degrees: f32,
    /// Whether the retail projectile is affected by global projectile gravity.
    pub affected_by_gravity: bool,
}

pub(super) fn collect_projectile_profiles(
    database: &Database,
) -> BTreeMap<String, ProjectileProfile> {
    database
        .objects
        .iter()
        .enumerate()
        .filter(|(_, object)| is_projectile(object))
        .map(|(index, object)| {
            let speed = positive_or(object.velocity, 1.0);
            let profile = ProjectileProfile {
                proto_object_id: object
                    .dbid
                    .unwrap_or_else(|| i32::try_from(index).unwrap_or(-1)),
                proto_object_name: object.name.clone(),
                speed,
                starting_speed: positive_or(object.starting_velocity, speed),
                acceleration: nonnegative_or_zero(object.acceleration),
                lifespan: positive_or(object.lifespan, 10.0),
                tracking: has_flag(object, "Tracking"),
                tracking_delay: nonnegative_or_zero(object.tracking_delay),
                turn_rate_degrees: nonnegative_or_zero(object.turn_rate),
                affected_by_gravity: has_flag(object, "IsAffectedByGravity"),
            };
            (object.name.to_ascii_lowercase(), profile)
        })
        .collect()
}

fn is_projectile(object: &ProtoObject) -> bool {
    object
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Projectile"))
}

fn has_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}

fn positive_or(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(fallback)
}

fn nonnegative_or_zero(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projectile_profiles_are_case_insensitive_and_preserve_motion_flags() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "Test_Rocket".to_owned(),
            dbid: Some(42),
            object_class: Some("Projectile".to_owned()),
            flags: vec!["tracking".to_owned(), "IsAffectedByGravity".to_owned()],
            velocity: Some(65.0),
            starting_velocity: Some(20.0),
            acceleration: Some(40.0),
            lifespan: Some(2.0),
            tracking_delay: Some(0.3),
            turn_rate: Some(680.0),
            ..ProtoObject::default()
        });

        let profiles = collect_projectile_profiles(&database);
        let profile = profiles.get("test_rocket").expect("projectile profile");
        assert_eq!(profile.proto_object_id, 42);
        assert!((profile.speed - 65.0).abs() < f32::EPSILON);
        assert!((profile.starting_speed - 20.0).abs() < f32::EPSILON);
        assert!((profile.acceleration - 40.0).abs() < f32::EPSILON);
        assert!(profile.tracking);
        assert!(profile.affected_by_gravity);
    }
}
