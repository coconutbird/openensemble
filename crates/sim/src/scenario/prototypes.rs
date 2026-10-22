//! Database prototype lookup and runtime classification helpers.

use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacedUnitKind {
    Mobile,
    Building,
}

pub(crate) fn classify_proto_object(proto: &ProtoObject) -> Option<PlacedUnitKind> {
    if proto
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Building"))
        || proto
            .select_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("Building"))
        || creates_base(proto)
    {
        return Some(PlacedUnitKind::Building);
    }
    proto
        .object_class
        .as_deref()
        .is_some_and(|class| {
            class.eq_ignore_ascii_case("Unit") || class.eq_ignore_ascii_case("Squad")
        })
        .then_some(PlacedUnitKind::Mobile)
}

pub(super) fn is_class_zero_object(proto: &ProtoObject) -> bool {
    proto
        .object_class
        .as_deref()
        .is_none_or(|class| class.eq_ignore_ascii_case("Object"))
}

pub(super) fn creates_base(proto: &ProtoObject) -> bool {
    prototype_has_flag(proto, "KBCreatesBase")
}

pub(super) fn prototype_has_flag(proto: &ProtoObject, expected: &str) -> bool {
    proto
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}

pub(super) fn find_proto_object<'a>(
    database: &'a Database,
    name: &str,
) -> Option<(usize, &'a ProtoObject)> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

pub(super) fn find_proto_squad<'a>(
    database: &'a Database,
    name: &str,
) -> Option<(usize, &'a ProtoSquad)> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

/// Proto object of a gamedata `<CodeProtoObject Type="...">` entry, with its proto ID.
///
/// `BDatabase_setupDBIDs` resolves each entry to a proto object ID, or `-1` if the name is
/// unknown (for example `dword_141524D4C` for `SkirmishEmptyBaseObject`).
pub(super) fn code_proto_object<'a>(
    database: &'a Database,
    object_type: &str,
) -> Option<(usize, &'a ProtoObject)> {
    let name = database
        .game_data
        .as_ref()?
        .code_proto_objects
        .as_ref()?
        .entries
        .iter()
        .find(|mapping| mapping.object_type.eq_ignore_ascii_case(object_type))?
        .proto_name
        .as_str();
    crate::prototypes::object_entry_by_name_trim_both(database, name)
}

/// Proto ID of a gamedata `<CodeProtoObject>` entry. See [`code_proto_object`].
pub(super) fn code_proto_object_index(database: &Database, object_type: &str) -> Option<usize> {
    code_proto_object(database, object_type).map(|(index, _)| index)
}
