//! Scenario-layered `MergedSquads` synthetic prototype mappings.

use pipeline::database::hw1::Database;
use pipeline::xmb::Document;
use std::collections::BTreeMap;

/// Immutable synthetic proto-squad produced by a retail Merge join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedSquadProfile {
    joining_proto_squad_name: String,
    target_proto_squad_name: String,
    proto_squad_name: String,
    proto_squad_id: i32,
}

/// Every valid source/target pair authored by the active `squads.xml`.
#[derive(Debug, Clone, Default)]
pub(crate) struct MergedSquadProfiles {
    profiles: BTreeMap<(String, String), MergedSquadProfile>,
}

impl MergedSquadProfile {
    /// Proto squad contributing the joining unit.
    #[must_use]
    pub fn joining_proto_squad_name(&self) -> &str {
        &self.joining_proto_squad_name
    }

    /// Proto squad that receives the joining unit.
    #[must_use]
    pub fn target_proto_squad_name(&self) -> &str {
        &self.target_proto_squad_name
    }

    /// Retail synthetic name, `merged_{target}_{joining}`.
    #[must_use]
    pub fn proto_squad_name(&self) -> &str {
        &self.proto_squad_name
    }

    /// Appended retail proto-squad table index.
    #[must_use]
    pub const fn proto_squad_id(&self) -> i32 {
        self.proto_squad_id
    }
}

impl MergedSquadProfiles {
    pub(crate) fn from_document(database: &Database, document: &Document) -> Self {
        let Some(root) = document.root() else {
            return Self::default();
        };
        let mut profiles = BTreeMap::new();
        let mut synthetic_index = database.squads.len();
        for merged in root
            .children
            .iter()
            .filter(|node| node.name.eq_ignore_ascii_case("MergedSquads"))
        {
            let Some(joining) = known_squad_name(database, &merged.text_string()) else {
                continue;
            };
            for target in merged
                .children
                .iter()
                .filter(|node| node.name.eq_ignore_ascii_case("MergedSquad"))
            {
                let Some(target) = known_squad_name(database, &target.text_string()) else {
                    continue;
                };
                let Ok(proto_squad_id) = i32::try_from(synthetic_index) else {
                    continue;
                };
                synthetic_index = synthetic_index.saturating_add(1);
                let profile = MergedSquadProfile {
                    proto_squad_name: format!("merged_{target}_{joining}"),
                    joining_proto_squad_name: joining.clone(),
                    target_proto_squad_name: target.clone(),
                    proto_squad_id,
                };
                profiles.insert(
                    (joining.to_ascii_lowercase(), target.to_ascii_lowercase()),
                    profile,
                );
            }
        }
        Self { profiles }
    }

    pub(super) fn resolve(
        &self,
        joining_proto_squad: &str,
        target_proto_squad: &str,
    ) -> Option<&MergedSquadProfile> {
        self.profiles.get(&(
            joining_proto_squad.to_ascii_lowercase(),
            target_proto_squad.to_ascii_lowercase(),
        ))
    }
}

fn known_squad_name(database: &Database, value: &str) -> Option<String> {
    let value = value.trim();
    database
        .squads
        .iter()
        .find(|squad| squad.name.eq_ignore_ascii_case(value))
        .map(|squad| squad.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::Squad;

    #[test]
    fn raw_mapping_builds_retail_named_appended_profiles_in_file_order() {
        let database = Database {
            squads: vec![squad("spartan"), squad("marine"), squad("odst")],
            ..Database::default()
        };
        let document = Document::from_xml(
            "<Squads><MergedSquads>spartan<MergedSquad>marine</MergedSquad>\
             <MergedSquad>odst</MergedSquad></MergedSquads></Squads>",
        )
        .unwrap();

        let profiles = MergedSquadProfiles::from_document(&database, &document);
        let marine = profiles.resolve("SPARTAN", "Marine").unwrap();
        assert_eq!(marine.proto_squad_name(), "merged_marine_spartan");
        assert_eq!(marine.proto_squad_id(), 3);
        assert_eq!(marine.joining_proto_squad_name(), "spartan");
        assert_eq!(marine.target_proto_squad_name(), "marine");
        assert_eq!(
            profiles
                .resolve("spartan", "odst")
                .unwrap()
                .proto_squad_id(),
            4
        );
    }

    #[test]
    fn unknown_squads_do_not_create_profiles_or_consume_runtime_ids() {
        let database = Database {
            squads: vec![squad("spartan"), squad("marine")],
            ..Database::default()
        };
        let document = Document::from_xml(
            "<Squads><MergedSquads>missing<MergedSquad>marine</MergedSquad></MergedSquads>\
             <MergedSquads>spartan<MergedSquad>missing</MergedSquad>\
             <MergedSquad>marine</MergedSquad></MergedSquads></Squads>",
        )
        .unwrap();

        let profiles = MergedSquadProfiles::from_document(&database, &document);
        assert!(profiles.resolve("missing", "marine").is_none());
        assert!(profiles.resolve("spartan", "missing").is_none());
        assert_eq!(
            profiles
                .resolve("spartan", "marine")
                .unwrap()
                .proto_squad_id(),
            2
        );
    }

    fn squad(name: &str) -> Squad {
        Squad {
            name: name.to_owned(),
            ..Squad::default()
        }
    }
}
