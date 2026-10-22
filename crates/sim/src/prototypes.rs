//! Engine proto IDs and shared lookups of object and squad prototypes.
//!
//! The engine gives each proto object, proto squad, tech and power an ID that is
//! equal to its load-order index (`BDatabase::setupProtoObjects` and
//! `BDatabase::setupProtoSquads`). The `dbid` attribute is only a key for save
//! games, and it is not unique. Thus the sim uses the index in
//! [`Database::objects`] as the proto object ID.
//!
//! The engine proto squad table has three ranges, in this order:
//!
//! 1. The `<Squad>` records of [`Database::squads`].
//! 2. One merged squad for each valid `<MergedSquad>` child of
//!    [`Database::merged_squads`], in document order.
//! 3. One implicit squad for each Squad, Building or Unit class proto object
//!    that no same-named one-unit squad represents, in object order.
//!
//! The squad name map holds the `<Squad>` names first, then the merged squad
//! names, then the object names of the implicit squads. The first entry of a
//! name wins.
//!
//! The name lookups compare names without case (`_stricmp` in the engine). The
//! callers use different whitespace policies, so each policy has its own
//! function:
//!
//! - `untrimmed`: compare the names as they are.
//! - `trim_query`: remove the outer whitespace of the query name only.
//! - `trim_both`: remove the outer whitespace of the query and prototype names.

use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

/// Return the proto ID of a table index, or `-1` if the index does not fit in an
/// `i32`.
pub(crate) fn proto_id(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(-1)
}

/// Declare each object type that the objects of a test database name, in first-use order, as
/// the shipped `objecttypes.xml` does for the shipped objects.
#[cfg(test)]
pub(crate) fn declare_authored_object_types(database: &mut Database) {
    for object in &database.objects {
        for name in &object.object_types {
            let declared = database
                .object_types
                .iter()
                .any(|object_type| object_type.name.eq_ignore_ascii_case(name.trim()));
            if !declared {
                database
                    .object_types
                    .push(pipeline::database::hw1::ObjectType {
                        name: name.trim().to_owned(),
                    });
            }
        }
    }
}

/// Find the object prototype with this proto ID (its load-order index).
pub(crate) fn object_by_id(database: &Database, id: i32) -> Option<&ProtoObject> {
    usize::try_from(id)
        .ok()
        .and_then(|index| database.objects.get(index))
}

/// Find the `<Squad>` record with this proto squad ID (its load-order index).
///
/// Merged and implicit proto squads have no record. Use [`proto_squad`] for
/// them.
pub(crate) fn squad_by_id(database: &Database, id: i32) -> Option<&ProtoSquad> {
    usize::try_from(id)
        .ok()
        .and_then(|index| database.squads.get(index))
}

/// One entry of the engine proto squad table.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ProtoSquadEntry<'database> {
    /// A `<Squad>` record.
    Squad(&'database ProtoSquad),
    /// A merged squad: `joining` joined the `target` squad.
    Merged {
        /// The squad that the `<MergedSquads>` text names.
        joining: &'database ProtoSquad,
        /// The squad that the `<MergedSquad>` child names.
        target: &'database ProtoSquad,
    },
    /// An implicit squad that holds one unit of this proto object.
    Implicit {
        /// Proto object ID of the unit.
        object_index: usize,
        /// The proto object of the unit.
        object: &'database ProtoObject,
    },
}

impl ProtoSquadEntry<'_> {
    /// Return the name of the proto squad.
    ///
    /// A merged squad has the name `merged_{target}_{joining}`
    /// (`sub_1403506F0`). An implicit squad has the name of its object.
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Squad(squad) => squad.name.clone(),
            Self::Merged { joining, target } => merged_squad_name(joining, target),
            Self::Implicit { object, .. } => object.name.clone(),
        }
    }

    /// Return the number of units that the proto squad holds (`BProtoSquad`
    /// data offset 40).
    ///
    /// A merged squad holds the units of its target and of its joining squad.
    pub(crate) fn size(&self, database: &Database) -> u32 {
        match self {
            Self::Squad(squad) => squad_size(database, squad),
            Self::Merged { joining, target } => {
                squad_size(database, target).saturating_add(squad_size(database, joining))
            }
            Self::Implicit { .. } => 1,
        }
    }
}

