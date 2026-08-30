//! Squad `RepairOther` profiles resolved from scenario-layered tactics.

use super::{AttackQuery, GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

pub(crate) const DEFAULT_REPAIR_WORK_RATE: f32 = 0.0;
pub(crate) const DEFAULT_REPAIR_WORK_RANGE: f32 = 0.1;

/// Idle-opportunity settings authored on a retail `RepairOther` action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoRepairProfile {
    idle_time_ms: u32,
    threshold: f32,
    search_distance: f32,
}

impl AutoRepairProfile {
    /// Required duration of the squad's idle action.
    #[must_use]
    pub const fn idle_time_ms(self) -> u32 {
        self.idle_time_ms
    }

    /// Maximum target hitpoint fraction eligible for automatic repair.
    #[must_use]
    pub const fn threshold(self) -> f32 {
        self.threshold
    }

    /// Maximum center distance searched for an allied squad.
    #[must_use]
    pub const fn search_distance(self) -> f32 {
        self.search_distance
    }
}

/// Immutable authored inputs for one retail squad `RepairOther` action.
#[derive(Debug, Clone, PartialEq)]
pub struct RepairOtherActionProfile {
    action_name: String,
    work_rate: f32,
    work_range: f32,
    allow_reinforce: bool,
    starts_disabled: bool,
    auto_repair: Option<AutoRepairProfile>,
    effect_proto_object: Option<String>,
    effect_bone: Option<String>,
}

impl RepairOtherActionProfile {
    /// Authored action name used by target rules and technology modifiers.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Base squad combat value restored per second.
    #[must_use]
    pub const fn work_rate(&self) -> f32 {
        self.work_rate
    }

    /// Maximum obstruction-surface distance at which repair work occurs.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Authored argument retained even though retail repair ignores it.
    #[must_use]
    pub const fn allow_reinforce(&self) -> bool {
        self.allow_reinforce
    }

    /// Whether the action begins disabled until technology or live state enables it.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    /// Optional idle opportunity settings.
    #[must_use]
    pub const fn auto_repair(&self) -> Option<AutoRepairProfile> {
        self.auto_repair
    }

    /// Explicit beam/effect proto object. Absence selects retail's fallback effect.
    #[must_use]
    pub fn effect_proto_object(&self) -> Option<&str> {
        self.effect_proto_object.as_deref()
    }

    /// Source bone authored for the explicit effect.
    #[must_use]
    pub fn effect_bone(&self) -> Option<&str> {
        self.effect_bone.as_deref()
    }
}

impl GameplayCatalog {
    /// Iterate `RepairOther` actions authored for one proto object.
    pub fn repair_other_actions(&self, proto_object_name: &str) -> &[RepairOtherActionProfile] {
        self.repair_other_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Resolve one named `RepairOther` profile.
    #[must_use]
    pub fn repair_other_action(
        &self,
        proto_object_name: &str,
        action_name: &str,
    ) -> Option<&RepairOtherActionProfile> {
        self.repair_other_actions(proto_object_name)
            .iter()
            .find(|profile| profile.action_name.eq_ignore_ascii_case(action_name))
    }

    /// Apply authored target rules, falling back only for rule-less tactics.
    pub(crate) fn select_repair_other_action<'catalog>(
        &'catalog self,
        proto_object_name: &str,
        query: &AttackQuery<'_>,
        mut action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<&'catalog RepairOtherActionProfile> {
        let object = self.object(proto_object_name)?;
        if object
            .tactics
            .tactic
            .as_ref()
            .is_some_and(|tactic| !tactic.target_rules.is_empty())
        {
            let action =
                self.select_work_action(proto_object_name, query, &mut action_is_enabled)?;
            if !is_repair_other(action) {
                return None;
            }
            return self.repair_other_action(proto_object_name, &action.name);
        }
        self.repair_other_actions(proto_object_name)
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

pub(super) fn collect_repair_other_actions(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<RepairOtherActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let profiles = gameplay
                .tactics
                .actions
                .iter()
                .filter(|action| is_repair_other(action))
                .map(profile_from_action)
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(action: &Action) -> RepairOtherActionProfile {
    let effect = action.proto_object.as_ref();
    RepairOtherActionProfile {
        action_name: action.name.clone(),
        work_rate: finite_nonnegative(action.work_rate, DEFAULT_REPAIR_WORK_RATE),
        work_range: finite_nonnegative(action.work_range, DEFAULT_REPAIR_WORK_RANGE),
        allow_reinforce: action.allow_reinforce == Some(true),
        starts_disabled: action.start_disabled == Some(true),
        auto_repair: action.auto_repair.as_ref().map(|auto| AutoRepairProfile {
            idle_time_ms: auto.idle_time.unwrap_or_default(),
            threshold: finite_nonnegative(auto.threshold, 1.0),
            search_distance: finite_nonnegative(auto.search_distance, 0.0),
        }),
        effect_proto_object: effect
            .map(|reference| reference.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        effect_bone: effect
            .and_then(|reference| reference.bone.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
    }
}

fn finite_nonnegative(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

fn is_repair_other(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("RepairOther"))
}

#[cfg(test)]
mod tests;
