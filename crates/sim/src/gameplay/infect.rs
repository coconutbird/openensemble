//! Persistent unit `Infect` profiles resolved from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use num_traits::ToPrimitive;
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Immutable authored inputs for one retail unit `Infect` action.
#[derive(Debug, Clone, PartialEq)]
pub struct InfectActionProfile {
    action_name: String,
    work_rate: f32,
    work_range: f32,
    min_idle_duration_ms: u32,
    attachment_proto_object: Option<String>,
    invalid_targets: Vec<String>,
    starts_disabled: bool,
}

impl InfectActionProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Combat value accumulated per second after the exposure delay.
    #[must_use]
    pub const fn work_rate(&self) -> f32 {
        self.work_rate
    }

    /// Radius used by the retail half-second squad scan.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Exposure delay before combat-value work begins.
    #[must_use]
    pub const fn min_idle_duration_ms(&self) -> u32 {
        self.min_idle_duration_ms
    }

    /// Visual proto-object attached to every child of an exposed squad.
    #[must_use]
    pub fn attachment_proto_object(&self) -> Option<&str> {
        self.attachment_proto_object.as_deref()
    }

    /// Action-level object types rejected while discovering infectees.
    #[must_use]
    pub fn invalid_targets(&self) -> &[String] {
        &self.invalid_targets
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Iterate persistent unit `Infect` actions for one object prototype.
    pub fn infect_actions(&self, proto_object_name: &str) -> &[InfectActionProfile] {
        self.infect_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Return the first connected persistent unit `Infect` action.
    #[must_use]
    pub fn infect(&self, proto_object_name: &str) -> Option<&InfectActionProfile> {
        self.infect_actions(proto_object_name).first()
    }
}

pub(super) fn collect_infect_actions(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<InfectActionProfile>> {
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
                            action.name.eq_ignore_ascii_case(name) && is_infect(action)
                        })
                    })
                    .map(profile_from_action)
                    .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(action: &Action) -> InfectActionProfile {
    InfectActionProfile {
        action_name: action.name.clone(),
        work_rate: finite_nonnegative(action.work_rate),
        work_range: finite_nonnegative(action.work_range),
        min_idle_duration_ms: seconds_to_milliseconds(action.min_idle_duration),
        attachment_proto_object: action
            .proto_object
            .as_ref()
            .map(|prototype| prototype.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        invalid_targets: action
            .invalid_targets
            .iter()
            .map(|target| target.trim())
            .filter(|target| !target.is_empty())
            .map(str::to_owned)
            .collect(),
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

fn is_infect(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Infect"))
}

#[cfg(test)]
mod tests;
