//! Persistent squad `AmbientLife` profiles from scenario-layered data.

use super::{GameplayCatalog, ObjectGameplay};
use num_traits::ToPrimitive;
use pipeline::database::hw1::tactics::Action;
use pipeline::database::hw1::{Database, GameData};
use std::collections::BTreeMap;

mod spawner;

pub use spawner::AmbientLifeSpawnerProfile;
pub(super) use spawner::collect_ambient_life_spawners;

/// Immutable authored inputs for retail's persistent ambient-life controller.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientLifeProfile {
    action_name: String,
    max_wander_frequency_ms: u32,
    predator_check_frequency_ms: u32,
    prey_check_frequency_ms: u32,
    opportunity_check_radius: f32,
    flee_distance: f32,
    flee_movement_modifier: f32,
    minimum_wander_distance: f32,
    maximum_wander_distance: f32,
    starts_disabled: bool,
}

impl AmbientLifeProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Inclusive maximum delay between wander attempts, in seconds.
    #[must_use]
    pub fn max_wander_frequency(&self) -> f32 {
        milliseconds_to_seconds(self.max_wander_frequency_ms)
    }

    /// Interval between predator scans, in seconds.
    #[must_use]
    pub fn predator_check_frequency(&self) -> f32 {
        milliseconds_to_seconds(self.predator_check_frequency_ms)
    }

    /// Interval between prey selections while hunting, in seconds.
    #[must_use]
    pub fn prey_check_frequency(&self) -> f32 {
        milliseconds_to_seconds(self.prey_check_frequency_ms)
    }

    /// Half-extent of retail's square opportunity query.
    #[must_use]
    pub const fn opportunity_check_radius(&self) -> f32 {
        self.opportunity_check_radius
    }

    /// Maximum random distance used for an ordinary flee move.
    #[must_use]
    pub const fn flee_distance(&self) -> f32 {
        self.flee_distance
    }

    /// Movement-speed multiplier applied while a flee child action is active.
    #[must_use]
    pub const fn flee_movement_modifier(&self) -> f32 {
        self.flee_movement_modifier
    }

    /// Inner radius of random ambient wander destinations.
    #[must_use]
    pub const fn minimum_wander_distance(&self) -> f32 {
        self.minimum_wander_distance
    }

    /// Outer radius of random ambient wander destinations.
    #[must_use]
    pub const fn maximum_wander_distance(&self) -> f32 {
        self.maximum_wander_distance
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    pub(crate) const fn max_wander_frequency_ms(&self) -> u32 {
        self.max_wander_frequency_ms
    }

    pub(crate) const fn predator_check_frequency_ms(&self) -> u32 {
        self.predator_check_frequency_ms
    }

    pub(crate) const fn prey_check_frequency_ms(&self) -> u32 {
        self.prey_check_frequency_ms
    }
}

impl GameplayCatalog {
    /// Return the persistent squad `AmbientLife` action for an object prototype.
    #[must_use]
    pub fn ambient_life(&self, proto_object_name: &str) -> Option<&AmbientLifeProfile> {
        self.ambient_life_actions
            .get(&proto_object_name.to_ascii_lowercase())
    }
}

pub(super) fn collect_ambient_life(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, AmbientLifeProfile> {
    let settings = database.game_data.as_ref();
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let action = rules.persistent_squad_actions.iter().find_map(|name| {
                gameplay.tactics.actions.iter().find(|action| {
                    action.name.eq_ignore_ascii_case(name) && is_ambient_life(action)
                })
            })?;
            Some((key.clone(), profile_from_action(action, settings)))
        })
        .collect()
}

fn is_ambient_life(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("AmbientLife"))
}

fn profile_from_action(action: &Action, settings: Option<&GameData>) -> AmbientLifeProfile {
    AmbientLifeProfile {
        action_name: action.name.clone(),
        max_wander_frequency_ms: retail_whole_seconds(
            settings.and_then(|data| data.al_max_wander_frequency),
        ),
        predator_check_frequency_ms: retail_whole_seconds(
            settings.and_then(|data| data.al_predator_check_frequency),
        ),
        prey_check_frequency_ms: retail_whole_seconds(
            settings.and_then(|data| data.al_prey_check_frequency),
        ),
        opportunity_check_radius: nonnegative(settings.and_then(|data| data.al_opp_check_radius)),
        flee_distance: nonnegative(settings.and_then(|data| data.al_flee_distance)),
        flee_movement_modifier: nonnegative(
            settings.and_then(|data| data.al_flee_movement_modifier),
        ),
        minimum_wander_distance: nonnegative(settings.and_then(|data| data.al_min_wander_distance)),
        maximum_wander_distance: nonnegative(settings.and_then(|data| data.al_max_wander_distance)),
        starts_disabled: action.start_disabled == Some(true),
    }
}

fn retail_whole_seconds(value: Option<f32>) -> u32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .and_then(|value| value.trunc().to_u32())
        .unwrap_or_default()
        .saturating_mul(1_000)
}

fn nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn milliseconds_to_seconds(milliseconds: u32) -> f32 {
    milliseconds.to_f32().unwrap_or(f32::MAX) / 1_000.0
}

#[cfg(test)]
mod tests;
