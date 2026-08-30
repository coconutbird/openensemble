//! Persistent unit `AmbientLifeSpawner` profiles from scenario-layered data.

use crate::gameplay::{GameplayCatalog, ObjectGameplay};
use num_traits::ToPrimitive;
use pipeline::database::hw1::tactics::Action;
use pipeline::database::hw1::{Database, GameData};
use std::collections::BTreeMap;

/// Immutable authored inputs for retail's one-shot ambient-life spawner.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientLifeSpawnerProfile {
    action_name: String,
    squad_type: String,
    check_frequency_ms: u32,
    opportunity_check_radius: f32,
    starts_disabled: bool,
}

impl AmbientLifeSpawnerProfile {
    /// Authored action name used by technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Logical squad prototype created after the first opportunity appears.
    #[must_use]
    pub fn squad_type(&self) -> &str {
        &self.squad_type
    }

    /// Delay between square opportunity queries, in seconds.
    #[must_use]
    pub fn check_frequency(&self) -> f32 {
        self.check_frequency_ms.to_f32().unwrap_or(f32::MAX) / 1_000.0
    }

    /// Half-extent of retail's square opportunity query.
    #[must_use]
    pub const fn opportunity_check_radius(&self) -> f32 {
        self.opportunity_check_radius
    }

    /// Whether the persistent action begins disabled.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    pub(crate) const fn check_frequency_ms(&self) -> u32 {
        self.check_frequency_ms
    }
}

impl GameplayCatalog {
    /// Return the persistent `AmbientLifeSpawner` action for an object prototype.
    #[must_use]
    pub fn ambient_life_spawner(
        &self,
        proto_object_name: &str,
    ) -> Option<&AmbientLifeSpawnerProfile> {
        self.ambient_life_spawners
            .get(&proto_object_name.to_ascii_lowercase())
    }
}

pub(in crate::gameplay) fn collect_ambient_life_spawners(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, AmbientLifeSpawnerProfile> {
    let settings = database.game_data.as_ref();
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let action = gameplay.tactics.actions.iter().find(|action| {
                rules
                    .persistent_actions
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&action.name))
                    && is_ambient_life_spawner(action)
            })?;
            profile_from_action(action, settings).map(|profile| (key.clone(), profile))
        })
        .collect()
}

fn is_ambient_life_spawner(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("AmbientLifeSpawner"))
}

fn profile_from_action(
    action: &Action,
    settings: Option<&GameData>,
) -> Option<AmbientLifeSpawnerProfile> {
    let squad_type = action.squad_type.as_deref()?.trim();
    if squad_type.is_empty() {
        return None;
    }
    Some(AmbientLifeSpawnerProfile {
        action_name: action.name.clone(),
        squad_type: squad_type.to_owned(),
        check_frequency_ms: retail_milliseconds(
            settings.and_then(|data| data.al_spawner_check_frequency),
        ),
        opportunity_check_radius: super::nonnegative(
            settings.and_then(|data| data.al_opp_check_radius),
        ),
        starts_disabled: action.start_disabled == Some(true),
    })
}

fn retail_milliseconds(value: Option<f32>) -> u32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .and_then(|value| (value * 1_000.0).trunc().to_u32())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
