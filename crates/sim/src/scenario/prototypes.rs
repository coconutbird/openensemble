//! Database prototype lookup and runtime classification helpers.

use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlacedUnitKind {
    Mobile,
    Building,
}

pub(super) fn classify_proto_object(proto: &ProtoObject) -> Option<PlacedUnitKind> {
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
        .is_some_and(|class| class.eq_ignore_ascii_case("Object"))
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

pub(super) fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}
