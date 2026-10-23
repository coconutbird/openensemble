//! Stock UNSC Warthog unit gameplay profile.
//!
//! Values come from the shipped object record, plus the recovered Warthog move action's fixed
//! 60 units/s² speed ramp. The body comes from the `warthog` physics info (see
//! `crate::physics`).

use glam::Vec3;

/// Shipped proto-object name.
pub const WARTHOG_UNIT_NAME: &str = "unsc_veh_warthog_01";
/// `PhysicsInfo` key selected by the Warthog proto object.
pub const WARTHOG_PHYSICS_INFO: &str = "warthog";
/// Shipped maximum hit points.
pub const WARTHOG_HITPOINTS: f32 = 2_369.0;

/// Stock Warthog movement configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarthogUnitSpec {
    /// Maximum desired velocity from `objects.xml`.
    pub max_speed: f32,
    /// Speed ramp recovered from `BUnitActionMoveWarthog`.
    pub acceleration: f32,
    /// Maximum yaw rate from `objects.xml`, in degrees per second.
    pub turn_rate_degrees: f32,
    /// Axis-aligned obstruction radii from `objects.xml`.
    pub half_extents: Vec3,
}

impl Default for WarthogUnitSpec {
    fn default() -> Self {
        Self {
            max_speed: 40.0,
            acceleration: 60.0,
            turn_rate_degrees: 450.0,
            half_extents: Vec3::new(5.0, 3.0, 5.0),
        }
    }
}

/// Check whether object metadata selects the stock Warthog implementation.
#[must_use]
pub fn is_warthog_unit(proto_name: &str, physics_info: Option<&str>) -> bool {
    proto_name.eq_ignore_ascii_case(WARTHOG_UNIT_NAME)
        || physics_info.is_some_and(|name| name.eq_ignore_ascii_case(WARTHOG_PHYSICS_INFO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_profile_matches_shipped_data() {
        let spec = WarthogUnitSpec::default();

        assert_eq!(spec.half_extents, Vec3::new(5.0, 3.0, 5.0));
        assert_eq!(spec.max_speed.to_bits(), 40.0_f32.to_bits());
        assert_eq!(spec.acceleration.to_bits(), 60.0_f32.to_bits());
        assert_eq!(spec.turn_rate_degrees.to_bits(), 450.0_f32.to_bits());
    }

    #[test]
    fn recognizes_unit_keys_case_insensitively() {
        assert!(is_warthog_unit("UNSC_VEH_WARTHOG_01", None));
        assert!(is_warthog_unit("custom_hog", Some("Warthog")));
        assert!(!is_warthog_unit("unsc_inf_marine_01", None));
    }
}
