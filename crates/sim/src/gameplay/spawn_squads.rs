//! Persistent `SpawnSquad` actions resolved from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use std::collections::{BTreeMap, BTreeSet};

/// Immutable authored inputs for one persistent retail `SpawnSquad` action.
#[derive(Debug, Clone, PartialEq)]
pub struct PersistentSpawnSquadProfile {
    action_name: String,
    squad_type: String,
    animation: Option<String>,
    work_rate: f32,
    work_rate_variance: f32,
    count: u32,
    flags: SpawnFlags,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SpawnFlags(u8);

impl SpawnFlags {
    const STARTS_DISABLED: Self = Self(1 << 0);
    const STATIONARY: Self = Self(1 << 1);
    const AUTO_JOIN: Self = Self(1 << 2);
    const HIDE_UNTIL_RELEASE: Self = Self(1 << 3);

    fn from_action(action: &Action) -> Self {
        let mut flags = Self::default();
        flags.set(Self::STARTS_DISABLED, action.start_disabled == Some(true));
        flags.set(Self::STATIONARY, action.stationary == Some(true));
        flags.set(Self::AUTO_JOIN, action.auto_join == Some(true));
        flags.set(
            Self::HIDE_UNTIL_RELEASE,
            action.hide_spawn_until_release == Some(true),
        );
        flags
    }

    const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 != 0
    }

    fn set(&mut self, flag: Self, enabled: bool) {
        if enabled {
            self.0 |= flag.0;
        }
    }
}

impl PersistentSpawnSquadProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Logical squad prototype created by the action.
    #[must_use]
    pub fn squad_type(&self) -> &str {
        &self.squad_type
    }

    /// Optional blocking spawn animation requested by retail.
    #[must_use]
    pub fn animation(&self) -> Option<&str> {
        self.animation.as_deref()
    }

    /// Fixed cycle time override in seconds; zero uses squad build points.
    #[must_use]
    pub const fn work_rate(&self) -> f32 {
        self.work_rate
    }

    /// Full width of the synchronized per-cycle work-rate variance.
    #[must_use]
    pub const fn work_rate_variance(&self) -> f32 {
        self.work_rate_variance
    }

    /// Maximum spawned count; zero means unbounded.
    #[must_use]
    pub const fn count(&self) -> u32 {
        self.count
    }

    /// Whether the action begins disabled before live overrides.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.flags.contains(SpawnFlags::STARTS_DISABLED)
    }

    /// Whether the squad is created directly on the owner.
    #[must_use]
    pub const fn stationary(&self) -> bool {
        self.flags.contains(SpawnFlags::STATIONARY)
    }

    /// Whether the new squad receives a multiple-permitted Join order.
    #[must_use]
    pub const fn auto_join(&self) -> bool {
        self.flags.contains(SpawnFlags::AUTO_JOIN)
    }

    /// Whether retail hides the pre-spawned leader until the animation tag.
    #[must_use]
    pub const fn hide_until_release(&self) -> bool {
        self.flags.contains(SpawnFlags::HIDE_UNTIL_RELEASE)
    }
}

impl GameplayCatalog {
    /// Return persistent squad-spawn actions in authored tactic order.
    #[must_use]
    pub fn persistent_squad_spawns(
        &self,
        proto_object_name: &str,
    ) -> &[PersistentSpawnSquadProfile] {
        self.persistent_squad_spawns
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }
}

pub(super) fn collect_persistent_spawns(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<PersistentSpawnSquadProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let persistent_names = gameplay
                .tactics
                .tactic
                .as_ref()
                .into_iter()
                .flat_map(|rules| &rules.persistent_actions)
                .map(|name| name.to_ascii_lowercase())
                .collect::<BTreeSet<_>>();
            let profiles = gameplay
                .tactics
                .actions
                .iter()
                .filter(|action| persistent_spawn(action, &persistent_names))
                .filter_map(profile_from_action)
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn persistent_spawn(action: &Action, names: &BTreeSet<String>) -> bool {
    let persistent = action.persistent_action_type.is_some()
        || names.contains(&action.name.to_ascii_lowercase());
    persistent
        && action
            .persistent_action_type
            .as_deref()
            .or(action.action_type.as_deref())
            .is_some_and(|kind| kind.eq_ignore_ascii_case("SpawnSquad"))
}

fn profile_from_action(action: &Action) -> Option<PersistentSpawnSquadProfile> {
    let squad_type = action.squad_type.as_deref()?.trim();
    if squad_type.is_empty() {
        return None;
    }
    Some(PersistentSpawnSquadProfile {
        action_name: action.name.clone(),
        squad_type: squad_type.to_owned(),
        animation: action
            .anim
            .as_ref()
            .map(|animation| animation.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        work_rate: finite_nonnegative(action.work_rate),
        work_rate_variance: finite_nonnegative(action.work_rate_variance),
        count: action.count.unwrap_or_default().max(0).cast_unsigned(),
        flags: SpawnFlags::from_action(action),
    })
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite())
        .unwrap_or_default()
        .max(0.0)
}

#[cfg(test)]
mod tests;
