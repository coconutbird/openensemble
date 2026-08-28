//! Projection of authored child sockets into authoritative unit entities.

use super::{
    PlacedUnitKind, classify_proto_object, configure_unit_from_proto, find_proto_object,
    prototypes::prototype_has_flag,
};
use crate::entity_id::EntityId;
use crate::world::World;
use crate::world::sockets::authored_socket_transform;
use glam::Vec3;
use pipeline::database::hw1::objects::{ChildObjectType, ProtoObject};
use pipeline::database::hw1::{Database, Vector3};

const MAX_SOCKET_DEPTH: usize = 16;

pub(super) fn materialize_authored_sockets(
    world: &mut World,
    parent_id: EntityId,
    parent_proto: &ProtoObject,
    database: &Database,
) {
    let mut ancestors = vec![parent_proto.name.to_ascii_lowercase()];
    materialize_recursive(world, parent_id, parent_proto, database, &mut ancestors);
}

fn materialize_recursive(
    world: &mut World,
    parent_id: EntityId,
    parent_proto: &ProtoObject,
    database: &Database,
    ancestors: &mut Vec<String>,
) {
    if ancestors.len() > MAX_SOCKET_DEPTH {
        return;
    }
    let Some((player_id, parent_position, parent_forward, parent_built)) =
        world.get_unit(parent_id).map(|parent| {
            (
                parent.base.player_id,
                parent.base.position,
                parent.base.forward,
                parent.built,
            )
        })
    else {
        return;
    };
    let children = parent_proto
        .child_objects
        .as_ref()
        .map_or(&[][..], |children| children.objects.as_slice());
    for child in children {
        if child.child_type != Some(ChildObjectType::Socket) {
            continue;
        }
        let Some((prototype_index, prototype)) =
            find_proto_object(database, child.proto_object.trim())
        else {
            continue;
        };
        let prototype_key = prototype.name.to_ascii_lowercase();
        if ancestors.contains(&prototype_key) {
            continue;
        }
        let local_offset = child.offset.map_or(Vec3::ZERO, vector_to_vec3);
        let local_yaw = child
            .rotation
            .filter(|value| value.is_finite())
            .unwrap_or(0.0);
        let (position, forward) = authored_socket_transform(
            parent_position,
            parent_forward,
            child.offset,
            child.rotation,
        );
        let socket_id = match classify_proto_object(prototype) {
            Some(PlacedUnitKind::Building) => world.create_building_at(player_id, position),
            _ => world.create_unit_at(player_id, position),
        };
        configure_unit_from_proto(
            world,
            socket_id,
            prototype.name.trim(),
            prototype_index,
            prototype,
        );
        if let Some(socket) = world.get_unit_mut(socket_id) {
            socket.base.set_forward(forward);
            if prototype_has_flag(prototype, "NotSelectableWhenChildObject") {
                socket.base.set_selectable(false);
            }
            socket.built = parent_built;
            socket.built_by = Some(parent_id);
        }
        if !world.associate_socket_at(parent_id, socket_id, local_offset, local_yaw) {
            let _removed = world.remove_unit(socket_id);
            continue;
        }
        ancestors.push(prototype_key);
        materialize_recursive(world, socket_id, prototype, database, ancestors);
        let _popped = ancestors.pop();
    }
}

const fn vector_to_vec3(value: Vector3) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::objects::{ChildObject, ChildObjects};

    #[test]
    fn authored_socket_entities_keep_order_transform_and_prototype_types() {
        let database = Database {
            objects: vec![
                ProtoObject {
                    name: "parent".to_owned(),
                    object_class: Some("Building".to_owned()),
                    child_objects: Some(ChildObjects {
                        objects: vec![
                            socket_child("building_socket", 2.0, 3.0, 90.0),
                            socket_child("turret_socket", -4.0, 1.0, -90.0),
                        ],
                    }),
                    ..ProtoObject::default()
                },
                socket_prototype("building_socket", "BuildingSocket"),
                socket_prototype("turret_socket", "TurretSocket"),
            ],
            ..Database::default()
        };
        let mut world = World::new();
        let parent_id = world.create_building_at(1, Vec3::new(10.0, 0.0, 20.0));
        world
            .get_unit_mut(parent_id)
            .unwrap()
            .base
            .set_forward(Vec3::X);

        materialize_authored_sockets(&mut world, parent_id, &database.objects[0], &database);

        let socket_ids = world
            .get_unit(parent_id)
            .unwrap()
            .associated_sockets()
            .to_vec();
        assert_eq!(socket_ids.len(), 2);
        let building_socket = world.get_unit(socket_ids[0]).unwrap();
        assert!(building_socket.is_object_type("BuildingSocket"));
        assert!(
            building_socket
                .base
                .position
                .abs_diff_eq(Vec3::new(13.0, 0.0, 18.0), 1.0e-6)
        );
        let turret_socket = world.get_unit(socket_ids[1]).unwrap();
        assert!(turret_socket.is_object_type("TurretSocket"));
        assert!(
            turret_socket
                .base
                .position
                .abs_diff_eq(Vec3::new(11.0, 0.0, 24.0), 1.0e-6)
        );
    }

    fn socket_child(name: &str, x: f32, z: f32, rotation: f32) -> ChildObject {
        ChildObject {
            proto_object: name.to_owned(),
            child_type: Some(ChildObjectType::Socket),
            offset: Some(Vector3 { x, y: 0.0, z }),
            rotation: Some(rotation),
            ..ChildObject::default()
        }
    }

    fn socket_prototype(name: &str, object_type: &str) -> ProtoObject {
        ProtoObject {
            name: name.to_owned(),
            object_class: Some("Building".to_owned()),
            object_types: vec![object_type.to_owned()],
            ..ProtoObject::default()
        }
    }
}
