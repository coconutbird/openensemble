//! Code-referenced proto-object mappings from the layered game-data table.

use pipeline::database::hw1::Database;

pub(super) fn named_proto(database: &Database, object_type: &str) -> Option<String> {
    database
        .game_data
        .as_ref()?
        .code_proto_objects
        .as_ref()?
        .entries
        .iter()
        .find(|entry| entry.object_type.trim().eq_ignore_ascii_case(object_type))
        .map(|entry| entry.proto_name.trim().to_owned())
        .filter(|name| !name.is_empty())
}
