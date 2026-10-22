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

/// Return whether an authored action uses the independently scanning turret
/// executor, including retail's persistent-action executor override.
pub(super) fn uses_secondary_turret_executor(action: &Action) -> bool {
    [
        action.action_type.as_deref(),
        action.persistent_action_type.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|kind| kind.eq_ignore_ascii_case("SecondaryTurretAttack"))
}

/// Return whether an authored action is driven by a parent ranged attack.
pub(super) fn uses_slave_turret_executor(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("SlaveTurretAttack"))
}

/// Return whether an action's animation tags author simulation attack events.
pub(super) fn uses_attack_timing(action: &Action) -> bool {
    uses_ranged_attack_executor(action)
        || uses_secondary_turret_executor(action)
        || uses_slave_turret_executor(action)
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

    #[test]
    fn turret_actions_have_distinct_executors_but_share_attack_tag_timing() {
        let secondary = action("SecondaryTurretAttack");
        let slave = action("SlaveTurretAttack");

        assert!(uses_secondary_turret_executor(&secondary));
        assert!(!uses_slave_turret_executor(&secondary));
        assert!(uses_slave_turret_executor(&slave));
        assert!(!uses_ranged_attack_executor(&slave));
        assert!(uses_attack_timing(&secondary));
        assert!(uses_attack_timing(&slave));

        let mut persistent_override = action("RangedAttack");
        persistent_override.persistent_action_type = Some("SecondaryTurretAttack".to_owned());
        assert!(uses_secondary_turret_executor(&persistent_override));
    }
}
