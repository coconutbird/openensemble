//! Persistent player-prototype substitutions authored by technology effects.

use crate::sync::SyncChecksum;
use pipeline::database::hw1::techs::TechEffect;
use std::collections::BTreeMap;

/// One concrete prototype substitution that must update live simulation units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AppliedPrototypeTransform {
    Unit(AppliedUnitTransform),
    Squad(AppliedSquadTransform),
}

/// One persistent proto-unit transformation applied by a newly active tech.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedUnitTransform {
    pub from: String,
    pub previous_definition: String,
    pub new_definition: String,
}

/// One persistent proto-squad transformation applied by a newly active tech.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedSquadTransform {
    pub from: String,
    pub previous_definition: String,
    pub new_definition: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PrototypeTransforms {
    units: BTreeMap<String, String>,
    squads: BTreeMap<String, String>,
}

impl PrototypeTransforms {
    pub(crate) fn resolve_unit<'name>(&'name self, logical_name: &'name str) -> &'name str {
        self.units
            .get(&normalize(logical_name))
            .map_or(logical_name, String::as_str)
    }

    pub(crate) fn resolve_squad<'name>(&'name self, logical_name: &'name str) -> &'name str {
        self.squads
            .get(&normalize(logical_name))
            .map_or(logical_name, String::as_str)
    }

    pub(crate) fn apply(&mut self, effect: &TechEffect) -> Option<AppliedPrototypeTransform> {
        let from = nonempty(effect.from_type.as_deref())?;
        let new_definition = nonempty(effect.to_type.as_deref())?;
        if effect
            .effect_type
            .eq_ignore_ascii_case("TransformProtoUnit")
        {
            let previous_definition = self.resolve_unit(from).to_owned();
            self.units
                .insert(normalize(from), new_definition.to_owned());
            return Some(AppliedPrototypeTransform::Unit(AppliedUnitTransform {
                from: from.to_owned(),
                previous_definition,
                new_definition: new_definition.to_owned(),
            }));
        }
        if effect
            .effect_type
            .eq_ignore_ascii_case("TransformProtoSquad")
        {
            let previous_definition = self.resolve_squad(from).to_owned();
            self.squads
                .insert(normalize(from), new_definition.to_owned());
            return Some(AppliedPrototypeTransform::Squad(AppliedSquadTransform {
                from: from.to_owned(),
                previous_definition,
                new_definition: new_definition.to_owned(),
            }));
        }
        None
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_mappings(checksum, &self.units);
        hash_mappings(checksum, &self.squads);
    }
}

fn hash_mappings(checksum: &mut SyncChecksum, mappings: &BTreeMap<String, String>) {
    checksum.hash_u32(u32::try_from(mappings.len()).unwrap_or(u32::MAX));
    for (from, to) in mappings {
        hash_string(checksum, from);
        hash_string(checksum, to);
    }
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
