//! Database-backed runtime entity spawning.
//!
//! Scenario placement, debug create commands, and base production all use the
//! same prototype configuration path. This keeps runtime-created entities
//! identical to entities authored in an SCN file.

use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

use crate::entities::BaseId;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use crate::scenario::{create_object_from_prototype, create_squad_from_prototype};
use crate::world::World;

/// Maximum number of entities accepted from one debug create command.
pub const MAX_SPAWN_BATCH: u32 = 256;

const BASE_SPAWN_CLEARANCE: f32 = 8.0;

/// Failure to create an entity from authoritative game data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpawnError {
    /// The requested owner is not present in the world.
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    /// The requested base does not exist.
    #[error("base {0:?} does not exist")]
    BaseNotFound(BaseId),
    /// The base's anchor building is no longer valid.
    #[error("base {base_id:?} has no live anchor building {anchor_id:?}")]
    BaseAnchorMissing {
        /// Base whose anchor could not be resolved.
        base_id: BaseId,
        /// Stale anchor entity ID.
        anchor_id: EntityId,
    },
    /// No squad prototype has the requested database ID.
    #[error("squad prototype database ID {0} was not found")]
    SquadPrototypeNotFound(i32),
    /// No object prototype has the requested database ID.
    #[error("object prototype database ID {0} was not found")]
    ObjectPrototypeNotFound(i32),
    /// No squad prototype has the requested name.
    #[error("squad prototype '{0}' was not found")]
    SquadPrototypeNameNotFound(String),
    /// The object prototype is not a class-zero object, mobile unit, or building.
    #[error("object prototype database ID {0} is not spawnable")]
    ObjectPrototypeNotSpawnable(i32),
    /// A supplied position or facing contains non-finite values.
    #[error("spawn transform must contain finite values")]
    InvalidTransform,
    /// A batch would exceed the command safety limit.
    #[error("spawn count {count} exceeds the per-command limit of {MAX_SPAWN_BATCH}")]
    BatchTooLarge {
        /// Requested entity count.
        count: u32,
    },
}

/// Resolve the wire/database ID for a named squad prototype.
#[must_use]
pub fn squad_prototype_id(database: &Database, name: &str) -> Option<i32> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name.trim()))
        .map(|(index, proto)| database_id(proto.dbid, index))
}

/// Resolve the wire/database ID for a named object prototype.
#[must_use]
pub fn object_prototype_id(database: &Database, name: &str) -> Option<i32> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name.trim()))
        .map(|(index, proto)| database_id(proto.dbid, index))
}

/// Spawn one fully configured squad at a world transform.
///
/// # Errors
///
/// Returns an error for an unknown player/prototype or an invalid transform.
pub fn spawn_squad_at(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    proto_squad_id: i32,
    position: Vec3,
    forward: Vec3,
) -> Result<EntityId, SpawnError> {
    validate_spawn(world, player_id, position, forward)?;
    let proto = find_squad_by_id(database, proto_squad_id)
        .ok_or(SpawnError::SquadPrototypeNotFound(proto_squad_id))?;
    Ok(create_squad_from_prototype(
        world,
        player_id,
        position,
        forward,
        proto.name.trim(),
        database,
    ))
}

/// Spawn a bounded batch of fully configured squads at one world transform.
///
/// # Errors
///
/// Returns an error before creating anything if validation fails.
pub fn spawn_squads_at(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    proto_squad_id: i32,
    count: u32,
    position: Vec3,
    forward: Vec3,
) -> Result<Vec<EntityId>, SpawnError> {
    validate_batch(count)?;
    validate_spawn(world, player_id, position, forward)?;
    let proto = find_squad_by_id(database, proto_squad_id)
        .ok_or(SpawnError::SquadPrototypeNotFound(proto_squad_id))?;
    let mut spawned = Vec::with_capacity(usize::try_from(count).unwrap_or_default());
    for _ in 0..count {
        spawned.push(create_squad_from_prototype(
            world,
            player_id,
            position,
            forward,
            proto.name.trim(),
            database,
        ));
    }
    Ok(spawned)
}

