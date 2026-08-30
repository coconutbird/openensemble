//! Persistent Hunter `SpiritBond` profiles from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Immutable authored inputs for one retail squad `SpiritBond` action.
#[derive(Debug, Clone, PartialEq)]
pub struct SpiritBondProfile {
    action_name: String,
    damage_modifier: f32,
    beam_proto_object: Option<String>,
    starts_disabled: bool,
}

impl SpiritBondProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Outgoing damage multiplier applied to both bonded squadmates.
    #[must_use]
    pub const fn damage_modifier(&self) -> f32 {
        self.damage_modifier
    }

    /// Class-zero visual stretched between the two bonded members.
    #[must_use]
    pub fn beam_proto_object(&self) -> Option<&str> {
        self.beam_proto_object.as_deref()
    }

    /// Whether the action waits for a player or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }
}

impl GameplayCatalog {
    /// Return the persistent squad `SpiritBond` action for an object prototype.
    #[must_use]
    pub fn spirit_bond(&self, proto_object_name: &str) -> Option<&SpiritBondProfile> {
        self.spirit_bonds
            .get(&proto_object_name.to_ascii_lowercase())
    }
}

pub(super) fn collect_spirit_bonds(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, SpiritBondProfile> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let action =
                rules.persistent_squad_actions.iter().find_map(|name| {
                    gameplay.tactics.actions.iter().find(|action| {
                        action.name.eq_ignore_ascii_case(name) && is_spirit_bond(action)
                    })
                })?;
            Some((key.clone(), profile_from_action(action)))
        })
        .collect()
}

fn is_spirit_bond(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("SpiritBond"))
}

fn profile_from_action(action: &Action) -> SpiritBondProfile {
    SpiritBondProfile {
        action_name: action.name.clone(),
        damage_modifier: action
            .damage_modifiers
            .as_ref()
            .and_then(|modifiers| modifiers.damage)
            .filter(|value| value.is_finite())
            .unwrap_or(1.0),
        beam_proto_object: action
            .proto_object
            .as_ref()
            .map(|reference| reference.name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        starts_disabled: action.start_disabled == Some(true),
    }
}

#[cfg(test)]
mod tests;
