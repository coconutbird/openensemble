//! Persistent squad `ReflectDamage` profiles from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Immutable authored inputs for one retail squad `ReflectDamage` action.
#[derive(Debug, Clone, PartialEq)]
pub struct ReflectDamageProfile {
    action_name: String,
    work_rate: f32,
    starts_disabled: bool,
}

impl ReflectDamageProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Fraction of the incoming damage-event payload returned to the attacker.
    #[must_use]
    pub const fn work_rate(&self) -> f32 {
        self.work_rate
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Return the persistent squad `ReflectDamage` action for an object prototype.
    #[must_use]
    pub fn reflect_damage(&self, proto_object_name: &str) -> Option<&ReflectDamageProfile> {
        self.reflect_damage_actions
            .get(&proto_object_name.to_ascii_lowercase())
    }
}

pub(super) fn collect_reflect_damage(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, ReflectDamageProfile> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let action = rules.persistent_squad_actions.iter().find_map(|name| {
                gameplay.tactics.actions.iter().find(|action| {
                    action.name.eq_ignore_ascii_case(name) && is_reflect_damage(action)
                })
            })?;
            Some((key.clone(), profile_from_action(action)))
        })
        .collect()
}

fn is_reflect_damage(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ReflectDamage"))
}

fn profile_from_action(action: &Action) -> ReflectDamageProfile {
    ReflectDamageProfile {
        action_name: action.name.clone(),
        work_rate: action
            .work_rate
            .filter(|value| value.is_finite())
            .unwrap_or_default()
            .max(0.0),
        starts_disabled: action.start_disabled == Some(true),
    }
}

#[cfg(test)]
mod tests;
