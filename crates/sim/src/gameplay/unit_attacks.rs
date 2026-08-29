//! Retail tactic action-type mapping for the shared unit attack executor.

use pipeline::database::hw1::tactics::Action;

/// Return whether an authored action maps to `BUnitActionRangedAttack`.
///
/// Retail maps both `RangedAttack` and `HandAttack` to that executor. The
/// latter only records additional melee-targeting restrictions; projectile
/// presence selects the projectile versus instant-hit damage path.
pub(super) fn uses_ranged_attack_executor(action: &Action) -> bool {
    action.action_type.as_deref().is_some_and(|kind| {
        kind.eq_ignore_ascii_case("RangedAttack") || kind.eq_ignore_ascii_case("HandAttack")
    })
}

pub(super) fn is_hand_attack(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("HandAttack"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(kind: &str) -> Action {
        Action {
            action_type: Some(kind.to_owned()),
            ..Action::default()
        }
    }

    #[test]
    fn retail_maps_ranged_and_hand_attacks_to_one_executor() {
        assert!(uses_ranged_attack_executor(&action("RangedAttack")));
        assert!(uses_ranged_attack_executor(&action("handattack")));
        assert!(!uses_ranged_attack_executor(&action("CollisionAttack")));
        assert!(!is_hand_attack(&action("RangedAttack")));
        assert!(is_hand_attack(&action("HandAttack")));
    }
}
