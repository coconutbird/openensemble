//! Scenario-layered `MergedSquads` synthetic prototype mappings.

use crate::prototypes::{merged_proto_squads, merged_squad_name};
use pipeline::database::hw1::Database;
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

    /// Engine proto squad ID: the merged squads follow the `<Squad>` records.
    #[must_use]
    pub const fn proto_squad_id(&self) -> i32 {
        self.proto_squad_id
    }
}

impl MergedSquadProfiles {
    pub(crate) fn from_database(database: &Database) -> Self {
        let mut profiles = BTreeMap::new();
        for (proto_squad_id, joining, target) in merged_proto_squads(database) {
            profiles
                .entry((
                    joining.name.to_ascii_lowercase(),
                    target.name.to_ascii_lowercase(),
                ))
                .or_insert_with(|| MergedSquadProfile {
                    proto_squad_name: merged_squad_name(joining, target),
                    joining_proto_squad_name: joining.name.clone(),
                    target_proto_squad_name: target.name.clone(),
                    proto_squad_id,
                });
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

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::Squad;
    use pipeline::database::hw1::squads::MergedSquads;

    #[test]
    fn profiles_use_retail_names_and_appended_ids_in_file_order() {
        let database = Database {
            squads: vec![squad("spartan"), squad("marine"), squad("odst")],
            merged_squads: vec![MergedSquads {
                squad: "spartan".to_owned(),
                targets: vec!["marine".to_owned(), "odst".to_owned()],
            }],
            ..Database::default()
        };

        let profiles = MergedSquadProfiles::from_database(&database);
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
    fn unknown_squads_do_not_create_profiles_or_consume_ids() {
        let database = Database {
            squads: vec![squad("spartan"), squad("marine")],
            merged_squads: vec![
                MergedSquads {
                    squad: "missing".to_owned(),
                    targets: vec!["marine".to_owned()],
                },
                MergedSquads {
                    squad: "spartan".to_owned(),
                    targets: vec!["missing".to_owned(), "marine".to_owned()],
                },
            ],
            ..Database::default()
        };

        let profiles = MergedSquadProfiles::from_database(&database);
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