/// Find the engine proto squad with this proto squad ID.
pub(crate) fn proto_squad(database: &Database, id: i32) -> Option<ProtoSquadEntry<'_>> {
    let index = usize::try_from(id).ok()?;
    if let Some(squad) = database.squads.get(index) {
        return Some(ProtoSquadEntry::Squad(squad));
    }
    let mut merged = merged_squads(database);
    let merged_index = index - database.squads.len();
    let mut merged_count = 0;
    for (position, (joining, target)) in merged.by_ref().enumerate() {
        if position == merged_index {
            return Some(ProtoSquadEntry::Merged { joining, target });
        }
        merged_count = position + 1;
    }
    let implicit_index = merged_index - merged_count;
    implicit_squad_objects(database)
        .nth(implicit_index)
        .map(|object_index| ProtoSquadEntry::Implicit {
            object_index,
            object: &database.objects[object_index],
        })
}

/// Iterate over every engine proto squad with its proto squad ID, in ID order.
pub(crate) fn proto_squads(
    database: &Database,
) -> impl Iterator<Item = (i32, ProtoSquadEntry<'_>)> {
    let squads = database.squads.iter().map(ProtoSquadEntry::Squad);
    let merged = merged_squads(database)
        .map(|(joining, target)| ProtoSquadEntry::Merged { joining, target });
    let implicit = implicit_squad_objects(database).map(|object_index| ProtoSquadEntry::Implicit {
        object_index,
        object: &database.objects[object_index],
    });
    squads
        .chain(merged)
        .chain(implicit)
        .enumerate()
        .map(|(index, entry)| (proto_id(index), entry))
}

/// Resolve a proto squad name with the engine squad name map
/// (`sub_140034930`). Trim only the query name.
///
/// The map holds the `<Squad>` names, then the merged squad names, then the
/// object names of the implicit squads. Thus a proto object name resolves to
/// the implicit squad of the object.
pub(crate) fn squad_id(database: &Database, name: &str) -> Option<i32> {
    if let Some((index, _)) = squad_entry_by_name_trim_query(database, name) {
        return Some(proto_id(index));
    }
    let name = name.trim();
    if let Some((id, _, _)) = merged_proto_squads(database)
        .find(|(_, joining, target)| merged_squad_name(joining, target).eq_ignore_ascii_case(name))
    {
        return Some(id);
    }
    let (object_index, _) = object_entry_by_name_trim_query(database, name)?;
    implicit_squad_id(database, object_index)
}

/// A resolved proto reference of a placed `.scn` object.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ScenarioProto<'database> {
    /// A proto squad.
    Squad(ProtoSquadEntry<'database>),
    /// A proto object, with its proto object ID.
    Object(usize, &'database ProtoObject),
}

/// Resolve the proto name of a placed `.scn` object as `BScenario_loadObject`
/// does. Trim only the query name.
///
/// - `is_squad` is `Some(true)`: use the squad name map.
/// - `is_squad` is `Some(false)`: use the object name map.
/// - `is_squad` is `None` (the `IsSquad` attribute is absent or does not
///   parse): use the squad name map, then the object name map.
///
/// The engine skips the object if the name does not resolve.
pub(crate) fn scenario_proto<'database>(
    database: &'database Database,
    name: &str,
    is_squad: Option<bool>,
) -> Option<ScenarioProto<'database>> {
    let squad = || {
        squad_id(database, name)
            .and_then(|id| proto_squad(database, id))
            .map(ScenarioProto::Squad)
    };
    let object = || {
        object_entry_by_name_trim_query(database, name)
            .map(|(index, object)| ScenarioProto::Object(index, object))
    };
    match is_squad {
        Some(true) => squad(),
        Some(false) => object(),
        None => squad().or_else(object),
    }
}

/// Return the proto squad ID that the engine stores on a proto object
/// (`BProtoObject` offset 868).
///
/// This is the same-named one-unit squad that represents the object, or else
/// the implicit squad of the object. Objects that are not of class Squad,
/// Building or Unit have no proto squad.
pub(crate) fn object_squad_id(database: &Database, object_index: usize) -> Option<i32> {
    let object = database.objects.get(object_index)?;
    if !has_implicit_squad_class(object) {
        return None;
    }
    if let Some(index) = representing_squad(database, object_index) {
        return Some(proto_id(index));
    }
    implicit_squad_id(database, object_index)
}

/// Return the `<Squad>` record that represents a proto object: it has the
/// object name and it holds exactly one unit of the object.
///
/// The engine rule (`BDatabase::setupProtoSquads`): the first squad of the name
/// matches if the sum of its unit counts is 1 and its first unit is the object.
/// Unit entries whose object name does not resolve do not count
/// (`BProtoSquad::load`).
pub(crate) fn representing_squad(database: &Database, object_index: usize) -> Option<usize> {
    let object = database.objects.get(object_index)?;
    let (index, squad) = squad_entry_by_name_untrimmed(database, &object.name)?;
    let (first, total) = resolved_units(database, squad, Some((object_index, &object.name)));
    (total == 1 && first == Some(object_index)).then_some(index)
}

