//! Deterministic direct-placement and virtual child-socket resolution.

use super::ConstructionError;
use crate::entity::Entity;
use crate::world::sockets::{authored_socket_transform, socket_transform};
use crate::{EntityId, World};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::{ChildObjectType, ProtoObject};

const SOURCE_SOCKET_INDEX: u16 = u16::MAX;

#[derive(Debug, Clone, Copy)]
pub(super) struct SocketChoice {
    pub(super) position: Vec3,
    pub(super) forward: Vec3,
    pub(super) entity_id: Option<EntityId>,
    pub(super) index: Option<u16>,
}

pub(super) fn direct_build_transform(
    world: &World,
    database: &Database,
    position: Vec3,
    socket_id: EntityId,
    prototype_id: i32,
) -> Result<SocketChoice, ConstructionError> {
    if !position.is_finite() {
        return Err(ConstructionError::InvalidTransform);
    }
    let target = object_by_runtime_id(database, prototype_id)
        .ok_or(ConstructionError::PrototypeNotFound(prototype_id))?;
    if !socket_id.is_invalid()
        && let Some(socket) = world.get_unit(socket_id)
    {
        if !world.socket_is_empty(socket_id) {
            return Err(socket_unavailable(socket_id, &target.name));
        }
        let socket_proto = object_by_name(database, &socket.proto_object_name);
        let (position, forward) = socket_proto.map_or(
            socket_transform(socket.base.position, socket.base.forward, Vec3::ZERO, 0.0),
            |socket_proto| {
                apply_build_transform(
                    target,
                    socket.base.position,
                    socket.base.forward,
                    socket_proto,
                )
            },
        );
        return Ok(SocketChoice {
            position,
            forward,
            entity_id: Some(socket_id),
            index: None,
        });
    }
    Ok(SocketChoice {
        position,
        forward: Vec3::Z,
        entity_id: None,
        index: None,
    })
}

pub(super) fn find_build_other_socket(
    world: &World,
    builder_id: EntityId,
    database: &Database,
    prototype_id: i32,
    prototype_name: &str,
) -> Result<SocketChoice, ConstructionError> {
    let builder = world
        .get_building(builder_id)
        .ok_or(ConstructionError::BuilderNotFound(builder_id))?;
    let target = object_by_runtime_id(database, prototype_id)
        .ok_or(ConstructionError::PrototypeNotFound(prototype_id))?;
    let required_type = target
        .socket
        .as_ref()
        .map(|socket| socket.object_type.trim())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| socket_unavailable(builder_id, prototype_name))?;
    let builder_proto = object_by_name(database, &builder.proto_object_name)
        .ok_or_else(|| socket_unavailable(builder_id, prototype_name))?;
    if proto_has_type(builder_proto, required_type)
        && world.socket_is_empty(builder_id)
        && !virtual_socket_is_occupied(world, builder_id, SOURCE_SOCKET_INDEX)
    {
        let (position, forward) = apply_build_transform(
            target,
            builder.base.position,
            builder.base.forward,
            builder_proto,
        );
        return Ok(SocketChoice {
            position,
            forward,
            entity_id: Some(builder_id),
            index: Some(SOURCE_SOCKET_INDEX),
        });
    }
    if !builder.associated_sockets().is_empty() {
        for &socket_id in builder.associated_sockets() {
            let Some(socket) = world.get_unit(socket_id) else {
                continue;
            };
            let Some(socket_proto) = object_by_name(database, &socket.proto_object_name) else {
                continue;
            };
            if !proto_has_type(socket_proto, required_type) || !world.socket_is_empty(socket_id) {
                continue;
            }
            let (position, forward) = apply_build_transform(
                target,
                socket.base.position,
                socket.base.forward,
                socket_proto,
            );
            return Ok(SocketChoice {
                position,
                forward,
                entity_id: Some(socket_id),
                index: None,
            });
        }
        return Err(socket_unavailable(builder_id, prototype_name));
    }
    for (index, child) in builder_proto
        .child_objects
        .as_ref()
        .map_or(&[][..], |children| children.objects.as_slice())
        .iter()
        .enumerate()
    {
        if child.child_type != Some(ChildObjectType::Socket) {
            continue;
        }
        let Some(socket_proto) = object_by_name(database, child.proto_object.trim()) else {
            continue;
        };
        let Some(index) = u16::try_from(index).ok() else {
            continue;
        };
        if !proto_has_type(socket_proto, required_type)
            || virtual_socket_is_occupied(world, builder_id, index)
        {
            continue;
        }
        let (socket_position, socket_forward) = authored_socket_transform(
            builder.base.position,
            builder.base.forward,
            child.offset,
            child.rotation,
        );
        let (position, forward) =
            apply_build_transform(target, socket_position, socket_forward, socket_proto);
        return Ok(SocketChoice {
            position,
            forward,
            entity_id: None,
            index: Some(index),
        });
    }
    Err(socket_unavailable(builder_id, prototype_name))
}

fn virtual_socket_is_occupied(world: &World, builder_id: EntityId, socket_index: u16) -> bool {
    world.units.iter().any(|(_, unit)| {
        unit.is_alive()
            && unit.built_by == Some(builder_id)
            && unit.build_socket_index == Some(socket_index)
    })
}

fn apply_build_transform(
    target: &ProtoObject,
    socket_position: Vec3,
    socket_forward: Vec3,
    socket_proto: &ProtoObject,
) -> (Vec3, Vec3) {
    if has_flag(&socket_proto.flags, "UseBuildRotation") {
        return authored_socket_transform(
            socket_position,
            socket_forward,
            target.build_offset,
            target.build_rotation,
        );
    }
    socket_transform(socket_position, socket_forward, Vec3::ZERO, 0.0)
}

fn socket_unavailable(builder_id: EntityId, prototype: &str) -> ConstructionError {
    ConstructionError::SocketUnavailable {
        builder_id,
        prototype: prototype.to_owned(),
    }
}

fn object_by_runtime_id(database: &Database, prototype_id: i32) -> Option<&ProtoObject> {
    usize::try_from(prototype_id)
        .ok()
        .and_then(|index| database.objects.get(index))
}

fn object_by_name<'a>(database: &'a Database, name: &str) -> Option<&'a ProtoObject> {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name.trim()))
}

fn proto_has_type(prototype: &ProtoObject, expected: &str) -> bool {
    prototype
        .object_types
        .iter()
        .any(|object_type| object_type.trim().eq_ignore_ascii_case(expected))
}

fn has_flag(flags: &[String], expected: &str) -> bool {
    flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}
