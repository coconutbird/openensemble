//! Per-player prototype availability overrides.
//!
//! Retail clones object, squad, and technology prototypes for every player and
//! mutates their `Forbid` flags. The sim stores only values that differ from the
//! scenario-layered database while exposing the same effective state.

use super::Player;
use crate::sync::SyncChecksum;
use pipeline::database::hw1::{Database, ProtoObject, Squad, Tech};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub(super) struct PlayerForbidState {
    objects: BTreeMap<i32, bool>,
    squads: BTreeMap<i32, bool>,
    technologies: BTreeMap<i32, bool>,
}

impl Player {
    /// Return whether this player's object prototype is forbidden.
    ///
    /// Object IDs use the wire/database ID stored by trigger variables and
    /// live entities, rather than the object's position in `Database::objects`.
    #[must_use]
    pub fn is_object_forbidden(&self, database: &Database, prototype_id: i32) -> bool {
        object_by_id(database, prototype_id).is_some_and(|prototype| {
            effective_flag(
                &self.forbids.objects,
                prototype_id,
                has_flag(&prototype.flags, "Forbid"),
            )
        })
    }

    /// Return whether this player's squad prototype is forbidden.
    ///
    /// Squad IDs use the wire/database ID stored by trigger variables and live
    /// squads, rather than the squad's position in `Database::squads`.
    #[must_use]
    pub fn is_squad_forbidden(&self, database: &Database, prototype_id: i32) -> bool {
        squad_by_id(database, prototype_id).is_some_and(|prototype| {
            effective_flag(
                &self.forbids.squads,
                prototype_id,
                has_flag(&prototype.flags, "Forbid"),
            )
        })
    }

    /// Return whether this player's technology prototype is forbidden.
    ///
    /// Technology IDs are runtime indices, matching retail trigger values.
    #[must_use]
    pub fn is_technology_forbidden(&self, database: &Database, technology_id: i32) -> bool {
        technology_by_id(database, technology_id).is_some_and(|technology| {
            effective_flag(
                &self.forbids.technologies,
                technology_id,
                has_flag(&technology.flags, "Forbid"),
            )
        })
    }

    /// Set this player's object override, returning whether effective state changed.
    ///
    /// Returns `None` when `prototype_id` is not present in the active database.
    pub fn set_object_forbidden(
        &mut self,
        database: &Database,
        prototype_id: i32,
        forbidden: bool,
    ) -> Option<bool> {
        let authored = object_by_id(database, prototype_id)
            .map(|prototype| has_flag(&prototype.flags, "Forbid"))?;
        Some(set_override(
            &mut self.forbids.objects,
            prototype_id,
            authored,
            forbidden,
        ))
    }

    /// Set this player's squad override, returning whether effective state changed.
    ///
    /// Returns `None` when `prototype_id` is not present in the active database.
    pub fn set_squad_forbidden(
        &mut self,
        database: &Database,
        prototype_id: i32,
        forbidden: bool,
    ) -> Option<bool> {
        let authored = squad_by_id(database, prototype_id)
            .map(|prototype| has_flag(&prototype.flags, "Forbid"))?;
        Some(set_override(
            &mut self.forbids.squads,
            prototype_id,
            authored,
            forbidden,
        ))
    }

    /// Set this player's technology override, returning whether effective state changed.
    ///
    /// Returns `None` when `technology_id` is not present in the active database.
    pub fn set_technology_forbidden(
        &mut self,
        database: &Database,
        technology_id: i32,
        forbidden: bool,
    ) -> Option<bool> {
        let authored = technology_by_id(database, technology_id)
            .map(|technology| has_flag(&technology.flags, "Forbid"))?;
        Some(set_override(
            &mut self.forbids.technologies,
            technology_id,
            authored,
            forbidden,
        ))
    }

    pub(crate) fn hash_forbid_state(&self, checksum: &mut SyncChecksum) {
        hash_overrides(checksum, &self.forbids.objects);
        hash_overrides(checksum, &self.forbids.squads);
        hash_overrides(checksum, &self.forbids.technologies);
    }
}

fn effective_flag(overrides: &BTreeMap<i32, bool>, id: i32, authored: bool) -> bool {
    overrides.get(&id).copied().unwrap_or(authored)
}

fn set_override(
    overrides: &mut BTreeMap<i32, bool>,
    id: i32,
    authored: bool,
    forbidden: bool,
) -> bool {
    let previous = effective_flag(overrides, id, authored);
    if previous == forbidden {
        return false;
    }
    if forbidden == authored {
        overrides.remove(&id);
    } else {
        overrides.insert(id, forbidden);
    }
    true
}

fn object_by_id(database: &Database, id: i32) -> Option<&ProtoObject> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(index, prototype)| database_id(prototype.dbid, *index) == id)
        .map(|(_, prototype)| prototype)
}

fn squad_by_id(database: &Database, id: i32) -> Option<&Squad> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(index, prototype)| database_id(prototype.dbid, *index) == id)
        .map(|(_, prototype)| prototype)
}

fn technology_by_id(database: &Database, id: i32) -> Option<&Tech> {
    usize::try_from(id)
        .ok()
        .and_then(|index| database.techs.get(index))
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn has_flag(flags: &[String], expected: &str) -> bool {
    flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

fn hash_overrides(checksum: &mut SyncChecksum, overrides: &BTreeMap<i32, bool>) {
    checksum.hash_u32(u32::try_from(overrides.len()).unwrap_or(u32::MAX));
    for (&id, &forbidden) in overrides {
        checksum.hash_i32(id);
        checksum.hash_u32(u32::from(forbidden));
    }
}

#[cfg(test)]
mod tests;
