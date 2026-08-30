//! Persistent unit `Bomb` profiles resolved from scenario-layered tactics and physics.

use super::{GameplayCatalog, ObjectGameplay, PhysicsReplacementProfile, vehicle_physics};
use pipeline::database::hw1::tactics::Action;
use pipeline::database::hw1::{Database, ProtoObject};
use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::BTreeMap;

const DEFAULT_ROLL_CHANCE: f32 = 0.1;

/// Immutable authored inputs for one retail persistent `Bomb` action.
#[derive(Debug, Clone, PartialEq)]
pub struct BombActionProfile {
    action_name: String,
    roll_chance: f32,
    starts_disabled: bool,
    physics_info: Option<String>,
    physics_body: Option<PhysicsReplacementProfile>,
    physics_load_issue: Option<String>,
    release_physics_on_completion: bool,
}

impl BombActionProfile {
    /// Return the authored action name used by technology and live enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Return the `WorkRange` threshold compared with retail's `[0, 1]` roll.
    #[must_use]
    pub const fn roll_chance(&self) -> f32 {
        self.roll_chance
    }

    /// Return whether the persistent action starts disabled.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    /// Return the `PhysicsInfo` or fallback `PhysicsReplacementInfo` reference.
    #[must_use]
    pub fn physics_info(&self) -> Option<&str> {
        self.physics_info.as_deref()
    }

    /// Return the body resolved through the active scenario asset stack.
    #[must_use]
    pub const fn physics_body(&self) -> Option<&PhysicsReplacementProfile> {
        self.physics_body.as_ref()
    }

    /// Return why the referenced physics chain could not be loaded.
    #[must_use]
    pub fn physics_load_issue(&self) -> Option<&str> {
        self.physics_load_issue.as_deref()
    }

    pub(crate) const fn release_physics_on_completion(&self) -> bool {
        self.release_physics_on_completion
    }
}

impl GameplayCatalog {
    /// Iterate persistent unit `Bomb` actions for one object prototype.
    pub fn bomb_actions(&self, proto_object_name: &str) -> &[BombActionProfile] {
        self.bomb_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Return the first persistent `Bomb` action in authored order.
    #[must_use]
    pub fn bomb(&self, proto_object_name: &str) -> Option<&BombActionProfile> {
        self.bomb_actions(proto_object_name).first()
    }
}

pub(super) fn collect_bomb_actions(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<BombActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let prototype = database
                .objects
                .iter()
                .find(|object| object.name.eq_ignore_ascii_case(key));
            let profiles =
                rules
                    .persistent_actions
                    .iter()
                    .filter_map(|name| {
                        gameplay.tactics.actions.iter().find(|action| {
                            action.name.eq_ignore_ascii_case(name) && is_bomb(action)
                        })
                    })
                    .map(|action| profile_from_action(action, prototype))
                    .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

pub(super) fn load_bomb_physics(
    profiles: &mut BTreeMap<String, Vec<BombActionProfile>>,
    source: &mut AssetSource<StdFileProvider>,
) {
    let mut cache = BTreeMap::<String, Result<PhysicsReplacementProfile, String>>::new();
    for actions in profiles.values_mut() {
        for profile in actions {
            let Some(reference) = profile.physics_info.as_deref() else {
                continue;
            };
            let key = reference.to_ascii_lowercase();
            if !cache.contains_key(&key) {
                cache.insert(
                    key.clone(),
                    vehicle_physics::load_dynamic_body_profile(reference, source),
                );
            }
            match cache.get(&key).expect("bomb physics cache entry") {
                Ok(body) => profile.physics_body = Some(body.clone()),
                Err(reason) => profile.physics_load_issue = Some(reason.clone()),
            }
        }
    }
}

fn profile_from_action(action: &Action, prototype: Option<&ProtoObject>) -> BombActionProfile {
    let (physics_info, release_physics_on_completion) = physics_reference(prototype);
    BombActionProfile {
        action_name: action.name.clone(),
        roll_chance: action
            .work_range
            .filter(|chance| chance.is_finite())
            .unwrap_or(DEFAULT_ROLL_CHANCE),
        starts_disabled: action.start_disabled == Some(true),
        physics_info,
        physics_body: None,
        physics_load_issue: None,
        release_physics_on_completion,
    }
}

fn physics_reference(prototype: Option<&ProtoObject>) -> (Option<String>, bool) {
    let Some(prototype) = prototype else {
        return (None, false);
    };
    if let Some(reference) = nonempty_reference(prototype.physics_info.as_deref()) {
        return (Some(reference.to_owned()), false);
    }
    let replacement = nonempty_reference(prototype.physics_replacement_info.as_deref());
    (replacement.map(str::to_owned), replacement.is_some())
}

fn nonempty_reference(reference: Option<&str>) -> Option<&str> {
    reference
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
}

fn is_bomb(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Bomb"))
}

#[cfg(test)]
mod tests;
