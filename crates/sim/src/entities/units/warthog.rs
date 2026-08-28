//! Stock UNSC Warthog unit gameplay and physics profile.
//!
//! Values come from the shipped object, `.physics`, and `.blueprint` records,
//! plus the recovered Warthog move action's fixed 60 units/s² speed ramp.

use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
use glam::Vec3;

/// Shipped proto-object name.
pub const WARTHOG_UNIT_NAME: &str = "unsc_veh_warthog_01";
/// `PhysicsInfo` key selected by the Warthog proto object.
pub const WARTHOG_PHYSICS_INFO: &str = "warthog";
/// Shipped proto-object database ID.
pub const WARTHOG_PROTO_OBJECT_ID: i32 = 156;
/// Shipped maximum hit points.
pub const WARTHOG_HITPOINTS: f32 = 2_369.0;

/// Complete stock Warthog movement and rigid-body configuration.
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
    /// Physics collider center offset from `warthog.physics.xml`.
    pub center_offset: Vec3,
    /// Blueprint mass.
    pub mass: f32,
    /// Blueprint friction.
    pub friction: f32,
    /// Blueprint restitution.
    pub restitution: f32,
    /// Blueprint angular damping.
    pub angular_damping: f32,
}

impl Default for WarthogUnitSpec {
    fn default() -> Self {
        Self {
            max_speed: 40.0,
            acceleration: 60.0,
            turn_rate_degrees: 450.0,
            half_extents: Vec3::new(5.0, 3.0, 5.0),
            center_offset: Vec3::new(0.0, 2.28, 0.0),
            mass: 150.0,
            friction: 2.0,
            restitution: 0.5,
            angular_damping: 0.01,
        }
    }
}

impl WarthogUnitSpec {
    /// Build a dynamic body at the unit origin's current terrain height.
    #[must_use]
    pub fn physics_body(self, ground_height: f32) -> PhysicsBody {
        let material = PhysicsMaterial {
            mass: self.mass,
            friction: self.friction,
            restitution: self.restitution,
            linear_damping: 0.0,
            angular_damping: self.angular_damping,
        };
        let collider = BoxCollider::new(self.half_extents, self.center_offset);
        PhysicsBody::ground_vehicle(
            material,
            collider,
            ground_height,
            self.max_speed,
            self.acceleration,
            self.turn_rate_degrees,
        )
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
    use crate::physics::MotionType;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn stock_profile_matches_shipped_data() {
        let spec = WarthogUnitSpec::default();
        let body = spec.physics_body(12.0);

        assert_eq!(body.motion_type(), MotionType::Dynamic);
        assert_close(body.material().mass, 150.0);
        assert_close(body.material().friction, 2.0);
        assert_close(body.material().restitution, 0.5);
        assert_eq!(body.collider().half_extents, Vec3::new(5.0, 3.0, 5.0));
        assert_close(body.collider().center_offset.y, 2.28);
        assert_close(body.max_speed(), 40.0);
        assert_close(body.acceleration(), 60.0);
        assert_close(body.turn_rate_degrees(), 450.0);
    }

    #[test]
    fn recognizes_unit_keys_case_insensitively() {
        assert!(is_warthog_unit("UNSC_VEH_WARTHOG_01", None));
        assert!(is_warthog_unit("custom_hog", Some("Warthog")));
        assert!(!is_warthog_unit("unsc_inf_marine_01", None));
    }
}