/// Spawn one fully configured class-zero object, mobile unit, or building.
///
/// # Errors
///
/// Returns an error for an unknown player/prototype, unsupported object kind,
/// or invalid transform.
pub fn spawn_object_at(
    world: &mut World,
    database: &Database,
    player_id: PlayerId,
    proto_object_id: i32,
    position: Vec3,
    forward: Vec3,
) -> Result<EntityId, SpawnError> {
    validate_spawn(world, player_id, position, forward)?;
    let proto = find_object_by_id(database, proto_object_id)
        .ok_or(SpawnError::ObjectPrototypeNotFound(proto_object_id))?;
    create_object_from_prototype(
        world,
        player_id,
        position,
        forward,
        proto.name.trim(),
        database,
    )
    .ok_or(SpawnError::ObjectPrototypeNotSpawnable(proto_object_id))
}

/// Spawn a squad just beyond a base anchor's obstruction footprint.
///
/// The new squad is owned by the base owner. Position and facing are derived
/// exclusively from authoritative base/anchor state.
///
/// # Errors
///
/// Returns an error for an invalid base/anchor or squad prototype.
pub fn spawn_squad_from_base(
    world: &mut World,
    database: &Database,
    base_id: BaseId,
    proto_squad_id: i32,
) -> Result<EntityId, SpawnError> {
    let (player_id, position, forward) = base_spawn_transform(world, base_id)?;
    spawn_squad_at(
        world,
        database,
        player_id,
        proto_squad_id,
        position,
        forward,
    )
}

/// Resolve a squad prototype by name and spawn it from a base.
///
/// # Errors
///
/// Returns an error for an unknown prototype or invalid base state.
pub fn spawn_squad_from_base_by_name(
    world: &mut World,
    database: &Database,
    base_id: BaseId,
    proto_name: &str,
) -> Result<EntityId, SpawnError> {
    let proto_id = squad_prototype_id(database, proto_name)
        .ok_or_else(|| SpawnError::SquadPrototypeNameNotFound(proto_name.trim().to_owned()))?;
    spawn_squad_from_base(world, database, base_id, proto_id)
}

fn validate_spawn(
    world: &World,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
) -> Result<(), SpawnError> {
    if world.get_player(player_id).is_none() {
        return Err(SpawnError::PlayerNotFound(player_id));
    }
    if !position.is_finite() || !forward.is_finite() {
        return Err(SpawnError::InvalidTransform);
    }
    Ok(())
}

fn validate_batch(count: u32) -> Result<(), SpawnError> {
    if count > MAX_SPAWN_BATCH {
        return Err(SpawnError::BatchTooLarge { count });
    }
    Ok(())
}

fn base_spawn_transform(
    world: &World,
    base_id: BaseId,
) -> Result<(PlayerId, Vec3, Vec3), SpawnError> {
    let base = world
        .get_base(base_id)
        .ok_or(SpawnError::BaseNotFound(base_id))?;
    let anchor =
        world
            .get_building(base.anchor_building_id)
            .ok_or(SpawnError::BaseAnchorMissing {
                base_id,
                anchor_id: base.anchor_building_id,
            })?;
    let forward =
        Vec3::new(anchor.base.forward.x, 0.0, anchor.base.forward.z).normalize_or(Vec3::Z);
    let anchor_radius = anchor
        .obstruction_half_extents
        .x
        .abs()
        .max(anchor.obstruction_half_extents.z.abs());
    let position = base.position + forward * (anchor_radius + BASE_SPAWN_CLEARANCE);
    Ok((base.player_id, position, forward))
}

fn find_squad_by_id(database: &Database, id: i32) -> Option<&ProtoSquad> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(index, proto)| database_id(proto.dbid, *index) == id)
        .map(|(_, proto)| proto)
}

pub(crate) fn find_object_by_id(database: &Database, id: i32) -> Option<&ProtoObject> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(index, proto)| database_id(proto.dbid, *index) == id)
        .map(|(_, proto)| proto)
}

pub(crate) fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}
