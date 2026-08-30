//! Voluntary squad Jump profiles compiled from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use crate::order::JumpOrderType;
use pipeline::database::hw1::tactics::{Action, TacticData};
use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeMap;

const DEFAULT_VELOCITY_SCALAR: f32 = 1.0;

/// Immutable authored inputs used by a retail squad/unit Jump action pair.
#[derive(Debug, Clone, PartialEq)]
pub struct JumpActionProfile {
    action_name: String,
    kind: JumpOrderType,
    max_distance: f32,
    velocity_scalar: f32,
    weapon_name: Option<String>,
    weapon_max_range: f32,
    animation_type: Option<String>,
    starts_disabled: bool,
    ability_starts_disabled: bool,
}

impl JumpActionProfile {
    /// Return the authored action name used by player technology effects.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Return the Jump order subtype implemented by this action.
    #[must_use]
    pub const fn kind(&self) -> JumpOrderType {
        self.kind
    }

    /// Return the maximum jump distance stored in retail's action duration field.
    #[must_use]
    pub const fn max_distance(&self) -> f32 {
        self.max_distance
    }

    /// Return spline travel speed in horizontal world units per second.
    #[must_use]
    pub const fn velocity_scalar(&self) -> f32 {
        self.velocity_scalar
    }

    /// Return the weapon referenced by this action, when one resolves.
    #[must_use]
    pub fn weapon_name(&self) -> Option<&str> {
        self.weapon_name.as_deref()
    }

    /// Return the referenced weapon's authored maximum range.
    #[must_use]
    pub const fn weapon_max_range(&self) -> f32 {
        self.weapon_max_range
    }

    /// Return the optional unit work-animation type.
    #[must_use]
    pub fn animation_type(&self) -> Option<&str> {
        self.animation_type.as_deref()
    }

    /// Return whether the tactic action starts disabled.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    /// Return the prototype's authored `AbilityDisabled` flag.
    #[must_use]
    pub const fn ability_starts_disabled(&self) -> bool {
        self.ability_starts_disabled
    }
}

impl GameplayCatalog {
    /// Iterate voluntary Jump actions for one effective object prototype.
    pub fn jump_actions(&self, proto_object_name: &str) -> &[JumpActionProfile] {
        self.jump_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }
}

pub(super) fn collect_jump_actions(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<JumpActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let prototype = find_object(database, &gameplay.proto_object_name);
            let ability_starts_disabled =
                prototype.is_some_and(|prototype| has_flag(prototype, "AbilityDisabled"));
            let profiles = gameplay
                .tactics
                .actions
                .iter()
                .filter_map(|action| {
                    profile_from_action(&gameplay.tactics, action, ability_starts_disabled)
                })
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(
    tactics: &TacticData,
    action: &Action,
    ability_starts_disabled: bool,
) -> Option<JumpActionProfile> {
    let kind = jump_kind(action.action_type.as_deref()?)?;
    let weapon_name = trimmed(action.weapon.as_deref());
    let weapon_max_range = weapon_name
        .as_deref()
        .and_then(|requested| {
            tactics
                .weapons
                .iter()
                .find(|weapon| weapon.name.eq_ignore_ascii_case(requested))
        })
        .and_then(|weapon| weapon.max_range)
        .filter(|range| range.is_finite())
        .unwrap_or_default();
    Some(JumpActionProfile {
        action_name: action.name.clone(),
        kind,
        max_distance: action
            .duration
            .as_ref()
            .map(|duration| duration.seconds)
            .filter(|distance| distance.is_finite())
            .unwrap_or_default(),
        velocity_scalar: action
            .velocity_scalar
            .filter(|velocity| velocity.is_finite())
            .unwrap_or(DEFAULT_VELOCITY_SCALAR),
        weapon_name,
        weapon_max_range,
        animation_type: trimmed(
            action
                .anim
                .as_ref()
                .map(|animation| animation.name.as_str()),
        ),
        starts_disabled: action.start_disabled == Some(true),
        ability_starts_disabled,
    })
}

fn jump_kind(value: &str) -> Option<JumpOrderType> {
    if value.eq_ignore_ascii_case("Jump") {
        Some(JumpOrderType::Jump)
    } else if value.eq_ignore_ascii_case("JumpGather") {
        Some(JumpOrderType::Gather)
    } else if value.eq_ignore_ascii_case("JumpGarrison") {
        Some(JumpOrderType::Garrison)
    } else if value.eq_ignore_ascii_case("JumpAttack") {
        Some(JumpOrderType::Attack)
    } else {
        None
    }
}

fn find_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name))
}

fn has_flag(prototype: &ProtoObject, expected: &str) -> bool {
    prototype
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
