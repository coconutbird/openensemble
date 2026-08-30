//! Persistent energy-shield presentation resolved from scenario-layered tactics.

use super::{GameplayCatalog, ObjectGameplay};
use crate::spawn::object_prototype_id;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::tactics::Action;
use std::collections::BTreeMap;

/// Visual behavior owned by one retail persistent energy-shield action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnergyShieldVisualProfile {
    /// A class-zero proto object attached while integral shields are raised.
    Attachment {
        prototype_name: String,
        prototype_id: Option<i32>,
        bone_name: Option<String>,
    },
    /// A named visual component hidden by the infantry shield action.
    Infantry {
        component_name: String,
        hit_duration_ms: u32,
    },
}

/// Immutable authored inputs for one persistent shield presentation action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnergyShieldActionProfile {
    action_name: String,
    starts_disabled: bool,
    visual: EnergyShieldVisualProfile,
}

impl EnergyShieldActionProfile {
    /// Authored action name used by live and technology enablement.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Whether the action waits for a technology or live enablement override.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    /// Presentation contract projected from authoritative simulation state.
    #[must_use]
    pub const fn visual(&self) -> &EnergyShieldVisualProfile {
        &self.visual
    }
}

impl GameplayCatalog {
    /// Iterate persistent energy-shield actions for one object prototype.
    pub fn energy_shield_actions(&self, proto_object_name: &str) -> &[EnergyShieldActionProfile] {
        self.energy_shield_actions
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Resolve one named persistent energy-shield action.
    #[must_use]
    pub fn energy_shield_action(
        &self,
        proto_object_name: &str,
        action_name: &str,
    ) -> Option<&EnergyShieldActionProfile> {
        self.energy_shield_actions(proto_object_name)
            .iter()
            .find(|profile| profile.action_name.eq_ignore_ascii_case(action_name))
    }

    pub(crate) fn energy_shield_animation_requests(&self) -> Vec<(String, String)> {
        self.energy_shield_actions
            .values()
            .flatten()
            .filter_map(|profile| match &profile.visual {
                EnergyShieldVisualProfile::Attachment { prototype_name, .. } => {
                    Some(prototype_name)
                }
                EnergyShieldVisualProfile::Infantry { .. } => None,
            })
            .flat_map(|prototype| {
                ["Idle", "Incoming", "Death"]
                    .map(|animation| (prototype.clone(), animation.to_owned()))
            })
            .collect()
    }
}

pub(super) fn collect_energy_shield_actions(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, Vec<EnergyShieldActionProfile>> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let rules = gameplay.tactics.tactic.as_ref()?;
            let profiles = rules
                .persistent_actions
                .iter()
                .filter_map(|name| {
                    gameplay
                        .tactics
                        .actions
                        .iter()
                        .find(|action| action.name.eq_ignore_ascii_case(name))
                })
                .filter_map(|action| profile_from_action(database, action))
                .collect::<Vec<_>>();
            (!profiles.is_empty()).then(|| (key.clone(), profiles))
        })
        .collect()
}

fn profile_from_action(database: &Database, action: &Action) -> Option<EnergyShieldActionProfile> {
    let action_type = action.action_type.as_deref()?;
    let visual = if action_type.eq_ignore_ascii_case("EnergyShield") {
        let reference = action.proto_object.as_ref()?;
        let prototype_name = reference.name.trim();
        if prototype_name.is_empty() {
            return None;
        }
        EnergyShieldVisualProfile::Attachment {
            prototype_name: prototype_name.to_owned(),
            prototype_id: object_prototype_id(database, prototype_name),
            bone_name: trimmed(reference.bone.as_deref()),
        }
    } else if action_type.eq_ignore_ascii_case("InfantryEnergyShield") {
        EnergyShieldVisualProfile::Infantry {
            component_name: "Shield".to_owned(),
            hit_duration_ms: duration_ms(action),
        }
    } else {
        return None;
    };
    Some(EnergyShieldActionProfile {
        action_name: action.name.clone(),
        starts_disabled: action.start_disabled == Some(true),
        visual,
    })
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn duration_ms(action: &Action) -> u32 {
    action
        .duration
        .as_ref()
        .map(|duration| duration.seconds)
        .filter(|duration| duration.is_finite() && *duration >= 0.0)
        .unwrap_or_default()
        .mul_add(1_000.0, 0.0)
        .to_u32()
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
