//! Persistent squad `Cloak` profiles from scenario-layered authoring.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeMap;

/// Immutable authored inputs for one retail squad `Cloak` action.
#[derive(Debug, Clone, PartialEq)]
pub struct CloakProfile {
    action_name: String,
    effect_proto_object: Option<String>,
    flags: CloakFlags,
    cloaking_delay: f32,
    recloak_delay: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CloakFlags(u8);

impl CloakFlags {
    const STARTS_DISABLED: u8 = 1 << 0;
    const PERMANENT: u8 = 1 << 1;
    const AUTO_CLOAK: u8 = 1 << 2;
    const MOVE_WHILE_CLOAKED: u8 = 1 << 3;
    const ATTACK_WHILE_CLOAKED: u8 = 1 << 4;

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

impl CloakProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Per-member visual-effect prototype attached while cloaked.
    #[must_use]
    pub fn effect_proto_object(&self) -> Option<&str> {
        self.effect_proto_object.as_deref()
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.flags.contains(CloakFlags::STARTS_DISABLED)
    }

    /// Retail's `NoAutoTarget` reuse that activates permanent cloak at startup.
    #[must_use]
    pub const fn permanent(&self) -> bool {
        self.flags.contains(CloakFlags::PERMANENT)
    }

    /// Whether the prototype carries the authored `AutoCloak` marker.
    #[must_use]
    pub const fn auto_cloak(&self) -> bool {
        self.flags.contains(CloakFlags::AUTO_CLOAK)
    }

    /// Whether active movement may retain cloak.
    #[must_use]
    pub const fn move_while_cloaked(&self) -> bool {
        self.flags.contains(CloakFlags::MOVE_WHILE_CLOAKED)
    }

    /// Return the authored attack-while-cloaked marker.
    ///
    /// Retail retains this data but disabled the corresponding uncloak branch.
    #[must_use]
    pub const fn attack_while_cloaked(&self) -> bool {
        self.flags.contains(CloakFlags::ATTACK_WHILE_CLOAKED)
    }

    /// Delay between a cloak request and activation, in seconds.
    #[must_use]
    pub const fn cloaking_delay(&self) -> f32 {
        self.cloaking_delay
    }

    /// Time an enemy detection notification remains active, in seconds.
    #[must_use]
    pub const fn recloak_delay(&self) -> f32 {
        self.recloak_delay
    }
}

impl GameplayCatalog {
    /// Return the persistent squad `Cloak` action for an object prototype.
    #[must_use]
    pub fn cloak(&self, proto_object_name: &str) -> Option<&CloakProfile> {
        self.cloak_actions
            .get(&proto_object_name.to_ascii_lowercase())
    }
}

pub(super) fn collect_cloaks(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, CloakProfile> {
    let cloaking_delay = game_data_time(database, |data| data.cloaking_delay);
    let recloak_delay = game_data_time(database, |data| data.recloak_delay);
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let action = rules.persistent_squad_actions.iter().find_map(|name| {
                gameplay
                    .tactics
                    .actions
                    .iter()
                    .find(|action| action.name.eq_ignore_ascii_case(name) && is_cloak(action))
            })?;
            let object = database.objects.iter().find(|object| {
                object
                    .name
                    .eq_ignore_ascii_case(&gameplay.proto_object_name)
            })?;
            Some((
                key.clone(),
                profile_from_action(action, object, cloaking_delay, recloak_delay),
            ))
        })
        .collect()
}

fn is_cloak(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Cloak"))
}

fn profile_from_action(
    action: &Action,
    object: &ProtoObject,
    cloaking_delay: f32,
    recloak_delay: f32,
) -> CloakProfile {
    CloakProfile {
        action_name: action.name.clone(),
        effect_proto_object: action
            .proto_object
            .as_ref()
            .map(|reference| reference.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        flags: cloak_flags(action, object),
        cloaking_delay,
        recloak_delay,
    }
}

fn cloak_flags(action: &Action, object: &ProtoObject) -> CloakFlags {
    let mut flags = 0;
    flags |= u8::from(action.start_disabled == Some(true)) * CloakFlags::STARTS_DISABLED;
    flags |= u8::from(action.no_auto_target == Some(true)) * CloakFlags::PERMANENT;
    flags |= u8::from(has_flag(object, "AutoCloak")) * CloakFlags::AUTO_CLOAK;
    flags |= u8::from(has_flag(object, "MoveWhileCloaked")) * CloakFlags::MOVE_WHILE_CLOAKED;
    flags |= u8::from(has_flag(object, "AttackWhileCloaked")) * CloakFlags::ATTACK_WHILE_CLOAKED;
    CloakFlags(flags)
}

fn has_flag(object: &ProtoObject, expected: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}

fn game_data_time(
    database: &Database,
    select: impl FnOnce(&pipeline::database::hw1::GameData) -> Option<f32>,
) -> f32 {
    database
        .game_data
        .as_ref()
        .and_then(select)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
