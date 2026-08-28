//! Stock single-vehicle Warthog squad profile.

/// Shipped proto-squad name.
pub const WARTHOG_SQUAD_NAME: &str = "unsc_veh_warthog_01";
/// Shipped proto-squad database ID.
pub const WARTHOG_PROTO_SQUAD_ID: i32 = 885;

/// Squad-level pathing configuration from `squads.xml`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarthogSquadSpec {
    /// Nominal turn-radius value stored in the element body.
    pub turn_radius: f32,
    /// Minimum pathing turn radius.
    pub min_turn_radius: f32,
    /// Maximum pathing turn radius.
    pub max_turn_radius: f32,
}

impl Default for WarthogSquadSpec {
    fn default() -> Self {
        Self {
            turn_radius: 0.0,
            min_turn_radius: 1.5,
            max_turn_radius: 4.0,
        }
    }
}

/// Check whether a proto-squad name selects the stock Warthog squad.
#[must_use]
pub fn is_warthog_squad(proto_name: &str) -> bool {
    proto_name.eq_ignore_ascii_case(WARTHOG_SQUAD_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn stock_profile_matches_shipped_data() {
        let spec = WarthogSquadSpec::default();
        assert_close(spec.turn_radius, 0.0);
        assert_close(spec.min_turn_radius, 1.5);
        assert_close(spec.max_turn_radius, 4.0);
        assert_eq!(WARTHOG_PROTO_SQUAD_ID, 885);
    }

    #[test]
    fn recognizes_squad_key_case_insensitively() {
        assert!(is_warthog_squad("UNSC_VEH_WARTHOG_01"));
        assert!(!is_warthog_squad("unsc_inf_marine_01"));
    }
}
