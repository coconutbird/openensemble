//! Stock UNSC Marine unit profile.
//!
//! Marines use the generic path/formation move action rather than the
//! Warthog's continuously active rigid body. `PhysicsReplacementInfo=dude`
//! is retained as source evidence, but does not create a live vehicle body.

use glam::Vec3;

/// Shipped proto-object name.
pub const MARINE_UNIT_NAME: &str = "unsc_inf_marine_01";
/// Shipped proto-object database ID.
pub const MARINE_PROTO_OBJECT_ID: i32 = 148;
/// Shipped physics-replacement profile.
pub const MARINE_PHYSICS_REPLACEMENT_INFO: &str = "dude";
/// Shipped maximum hit points.
pub const MARINE_HITPOINTS: f32 = 720.0;

/// Complete stock Marine locomotion and obstruction configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarineUnitSpec {
    /// Maximum velocity from `objects.xml`.
    pub max_speed: f32,
    /// Acceleration from `objects.xml`.
    pub acceleration: f32,
    /// Maximum yaw rate in degrees per second.
    pub turn_rate_degrees: f32,
    /// Axis-aligned obstruction radii from `objects.xml`.
    pub half_extents: Vec3,
}

impl Default for MarineUnitSpec {
    fn default() -> Self {
        Self {
            max_speed: 10.0,
            acceleration: 26.0,
            turn_rate_degrees: 540.0,
            half_extents: Vec3::new(1.0, 2.0, 1.0),
        }
    }
}

/// Check whether object metadata selects the stock Marine implementation.
#[must_use]
pub fn is_marine_unit(proto_name: &str) -> bool {
    proto_name.eq_ignore_ascii_case(MARINE_UNIT_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn stock_profile_matches_shipped_data() {
        let spec = MarineUnitSpec::default();
        assert_eq!(MARINE_PROTO_OBJECT_ID, 148);
        assert_close(MARINE_HITPOINTS, 720.0);
        assert_eq!(MARINE_PHYSICS_REPLACEMENT_INFO, "dude");
        assert_close(spec.max_speed, 10.0);
        assert_close(spec.acceleration, 26.0);
        assert_close(spec.turn_rate_degrees, 540.0);
        assert_eq!(spec.half_extents, Vec3::new(1.0, 2.0, 1.0));
    }

    #[test]
    fn recognizes_only_the_stock_marine_key() {
        assert!(is_marine_unit("UNSC_INF_MARINE_01"));
        assert!(!is_marine_unit("unsc_veh_warthog_01"));
    }
}
