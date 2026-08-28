//! Retail conditions for associated construction and turret sockets.

use super::{Condition, TriggerScript, TriggerValue, World, as_i32, value_at, write_trigger_value};
use crate::entity_id::EntityId;
use crate::trigger::VarId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SocketKind {
    Building,
    Turret,
}

pub(super) fn can_get_socket_units(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    if !matches!(condition.version, 1 | 2) {
        return false;
    }
    let only_empty = condition.version == 2
        && value_at(condition, script, 4)
            .and_then(TriggerValue::as_bool)
            .unwrap_or(false);
    let mut building_sockets = Vec::new();
    let mut turret_sockets = Vec::new();
    if let Some(source) = source_unit(condition, script, world) {
        for &socket_id in source.associated_sockets() {
            if only_empty && !world.socket_is_empty(socket_id) {
                continue;
            }
            match socket_kind(world, socket_id) {
                Some(SocketKind::Building) => unique_add(&mut building_sockets, socket_id),
                Some(SocketKind::Turret) => unique_add(&mut turret_sockets, socket_id),
                None => {}
            }
        }
    }
    write_used(
        condition,
        script,
        2,
        TriggerValue::UnitList(building_sockets.clone()),
    );
    write_used(
        condition,
        script,
        3,
        TriggerValue::UnitList(turret_sockets.clone()),
    );
    !building_sockets.is_empty() || !turret_sockets.is_empty()
}

pub(super) fn can_get_one_socket_unit(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    if condition.version != 1 {
        return false;
    }
    let mut building_index = value_at(condition, script, 2)
        .and_then(as_i32)
        .unwrap_or(-1);
    let mut turret_index = value_at(condition, script, 3)
        .and_then(as_i32)
        .unwrap_or(-1);
    let mut building_socket = None;
    let mut turret_socket = None;
    if (building_index != -1 || turret_index != -1)
        && let Some(source) = source_unit(condition, script, world)
    {
        for &socket_id in source.associated_sockets() {
            match socket_kind(world, socket_id) {
                Some(SocketKind::Building) => {
                    if building_index == 0 {
                        building_socket = Some(socket_id);
                    }
                    if building_index >= 0 {
                        building_index -= 1;
                    }
                }
                Some(SocketKind::Turret) => {
                    if turret_index == 0 {
                        turret_socket = Some(socket_id);
                    }
                    if turret_index >= 0 {
                        turret_index -= 1;
                    }
                }
                None => {}
            }
        }
    }
    write_used(
        condition,
        script,
        4,
        TriggerValue::Unit(building_socket.unwrap_or(EntityId::INVALID)),
    );
    write_used(
        condition,
        script,
        5,
        TriggerValue::Unit(turret_socket.unwrap_or(EntityId::INVALID)),
    );
    building_socket.is_some() || turret_socket.is_some()
}

pub(super) fn is_empty_socket_unit(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> bool {
    let Some(socket_id) = unit_id_at(condition, script, 1) else {
        return false;
    };
    let recognized_socket = socket_kind(world, socket_id).is_some()
        || world.unit_object_type_match(socket_id, "Settlement") == Some(true);
    recognized_socket && world.socket_is_empty(socket_id)
}

pub(super) fn can_get_socket_parent_building(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    let parent_id = unit_id_at(condition, script, 1)
        .filter(|&socket_id| socket_kind(world, socket_id).is_some())
        .and_then(|socket_id| world.get_unit(socket_id))
        .and_then(crate::entities::Unit::socket_parent)
        .filter(|&parent_id| world.get_unit(parent_id).is_some());
    write_used(
        condition,
        script,
        2,
        TriggerValue::Unit(parent_id.unwrap_or(EntityId::INVALID)),
    );
    parent_id.is_some()
}

pub(super) fn can_get_socket_plug_unit(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    let plug_id =
        unit_id_at(condition, script, 1).and_then(|socket_id| world.socket_plug(socket_id));
    write_used(
        condition,
        script,
        2,
        TriggerValue::Unit(plug_id.unwrap_or(EntityId::INVALID)),
    );
    plug_id.is_some()
}

fn source_unit<'world>(
    condition: &Condition,
    script: &TriggerScript,
    world: &'world World,
) -> Option<&'world crate::entities::Unit> {
    let TriggerValue::Unit(unit_id) = value_at(condition, script, 1)? else {
        return None;
    };
    world.get_unit(*unit_id)
}

fn unit_id_at(
    condition: &Condition,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<EntityId> {
    let TriggerValue::Unit(unit_id) = value_at(condition, script, signature_id)? else {
        return None;
    };
    (!unit_id.is_invalid()).then_some(*unit_id)
}

fn socket_kind(world: &World, socket_id: EntityId) -> Option<SocketKind> {
    if world.unit_object_type_match(socket_id, "BuildingSocket") == Some(true) {
        Some(SocketKind::Building)
    } else if world.unit_object_type_match(socket_id, "TurretSocket") == Some(true) {
        Some(SocketKind::Turret)
    } else {
        None
    }
}

fn unique_add(values: &mut Vec<EntityId>, value: EntityId) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn write_used(
    condition: &Condition,
    script: &mut TriggerScript,
    signature_id: u16,
    value: TriggerValue,
) {
    let Some(variable_id) = used_variable_id(condition, script, signature_id) else {
        return;
    };
    write_trigger_value(script, variable_id, value);
}

fn used_variable_id(
    condition: &Condition,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<VarId> {
    let variable_id = condition.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .is_some_and(|variable| !variable.is_null)
        .then_some(variable_id)
}

#[cfg(test)]
mod tests;
