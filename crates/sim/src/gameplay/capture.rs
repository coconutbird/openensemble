//! Unit `Capture` profiles resolved from scenario-layered tactics.

use super::{AttackQuery, GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

pub(crate) const DEFAULT_CAPTURE_WORK_RANGE: f32 = 0.1;
pub(crate) const DEFAULT_CAPTURE_WORK_RATE: f32 = 0.0;

/// Immutable authored inputs for one retail unit `Capture` action.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureActionProfile {
    action_name: String,
    work_rate: f32,
    work_range: f32,
    die_on_built: bool,
    starts_disabled: bool,
}

impl CaptureActionProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Base capture points contributed by one unit per second.
    #[must_use]
    pub const fn work_rate(&self) -> f32 {
        self.work_rate
    }

    /// Maximum obstruction-surface distance at which the unit can work.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Whether completion kills the unit performing this action.
    #[must_use]
    pub const fn die_on_built(&self) -> bool {
        self.die_on_built
    }

    /// Whether the action waits for player or live enablement.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Iterate the `Capture` actions authored for one proto object.
    pub fn capture_actions(&self, proto_object_name: &str) -> &[CaptureActionProfile] {
        self.capture_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Resolve one named `Capture` profile.
    #[must_use]
    pub fn capture_action(
        &self,
        proto_object_name: &str,
        action_name: &str,
    ) -> Option<&CaptureActionProfile> {
        self.capture_actions(proto_object_name)
            .iter()
            .find(|profile| profile.action_name.eq_ignore_ascii_case(action_name))
    }

    /// Apply authored target rules, falling back only for shipped rule-less tactics.
    pub(crate) fn select_capture_action<'catalog>(
        &'catalog self,
        proto_object_name: &str,
        query: &AttackQuery<'_>,
        mut action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<&'catalog CaptureActionProfile> {
        let object = self.object(proto_object_name)?;
        if object
            .tactics
            .tactic
            .as_ref()
            .is_some_and(|tactic| !tactic.target_rules.is_empty())
        {
            let action =
                self.select_work_action(proto_object_name, query, &mut action_is_enabled)?;
            if !is_capture(action) {
                return None;
            }
            return self.capture_action(proto_object_name, &action.name);
        }
        self.capture_actions(proto_object_name)
            .iter()
            .find(|profile| {
                object
                    .tactics
                    .actions
                    .iter()
                    .find(|action| action.name.eq_ignore_ascii_case(profile.action_name()))
                    .is_some_and(|action| {
                        object.action_available_in_tactic_state(query.tactic_state, action)
                            && action_is_enabled(action)
                    })
            })
    }
}

pub(super) fn collect_capture_actions(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<CaptureActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let profiles = gameplay
                .tactics
                .actions
                .iter()
                .filter(|action| is_capture(action))
                .map(profile_from_action)
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(action: &Action) -> CaptureActionProfile {
    CaptureActionProfile {
        action_name: action.name.clone(),
        work_rate: finite_nonnegative(action.work_rate, DEFAULT_CAPTURE_WORK_RATE),
        work_range: finite_nonnegative(action.work_range, DEFAULT_CAPTURE_WORK_RANGE),
        die_on_built: action.die_on_built == Some(true),
        starts_disabled: action.start_disabled == Some(true),
    }
}

fn finite_nonnegative(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

fn is_capture(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Capture"))
}

#[cfg(test)]
mod tests;
