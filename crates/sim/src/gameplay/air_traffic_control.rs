//! Persistent air-traffic-control profiles resolved from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Immutable authored inputs for one persistent `AirTrafficControl` action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirTrafficControlActionProfile {
    action_name: String,
    starts_disabled: bool,
}

impl AirTrafficControlActionProfile {
    /// Authored action name used by technology and live enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Whether the action waits for technology or a live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Iterate persistent air-traffic-control actions for one object prototype.
    pub fn air_traffic_control_actions(
        &self,
        proto_object_name: &str,
    ) -> &[AirTrafficControlActionProfile] {
        self.air_traffic_control_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }
}

pub(super) fn collect_air_traffic_control_actions(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<AirTrafficControlActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let profiles = rules
                .persistent_actions
                .iter()
                .filter_map(|name| {
                    gameplay.tactics.actions.iter().find(|action| {
                        action.name.eq_ignore_ascii_case(name) && is_air_traffic_control(action)
                    })
                })
                .map(profile_from_action)
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(action: &Action) -> AirTrafficControlActionProfile {
    AirTrafficControlActionProfile {
        action_name: action.name.clone(),
        starts_disabled: action.start_disabled == Some(true),
    }
}

fn is_air_traffic_control(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("AirTrafficControl"))
}

#[cfg(test)]
mod tests;
