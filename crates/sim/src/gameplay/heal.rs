//! Persistent unit `Heal` profiles resolved from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use num_traits::ToPrimitive;
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Immutable authored inputs for one retail unit `Heal` action.
#[derive(Debug, Clone, PartialEq)]
pub struct HealActionProfile {
    action_name: String,
    work_rate: f32,
    min_idle_duration_ms: u32,
    allow_reinforce: bool,
    heal_target: bool,
    starts_disabled: bool,
}

impl HealActionProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Hit points restored per second.
    #[must_use]
    pub const fn work_rate(&self) -> f32 {
        self.work_rate
    }

    /// Required idle, post-damage, and post-attack duration in milliseconds.
    #[must_use]
    pub const fn min_idle_duration_ms(&self) -> u32 {
        self.min_idle_duration_ms
    }

    /// Whether missing authored squad members may be recreated.
    #[must_use]
    pub const fn allow_reinforce(&self) -> bool {
        self.allow_reinforce
    }

    /// Whether Join's action target replaces the healer's own parent squad.
    #[must_use]
    pub const fn heal_target(&self) -> bool {
        self.heal_target
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Iterate persistent unit `Heal` actions for one object prototype.
    pub fn heal_actions(&self, proto_object_name: &str) -> &[HealActionProfile] {
        self.heal_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Return the first connected persistent unit `Heal` action.
    #[must_use]
    pub fn heal(&self, proto_object_name: &str) -> Option<&HealActionProfile> {
        self.heal_actions(proto_object_name).first()
    }
}

pub(super) fn collect_heal_actions(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<HealActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let profiles =
                rules
                    .persistent_actions
                    .iter()
                    .filter_map(|name| {
                        gameplay.tactics.actions.iter().find(|action| {
                            action.name.eq_ignore_ascii_case(name) && is_heal(action)
                        })
                    })
                    .map(profile_from_action)
                    .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(action: &Action) -> HealActionProfile {
    HealActionProfile {
        action_name: action.name.clone(),
        work_rate: finite_nonnegative(action.work_rate),
        min_idle_duration_ms: seconds_to_milliseconds(action.min_idle_duration),
        allow_reinforce: action.allow_reinforce == Some(true),
        heal_target: action.heal_target == Some(true),
        starts_disabled: action.start_disabled == Some(true),
    }
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn seconds_to_milliseconds(value: Option<f32>) -> u32 {
    (finite_nonnegative(value) * 1_000.0)
        .to_u32()
        .unwrap_or(u32::MAX)
}

fn is_heal(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Heal"))
}

#[cfg(test)]
mod tests;
