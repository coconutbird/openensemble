//! Unit `Gather` profiles resolved from scenario-layered tactics and game data.

use super::{GameplayCatalog, ObjectGameplay};
use crate::player::MAX_RESOURCES;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

const DEFAULT_WORK_RANGE: f32 = 0.1;
const DEFAULT_WORK_RATE: f32 = 1.0;

/// Immutable authored inputs for one retail unit `Gather` action.
#[derive(Debug, Clone, PartialEq)]
pub struct GatherActionProfile {
    action_name: String,
    resource_name: String,
    resource_id: usize,
    work_rate: f32,
    work_range: f32,
    team_share: bool,
    starts_disabled: bool,
}

impl GatherActionProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Scenario-layered game-data resource name granted by this action.
    #[must_use]
    pub fn resource_name(&self) -> &str {
        &self.resource_name
    }

    /// Runtime slot in `GameData/Resources`.
    #[must_use]
    pub const fn resource_id(&self) -> usize {
        self.resource_id
    }

    /// Base amount gathered by one unit per second.
    #[must_use]
    pub const fn work_rate(&self) -> f32 {
        self.work_rate
    }

    /// Maximum surface distance at which the unit can work.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Whether income is divided among playing members of the owner's team.
    #[must_use]
    pub const fn team_share(&self) -> bool {
        self.team_share
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Iterate the `Gather` actions authored for one proto object in tactic order.
    pub fn gather_actions(&self, proto_object_name: &str) -> &[GatherActionProfile] {
        self.gather_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Resolve the first gather action that grants the target resource.
    #[must_use]
    pub fn gather_action(
        &self,
        proto_object_name: &str,
        resource_name: &str,
    ) -> Option<&GatherActionProfile> {
        self.gather_actions(proto_object_name)
            .iter()
            .find(|profile| profile.resource_name.eq_ignore_ascii_case(resource_name))
    }

    pub(crate) fn gather_action_named(
        &self,
        proto_object_name: &str,
        action_name: &str,
    ) -> Option<&GatherActionProfile> {
        self.gather_actions(proto_object_name)
            .iter()
            .find(|profile| profile.action_name.eq_ignore_ascii_case(action_name))
    }
}

pub(super) fn collect_gather_actions(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<GatherActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let profiles = gameplay
                .tactics
                .actions
                .iter()
                .filter(|action| is_gather(action))
                .filter_map(|action| profile_from_action(database, action))
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(database: &Database, action: &Action) -> Option<GatherActionProfile> {
    let requested_resource = action
        .resource
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or_else(|| conventional_resource_name(&action.name))?;
    let (resource_id, resource_name) = resolve_resource(database, requested_resource)?;
    Some(GatherActionProfile {
        action_name: action.name.clone(),
        resource_name: resource_name.to_owned(),
        resource_id,
        work_rate: action
            .work_rate
            .filter(|rate| rate.is_finite() && *rate >= 0.0)
            .unwrap_or(DEFAULT_WORK_RATE),
        work_range: action
            .work_range
            .filter(|range| range.is_finite() && *range >= 0.0)
            .unwrap_or(DEFAULT_WORK_RANGE),
        team_share: action.team_share == Some(true),
        starts_disabled: action.start_disabled == Some(true),
    })
}

fn resolve_resource<'a>(database: &'a Database, requested: &str) -> Option<(usize, &'a str)> {
    database
        .game_data
        .as_ref()?
        .resources
        .as_ref()?
        .entries
        .iter()
        .take(MAX_RESOURCES)
        .enumerate()
        .find(|(_, resource)| resource.name.trim().eq_ignore_ascii_case(requested))
        .map(|(id, resource)| (id, resource.name.trim()))
}

fn conventional_resource_name(action_name: &str) -> Option<&'static str> {
    let action_name = action_name.trim();
    if action_name.eq_ignore_ascii_case("GatherSupplies") {
        Some("Supplies")
    } else if action_name.eq_ignore_ascii_case("GatherCollectables") {
        Some("Collectable")
    } else if action_name.eq_ignore_ascii_case("CampaignAction") {
        Some("CampaignFoo")
    } else {
        None
    }
}

fn is_gather(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Gather"))
}

#[cfg(test)]
mod tests;
