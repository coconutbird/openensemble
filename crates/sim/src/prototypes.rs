//! Shared lookups of object and squad prototypes in the game database.
//!
//! Each lookup scans the database in order and returns the first match.
//! Name lookups compare names without case. The callers use different
//! whitespace policies, so each policy has its own function:
//!
//! - `untrimmed`: compare the names as they are.
//! - `trim_query`: remove the outer whitespace of the query name only.
//! - `trim_both`: remove the outer whitespace of the query and prototype names.

use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

/// Return the runtime database ID of a prototype.
///
/// An explicit `dbid` has priority. Otherwise the database index is the ID, or
/// `-1` if the index does not fit in an `i32`.
pub(crate) fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

/// Find the first object prototype with this runtime database ID.
pub(crate) fn object_by_id(database: &Database, id: i32) -> Option<&ProtoObject> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(index, proto)| database_id(proto.dbid, *index) == id)
        .map(|(_, proto)| proto)
}

/// Find the first squad prototype with this runtime database ID.
pub(crate) fn squad_by_id(database: &Database, id: i32) -> Option<&ProtoSquad> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(index, proto)| database_id(proto.dbid, *index) == id)
        .map(|(_, proto)| proto)
}

/// Find the first object prototype with this name. Do not trim either name.
pub(crate) fn object_by_name_untrimmed<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(name))
}

/// Find the first object prototype with this name. Trim only the query name.
pub(crate) fn object_by_name_trim_query<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    object_entry_by_name_trim_query(database, name).map(|(_, object)| object)
}

/// Find the index and the first object prototype with this name. Trim only the
/// query name.
pub(crate) fn object_entry_by_name_trim_query<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoObject)> {
    let name = name.trim();
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, object)| object.name.eq_ignore_ascii_case(name))
}

/// Find the first object prototype with this name. Trim the query name and the
/// prototype name.
pub(crate) fn object_by_name_trim_both<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    object_entry_by_name_trim_both(database, name).map(|(_, object)| object)
}

/// Find the index and the first object prototype with this name. Trim the
/// query name and the prototype name.
pub(crate) fn object_entry_by_name_trim_both<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoObject)> {
    let name = name.trim();
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, object)| object.name.trim().eq_ignore_ascii_case(name))
}

/// Find the first squad prototype with this name. Trim only the query name.
pub(crate) fn squad_by_name_trim_query<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoSquad> {
    squad_entry_by_name_trim_query(database, name).map(|(_, squad)| squad)
}

/// Find the index and the first squad prototype with this name. Trim only the
/// query name.
pub(crate) fn squad_entry_by_name_trim_query<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoSquad)> {
    let name = name.trim();
    database
        .squads
        .iter()
        .enumerate()
        .find(|(_, squad)| squad.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests;
