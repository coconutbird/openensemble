//! Stock four-member UNSC Marine squad profile.

use glam::Vec3;

/// Shipped proto-squad name.
pub const MARINE_SQUAD_NAME: &str = "unsc_inf_marine_01";
/// Shipped proto-squad database ID.
pub const MARINE_PROTO_SQUAD_ID: i32 = 877;

/// Squad-level values from the stock Marine record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarineSquadSpec {
    /// Number of `unsc_inf_marine_01` members in the stock squad.
    pub member_count: usize,
    /// AI leash distance.
    pub leash_distance: f32,
    /// AI aggression distance.
    pub aggro_distance: f32,
    /// Leash deadzone distance.
    pub leash_deadzone: f32,
    /// Raw leash recall delay stored by the game data.
    pub leash_recall_delay_ms: u32,
    /// Deterministic separation used to seed the runtime `Flock` formation.
    pub initial_member_separation: f32,
}

impl Default for MarineSquadSpec {
    fn default() -> Self {
        Self {
            member_count: 4,
            leash_distance: 55.0,
            aggro_distance: 35.0,
            leash_deadzone: 25.0,
            leash_recall_delay_ms: 2_500,
            // Marine obstruction diameter is 2.0; the extra half unit keeps
            // the initial positions distinct before flock steering begins.
            initial_member_separation: 2.5,
        }
    }
}

impl MarineSquadSpec {
    /// Return a centered, deterministic seed offset for one of four members.
    ///
    /// Vanilla labels the squad `Flock` and subsequently tracks a velocity and
    /// transform per member. The shipped data does not prescribe initial random
    /// offsets, so the deterministic sim begins from a centered 2x2 seed.
    #[must_use]
    pub fn initial_formation_offset(self, slot: usize) -> Option<Vec3> {
        if slot >= self.member_count {
            return None;
        }
        let half = self.initial_member_separation * 0.5;
        let x = if slot.is_multiple_of(2) { -half } else { half };
        let z = if slot < 2 { -half } else { half };
        Some(Vec3::new(x, 0.0, z))
    }
}

/// Check whether a proto-squad name selects the stock Marine squad.
#[must_use]
pub fn is_marine_squad(proto_name: &str) -> bool {
    proto_name.eq_ignore_ascii_case(MARINE_SQUAD_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn stock_profile_matches_shipped_data() {
        let spec = MarineSquadSpec::default();
        assert_eq!(MARINE_PROTO_SQUAD_ID, 877);
        assert_eq!(spec.member_count, 4);
        assert_close(spec.leash_distance, 55.0);
        assert_close(spec.aggro_distance, 35.0);
        assert_close(spec.leash_deadzone, 25.0);
        assert_eq!(spec.leash_recall_delay_ms, 2_500);
    }

    #[test]
    fn initial_flock_seed_is_centered_and_distinct() {
        let spec = MarineSquadSpec::default();
        let offsets = (0..spec.member_count)
            .map(|slot| spec.initial_formation_offset(slot).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(offsets.iter().copied().sum::<Vec3>(), Vec3::ZERO);
        for (index, offset) in offsets.iter().enumerate() {
            assert!(!offsets[..index].contains(offset));
        }
        assert_eq!(spec.initial_formation_offset(spec.member_count), None);
    }

    #[test]
    fn recognizes_squad_key_case_insensitively() {
        assert!(is_marine_squad("UNSC_INF_MARINE_01"));
        assert!(!is_marine_squad("unsc_veh_warthog_01"));
    }
}