/// Return the first resolved unit and the sum of the resolved unit counts of a
/// `<Squad>` record.
///
/// `BProtoSquad::load` drops a unit entry whose object name does not resolve.
/// `known` gives one object name that is known to resolve to an index, so the
/// lookup can skip the object table for it.
fn resolved_units(
    database: &Database,
    squad: &ProtoSquad,
    known: Option<(usize, &str)>,
) -> (Option<usize>, i64) {
    let entries = squad.units.as_ref().map_or(&[][..], |units| &units.entries);
    let mut first = None;
    let mut total = 0_i64;
    for entry in entries {
        let resolved = match known {
            Some((index, name)) if entry.proto_object.eq_ignore_ascii_case(name) => Some(index),
            _ => object_index_untrimmed(database, &entry.proto_object),
        };
        let Some(resolved) = resolved else {
            continue;
        };
        first.get_or_insert(resolved);
        total += i64::from(entry.count);
    }
    (first, total)
}

fn implicit_squad_id(database: &Database, object_index: usize) -> Option<i32> {
    let first_implicit = database.squads.len() + merged_squads(database).count();
    implicit_squad_objects(database)
        .take_while(|candidate| *candidate <= object_index)
        .position(|candidate| candidate == object_index)
        .map(|position| proto_id(first_implicit + position))
}

/// Iterate over the proto objects that get an implicit squad, in object order.
fn implicit_squad_objects(database: &Database) -> impl Iterator<Item = usize> + '_ {
    database
        .objects
        .iter()
        .enumerate()
        .filter(move |(index, object)| {
            has_implicit_squad_class(object) && representing_squad(database, *index).is_none()
        })
        .map(|(index, _)| index)
}

/// Iterate over the valid merged squads as `(proto squad ID, joining squad,
/// target squad)`, in proto squad ID order.
///
/// The engine stops the database load when a `<MergedSquads>` or
/// `<MergedSquad>` name does not resolve. The sim skips that entry, and the
/// entry gets no proto squad ID.
pub(crate) fn merged_proto_squads(
    database: &Database,
) -> impl Iterator<Item = (i32, &ProtoSquad, &ProtoSquad)> {
    merged_squads(database)
        .enumerate()
        .map(|(index, (joining, target))| {
            (proto_id(database.squads.len() + index), joining, target)
        })
}

fn merged_squads(database: &Database) -> impl Iterator<Item = (&ProtoSquad, &ProtoSquad)> {
    database.merged_squads.iter().flat_map(move |group| {
        let joining = squad_entry_by_name_untrimmed(database, &group.squad).map(|(_, squad)| squad);
        group.targets.iter().filter_map(move |target| {
            let target = squad_entry_by_name_untrimmed(database, target).map(|(_, squad)| squad)?;
            Some((joining?, target))
        })
    })
}

/// Return the engine name of a merged squad, `merged_{target}_{joining}`
/// (`sub_1403506F0`).
pub(crate) fn merged_squad_name(joining: &ProtoSquad, target: &ProtoSquad) -> String {
    format!("merged_{}_{}", target.name, joining.name)
}

/// Return whether the engine object class is Squad, Building or Unit
/// (`BDatabase_getObjectClassFromName` values 1, 2 and 3).
fn has_implicit_squad_class(object: &ProtoObject) -> bool {
    object.object_class.as_deref().is_some_and(|class| {
        ["Squad", "Building", "Unit"]
            .iter()
            .any(|expected| class.eq_ignore_ascii_case(expected))
    })
}

/// Return the unit count of a `<Squad>` record: the sum of the counts of the
/// unit entries whose object name resolves (`BProtoSquad::load`).
pub(crate) fn squad_size(database: &Database, squad: &ProtoSquad) -> u32 {
    let (_, total) = resolved_units(database, squad, None);
    u32::try_from(total.max(0)).unwrap_or(u32::MAX)
}

/// Resolve a proto object name with the engine object name map
/// (`sub_1400348D0`). Do not trim either name.
fn object_index_untrimmed(database: &Database, name: &str) -> Option<usize> {
    database
        .objects
        .iter()
        .position(|object| object.name.eq_ignore_ascii_case(name))
}

/// Find the first object prototype with this name. Do not trim either name.
pub(crate) fn object_by_name_untrimmed<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    object_index_untrimmed(database, name).map(|index| &database.objects[index])
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

fn squad_entry_by_name_untrimmed<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoSquad)> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(_, squad)| squad.name.eq_ignore_ascii_case(name))
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
    squad_entry_by_name_untrimmed(database, name.trim())
}

#[cfg(test)]
mod tests;
