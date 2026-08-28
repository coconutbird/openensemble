//! Authoritative socket associations, plug relationships, and transforms.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use glam::{Quat, Vec3};
use pipeline::database::hw1::Vector3;

impl World {
    /// Associate an existing socket unit with a parent, preserving its current transform.
    pub fn associate_socket(&mut self, parent_id: EntityId, socket_id: EntityId) -> bool {
        let Some((parent_position, parent_forward)) =
            self.units.get(parent_id).and_then(|parent| {
                parent
                    .is_alive()
                    .then_some((parent.base.position, parent.base.forward))
            })
        else {
            return false;
        };
        let Some((socket_position, socket_forward)) =
            self.units.get(socket_id).and_then(|socket| {
                socket
                    .is_alive()
                    .then_some((socket.base.position, socket.base.forward))
            })
        else {
            return false;
        };
        let local_offset = world_offset_to_local(
            socket_position - parent_position,
            normalized_forward(parent_forward),
        );
        let local_yaw_degrees = relative_yaw_degrees(parent_forward, socket_forward);
        self.associate_socket_at(parent_id, socket_id, local_offset, local_yaw_degrees)
    }

    pub(crate) fn associate_socket_at(
        &mut self,
        parent_id: EntityId,
        socket_id: EntityId,
        local_offset: Vec3,
        local_yaw_degrees: f32,
    ) -> bool {
        if parent_id == socket_id
            || !local_offset.is_finite()
            || !local_yaw_degrees.is_finite()
            || self.socket_parent_chain_contains(parent_id, socket_id)
        {
            return false;
        }
        let Some(parent) = self.units.get(parent_id) else {
            return false;
        };
        let parent_transform = (parent.base.position, parent.base.forward);
        if !parent.is_alive() || !self.units.get(socket_id).is_some_and(Entity::is_alive) {
            return false;
        }

        let old_parent_id = self
            .units
            .get(socket_id)
            .and_then(|socket| socket.socket_parent_id);
        if let Some(old_parent_id) = old_parent_id
            && old_parent_id != parent_id
            && let Some(old_parent) = self.units.get_mut(old_parent_id)
        {
            old_parent
                .associated_socket_ids
                .retain(|&candidate| candidate != socket_id);
        }
        if let Some(parent) = self.units.get_mut(parent_id)
            && !parent.associated_socket_ids.contains(&socket_id)
        {
            parent.associated_socket_ids.push(socket_id);
        }
        let (position, forward) = socket_transform(
            parent_transform.0,
            parent_transform.1,
            local_offset,
            local_yaw_degrees,
        );
        let Some(socket) = self.units.get_mut(socket_id) else {
            return false;
        };
        socket.socket_parent_id = Some(parent_id);
        socket.socket_local_offset = local_offset;
        socket.socket_local_yaw_degrees = local_yaw_degrees;
        socket.base.position = position;
        socket.base.set_forward(forward);
        true
    }

    /// Establish the retail two-way parent-socket/socket-plug relationship.
    pub fn connect_socket_plug(&mut self, socket_id: EntityId, plug_id: EntityId) -> bool {
        if socket_id == plug_id
            || !self.units.get(socket_id).is_some_and(Entity::is_alive)
            || !self.units.get(plug_id).is_some_and(Entity::is_alive)
        {
            return false;
        }
        let old_plug_id = self
            .units
            .get(socket_id)
            .and_then(|socket| socket.socket_plug_id);
        let old_socket_id = self
            .units
            .get(plug_id)
            .and_then(|plug| plug.build_socket_id);
        if let Some(old_plug_id) = old_plug_id
            && let Some(old_plug) = self.units.get_mut(old_plug_id)
            && old_plug.build_socket_id == Some(socket_id)
        {
            old_plug.build_socket_id = None;
        }
        if let Some(old_socket_id) = old_socket_id
            && let Some(old_socket) = self.units.get_mut(old_socket_id)
            && old_socket.socket_plug_id == Some(plug_id)
        {
            old_socket.socket_plug_id = None;
        }
        if let Some(socket) = self.units.get_mut(socket_id) {
            socket.socket_plug_id = Some(plug_id);
        }
        if let Some(plug) = self.units.get_mut(plug_id) {
            plug.build_socket_id = Some(socket_id);
        }
        true
    }

    /// Return the live building currently plugged into a socket.
    #[must_use]
    pub fn socket_plug(&self, socket_id: EntityId) -> Option<EntityId> {
        let plug_id = self.units.get(socket_id)?.socket_plug_id?;
        self.units
            .get(plug_id)
            .is_some_and(|plug| plug.is_alive() && plug.build_socket_id == Some(socket_id))
            .then_some(plug_id)
    }

    /// Return whether a live socket has no live plug relationship.
    #[must_use]
    pub fn socket_is_empty(&self, socket_id: EntityId) -> bool {
        self.units.get(socket_id).is_some_and(Entity::is_alive)
            && self.socket_plug(socket_id).is_none()
    }

