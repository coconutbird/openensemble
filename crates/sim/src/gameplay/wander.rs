//! Persistent squad `Wander` profiles from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

const DEFAULT_WORK_RANGE: f32 = 0.1;

/// Immutable authored inputs for retail's persistent squad wander action.
#[derive(Debug, Clone, PartialEq)]
pub struct WanderProfile {
    action_name: String,
    work_range: f32,
    starts_disabled: bool,
}

impl WanderProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Radius around the connection-time squad origin used for random targets.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Return the persistent squad `Wander` action for an object prototype.
    #[must_use]
    pub fn wander(&self, proto_object_name: &str) -> Option<&WanderProfile> {
        self.wander_actions
            .get(&proto_object_name.to_ascii_lowercase())
    }
}

pub(super) fn collect_wanders(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, WanderProfile> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let action = rules.persistent_squad_actions.iter().find_map(|name| {
                gameplay
                    .tactics
                    .actions
                    .iter()
                    .find(|action| action.name.eq_ignore_ascii_case(name) && is_wander(action))
            })?;
            Some((key.clone(), profile_from_action(action)))
        })
        .collect()
}

fn is_wander(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Wander"))
}

fn profile_from_action(action: &Action) -> WanderProfile {
    WanderProfile {
        action_name: action.name.clone(),
        work_range: action
            .work_range
            .filter(|range| range.is_finite() && *range >= 0.0)
            .unwrap_or(DEFAULT_WORK_RANGE),
        starts_disabled: action.start_disabled == Some(true),
    }
}

#[cfg(test)]
mod tests;