    pub(super) fn detach_unit_socket_refs(&mut self, unit_id: EntityId) {
        let Some((parent_id, children, parent_socket_id, plug_id)) =
            self.units.get(unit_id).map(|unit| {
                (
                    unit.socket_parent_id,
                    unit.associated_socket_ids.clone(),
                    unit.build_socket_id,
                    unit.socket_plug_id,
                )
            })
        else {
            return;
        };
        if let Some(parent_id) = parent_id
            && let Some(parent) = self.units.get_mut(parent_id)
        {
            parent
                .associated_socket_ids
                .retain(|&candidate| candidate != unit_id);
        }
        for child_id in children {
            if let Some(child) = self.units.get_mut(child_id)
                && child.socket_parent_id == Some(unit_id)
            {
                child.socket_parent_id = None;
            }
        }
        if let Some(parent_socket_id) = parent_socket_id
            && let Some(parent_socket) = self.units.get_mut(parent_socket_id)
            && parent_socket.socket_plug_id == Some(unit_id)
        {
            parent_socket.socket_plug_id = None;
        }
        if let Some(plug_id) = plug_id
            && let Some(plug) = self.units.get_mut(plug_id)
            && plug.build_socket_id == Some(unit_id)
        {
            plug.build_socket_id = None;
        }
    }

    pub(super) fn sync_associated_socket_transforms(&mut self) {
        let sockets = self
            .units
            .iter()
            .filter_map(|(socket_id, socket)| {
                socket.socket_parent_id.map(|parent_id| {
                    (
                        socket_id,
                        parent_id,
                        socket.socket_local_offset,
                        socket.socket_local_yaw_degrees,
                    )
                })
            })
            .collect::<Vec<_>>();
        for (socket_id, parent_id, local_offset, local_yaw_degrees) in sockets {
            let Some(parent) = self.units.get(parent_id) else {
                continue;
            };
            let (position, forward) = socket_transform(
                parent.base.position,
                parent.base.forward,
                local_offset,
                local_yaw_degrees,
            );
            let velocity = parent.base.velocity;
            if let Some(socket) = self.units.get_mut(socket_id) {
                socket.base.position = position;
                socket.base.set_forward(forward);
                socket.base.velocity = velocity;
            }
        }
    }

    fn socket_parent_chain_contains(&self, mut parent_id: EntityId, target: EntityId) -> bool {
        for _ in 0..=self.units.len() {
            if parent_id == target {
                return true;
            }
            let Some(next) = self
                .units
                .get(parent_id)
                .and_then(|parent| parent.socket_parent_id)
            else {
                return false;
            };
            parent_id = next;
        }
        true
    }
}

pub(crate) fn authored_socket_transform(
    parent_position: Vec3,
    parent_forward: Vec3,
    offset: Option<Vector3>,
    rotation_degrees: Option<f32>,
) -> (Vec3, Vec3) {
    let local_offset = offset.map_or(Vec3::ZERO, |value| Vec3::new(value.x, value.y, value.z));
    socket_transform(
        parent_position,
        parent_forward,
        local_offset,
        rotation_degrees
            .filter(|value| value.is_finite())
            .unwrap_or(0.0),
    )
}

pub(crate) fn socket_transform(
    parent_position: Vec3,
    parent_forward: Vec3,
    local_offset: Vec3,
    local_yaw_degrees: f32,
) -> (Vec3, Vec3) {
    let parent_forward = normalized_forward(parent_forward);
    let right = Vec3::Y.cross(parent_forward).normalize_or(Vec3::X);
    let position = parent_position
        + right * local_offset.x
        + Vec3::Y * local_offset.y
        + parent_forward * local_offset.z;
    let forward =
        normalized_forward(Quat::from_rotation_y(local_yaw_degrees.to_radians()) * parent_forward);
    (position, forward)
}

fn world_offset_to_local(offset: Vec3, parent_forward: Vec3) -> Vec3 {
    let right = Vec3::Y.cross(parent_forward).normalize_or(Vec3::X);
    Vec3::new(offset.dot(right), offset.y, offset.dot(parent_forward))
}

fn relative_yaw_degrees(parent_forward: Vec3, socket_forward: Vec3) -> f32 {
    let parent_forward = normalized_forward(parent_forward);
    let socket_forward = normalized_forward(socket_forward);
    let right = Vec3::Y.cross(parent_forward).normalize_or(Vec3::X);
    socket_forward
        .dot(right)
        .atan2(socket_forward.dot(parent_forward))
        .to_degrees()
}

fn normalized_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::Z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_associations_follow_parent_transforms_and_cleanup_both_directions() {
        let mut world = World::new();
        let parent_id = world.create_building_at(1, Vec3::new(10.0, 0.0, 20.0));
        let socket_id = world.create_building(1);
        let plug_id = world.create_building(1);
        assert!(world.associate_socket_at(parent_id, socket_id, Vec3::new(2.0, 1.0, 3.0), 90.0));
        assert!(world.connect_socket_plug(socket_id, plug_id));
        assert_eq!(world.socket_plug(socket_id), Some(plug_id));

        let parent = world.get_unit_mut(parent_id).unwrap();
        parent.base.position = Vec3::new(30.0, 2.0, 40.0);
        parent.base.set_forward(Vec3::X);
        world.sync_associated_socket_transforms();
        let socket = world.get_unit(socket_id).unwrap();
        assert!(
            socket
                .base
                .position
                .abs_diff_eq(Vec3::new(33.0, 3.0, 38.0), 1.0e-6)
        );
        assert!(socket.base.forward.abs_diff_eq(Vec3::NEG_Z, 1.0e-6));

        let _removed = world.remove_unit(socket_id);
        assert_eq!(world.get_unit(plug_id).unwrap().build_socket_id, None);
        assert!(
            world
                .get_unit(parent_id)
                .unwrap()
                .associated_sockets()
                .is_empty()
        );
    }

    #[test]
    fn socket_associations_reject_cycles() {
        let mut world = World::new();
        let first = world.create_unit(1);
        let second = world.create_unit(1);
        assert!(world.associate_socket(first, second));
        assert!(!world.associate_socket(second, first));
    }
}
