//! Retail spatial query and authoritative transform effects.

mod random_location;
pub(super) use random_location::random_location;

use super::support::{
    EntityListKind, bool_at, entities_at, unique_add, used_variable_id, variable_is_used, vector_at,
};
use super::{EffectOutcome, write_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue, TriggerVec3};
use crate::{EntityId, World};
use glam::Vec3;
use num_traits::ToPrimitive;

const INVALID_VECTOR: Vec3 = Vec3::new(-1.0, -1.0, -1.0);

pub(super) fn set_playable_bounds(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(first) = vector_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = vector_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    if world.set_playable_bounds(first, second) {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

pub(super) fn get_location(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let unit_used = variable_is_used(effect, script, 1);
    let squad_used = variable_is_used(effect, script, 2);
    let object_used = effect.version == 2 && variable_is_used(effect, script, 4);
    if !unit_used && !squad_used && !object_used {
        return EffectOutcome::Skipped;
    }

    let position = if effect.version == 1 {
        unit_used
            .then(|| scalar_entity(effect, script, 1, EntityListKind::Unit))
            .flatten()
            .and_then(|entity_id| world.entity_position(entity_id))
            .or_else(|| {
                squad_used
                    .then(|| scalar_entity(effect, script, 2, EntityListKind::Squad))
                    .flatten()
                    .and_then(|entity_id| world.entity_position(entity_id))
            })
    } else if unit_used {
        scalar_entity(effect, script, 1, EntityListKind::Unit)
            .and_then(|entity_id| world.entity_position(entity_id))
    } else if squad_used {
        scalar_entity(effect, script, 2, EntityListKind::Squad)
            .and_then(|entity_id| world.entity_position(entity_id))
    } else {
        scalar_entity(effect, script, 4, EntityListKind::Object)
            .and_then(|entity_id| world.entity_position(entity_id))
    };
    if let Some(position) = position {
        write_vector(effect, script, 3, position);
    }
    EffectOutcome::Applied
}

pub(super) fn get_mean_location(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(input) = mean_input(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let mean = match input {
        MeanInput::Entities(kind, entities) => {
            mean_entity_positions(world, kind, &entities, effect.version)
        }
        MeanInput::Locations(locations) => mean_locations(&locations),
    };
    write_vector(effect, script, 4, mean);
    EffectOutcome::Applied
}

#[derive(Debug)]
enum MeanInput {
    Entities(EntityListKind, Vec<EntityId>),
    Locations(Vec<Vec3>),
}

fn mean_input(effect: &Effect, script: &TriggerScript) -> Option<MeanInput> {
    for (slot, kind) in [
        (1, EntityListKind::Unit),
        (2, EntityListKind::Squad),
        (3, EntityListKind::Object),
    ] {
        if variable_is_used(effect, script, slot) {
            return Some(MeanInput::Entities(
                kind,
                entities_at(effect, script, slot, kind).unwrap_or_default(),
            ));
        }
    }
    if effect.version == 2 && variable_is_used(effect, script, 5) {
        return Some(MeanInput::Locations(
            location_list_at(effect, script, 5).unwrap_or_default(),
        ));
    }
    None
}

fn mean_entity_positions(
    world: &World,
    kind: EntityListKind,
    entities: &[EntityId],
    version: u8,
) -> Vec3 {
    if entities.is_empty() {
        return INVALID_VECTOR;
    }
    let mut sum = Vec3::ZERO;
    let mut valid = 0_u32;
    for &entity_id in entities {
        if let Some(position) = mean_entity_position(world, entity_id, kind) {
            sum += position;
            valid += 1;
        }
    }
    let divisor = if version == 1 {
        entities
            .len()
            .to_f32()
            .expect("a retail entity list length has an f32 representation")
    } else {
        valid
            .to_f32()
            .expect("a retail valid-entity count has an f32 representation")
    };
    sum / divisor
}

fn mean_entity_position(world: &World, entity_id: EntityId, kind: EntityListKind) -> Option<Vec3> {
    match kind {
        EntityListKind::Unit => world.get_unit(entity_id).map(|unit| unit.base.position),
        EntityListKind::Squad => squad_average_position(world, entity_id),
        EntityListKind::Object => world
            .get_object(entity_id)
            .map(|object| object.base.position)
            .or_else(|| world.get_unit(entity_id).map(|unit| unit.base.position))
            .or_else(|| {
                world
                    .get_projectile(entity_id)
                    .map(|projectile| projectile.base.position)
            }),
    }
}

fn squad_average_position(world: &World, squad_id: EntityId) -> Option<Vec3> {
    let squad = world.get_squad(squad_id)?;
    let mut sum = Vec3::ZERO;
    let mut count = 0_u32;
    for &unit_id in &squad.unit_ids {
        if let Some(unit) = world.get_unit(unit_id) {
            sum += unit.base.position;
            count += 1;
        }
    }
    Some(if count == 0 {
        Vec3::ZERO
    } else {
        sum / count
            .to_f32()
            .expect("a retail squad child count has an f32 representation")
    })
}

fn mean_locations(locations: &[Vec3]) -> Vec3 {
    if locations.is_empty() {
        return INVALID_VECTOR;
    }
    let count = locations
        .len()
        .to_f32()
        .expect("a retail location-list length has an f32 representation");
    locations.iter().copied().sum::<Vec3>() / count
}

fn location_list_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<Vec<Vec3>> {
    let (TriggerValue::LocationList(values) | TriggerValue::VectorList(values)) =
        super::value_at(effect, script, signature_id)?
    else {
        return None;
    };
    Some(
        values
            .iter()
            .map(|value| Vec3::new(value.x, value.y, value.z))
            .collect(),
    )
}

pub(super) fn get_direction(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let selected = [
        (1, EntityListKind::Unit),
        (2, EntityListKind::Squad),
        (3, EntityListKind::Object),
    ]
    .into_iter()
    .find_map(|(slot, kind)| {
        variable_is_used(effect, script, slot)
            .then(|| scalar_entity(effect, script, slot, kind))
            .flatten()
    });
    let forward = selected
        .and_then(|entity_id| world.entity_forward(entity_id))
        .unwrap_or(INVALID_VECTOR);
    if effect.version == 1 {
        write_vector(effect, script, 4, forward);
        return EffectOutcome::Applied;
    }
    let right = if forward == INVALID_VECTOR {
        INVALID_VECTOR
    } else {
        Vec3::Y.cross(forward).normalize_or_zero()
    };
    let up = if forward == INVALID_VECTOR {
        INVALID_VECTOR
    } else {
        forward.cross(right).normalize_or_zero()
    };
    write_vector(effect, script, 4, forward);
    write_vector(effect, script, 5, right);
    write_vector(effect, script, 6, up);
    EffectOutcome::Applied
}

pub(super) fn get_direction_from_locations(
    effect: &Effect,
    script: &mut TriggerScript,
) -> EffectOutcome {
    let Some(first) = vector_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(second) = vector_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    write_vector(effect, script, 4, (second - first).normalize_or_zero());
    EffectOutcome::Applied
}

pub(super) fn set_direction(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if effect.version == 2 && bool_at(effect, script, 8).unwrap_or(false) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(direction) = vector_at(effect, script, 7).filter(|value| value.is_finite()) else {
        return EffectOutcome::Skipped;
    };
    let entities = if effect.version == 1 {
        let Some(entities) = direction_targets_v1(effect, script, world) else {
            return EffectOutcome::Skipped;
        };
        entities
    } else {
        direction_targets_v2(effect, script, world)
    };
    for entity_id in entities {
        let _set = world.set_entity_forward(entity_id, direction);
    }
    EffectOutcome::Applied
}

pub(super) fn teleport(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if !matches!(effect.version, 2 | 3) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(location) = vector_at(effect, script, 5).filter(|value| value.is_finite()) else {
        return EffectOutcome::Skipped;
    };
    let Some((squads, objects)) = teleport_targets(effect, script, world) else {
        return EffectOutcome::Skipped;
    };
    let has_live_squad = squads
        .iter()
        .any(|squad_id| world.get_squad(*squad_id).is_some());
    let ignore_plot = effect.version == 3 && bool_at(effect, script, 8).unwrap_or(false);
    if has_live_squad && !ignore_plot {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    for squad_id in squads {
        let _teleported = world.teleport_squad(squad_id, location);
    }
    for object_id in objects {
        let _teleported = world.teleport_object(object_id, location);
    }
    EffectOutcome::Applied
}

fn direction_targets_v1(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
) -> Option<Vec<EntityId>> {
    if !(1..=6).any(|slot| variable_is_used(effect, script, slot)) {
        return None;
    }
    let mut entities = entities_at(effect, script, 2, EntityListKind::Unit).unwrap_or_default();
    if let Some(unit_id) = scalar_entity(effect, script, 1, EntityListKind::Unit) {
        unique_add(&mut entities, unit_id);
    }
    for squad_id in entities_at(effect, script, 4, EntityListKind::Squad).unwrap_or_default() {
        add_squad_and_members(world, squad_id, &mut entities);
    }
    if let Some(squad_id) = scalar_entity(effect, script, 3, EntityListKind::Squad) {
        add_squad_and_members(world, squad_id, &mut entities);
    }
    entities.extend(entities_at(effect, script, 6, EntityListKind::Object).unwrap_or_default());
    if let Some(object_id) = scalar_entity(effect, script, 5, EntityListKind::Object) {
        unique_add(&mut entities, object_id);
    }
    Some(entities)
}

fn direction_targets_v2(effect: &Effect, script: &TriggerScript, world: &World) -> Vec<EntityId> {
    let mut objects = entities_at(effect, script, 6, EntityListKind::Object).unwrap_or_default();
    if let Some(object_id) = scalar_entity(effect, script, 5, EntityListKind::Object) {
        unique_add(&mut objects, object_id);
    }
    for unit_id in entities_at(effect, script, 2, EntityListKind::Unit).unwrap_or_default() {
        if world.get_unit(unit_id).is_some() {
            unique_add(&mut objects, unit_id);
        }
    }
    if let Some(unit_id) = scalar_entity(effect, script, 1, EntityListKind::Unit)
        && world.get_unit(unit_id).is_some()
    {
        unique_add(&mut objects, unit_id);
    }
    for squad_id in entities_at(effect, script, 4, EntityListKind::Squad).unwrap_or_default() {
        add_squad_members(world, squad_id, &mut objects);
    }
    if let Some(squad_id) = scalar_entity(effect, script, 3, EntityListKind::Squad) {
        add_squad_members(world, squad_id, &mut objects);
    }
    objects
}

fn teleport_targets(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
) -> Option<(Vec<EntityId>, Vec<EntityId>)> {
    let target_used = if effect.version == 2 {
        [1, 2, 3, 4, 6, 7]
            .into_iter()
            .any(|slot| variable_is_used(effect, script, slot))
    } else {
        [3, 4, 6, 7]
            .into_iter()
            .any(|slot| variable_is_used(effect, script, slot))
    };
    if !target_used {
        return None;
    }
    let mut squads = entities_at(effect, script, 4, EntityListKind::Squad).unwrap_or_default();
    if let Some(squad_id) = scalar_entity(effect, script, 3, EntityListKind::Squad) {
        unique_add(&mut squads, squad_id);
    }
    if effect.version == 2 {
        let mut units = entities_at(effect, script, 2, EntityListKind::Unit).unwrap_or_default();
        if let Some(unit_id) = scalar_entity(effect, script, 1, EntityListKind::Unit) {
            unique_add(&mut units, unit_id);
        }
        for unit_id in units {
            if let Some(squad_id) = world.get_unit(unit_id).and_then(|unit| unit.squad_id) {
                unique_add(&mut squads, squad_id);
            }
        }
    }
    let mut objects = entities_at(effect, script, 7, EntityListKind::Object).unwrap_or_default();
    if let Some(object_id) = scalar_entity(effect, script, 6, EntityListKind::Object) {
        unique_add(&mut objects, object_id);
    }
    Some((squads, objects))
}

fn add_squad_and_members(world: &World, squad_id: EntityId, entities: &mut Vec<EntityId>) {
    let Some(squad) = world.get_squad(squad_id) else {
        return;
    };
    unique_add(entities, squad_id);
    for unit_id in &squad.unit_ids {
        unique_add(entities, *unit_id);
    }
}

fn add_squad_members(world: &World, squad_id: EntityId, entities: &mut Vec<EntityId>) {
    let Some(squad) = world.get_squad(squad_id) else {
        return;
    };
    for unit_id in &squad.unit_ids {
        unique_add(entities, *unit_id);
    }
}

fn scalar_entity(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<EntityId> {
    entities_at(effect, script, signature_id, kind)?
        .into_iter()
        .next()
}

fn write_vector(effect: &Effect, script: &mut TriggerScript, signature_id: u16, value: Vec3) {
    if let Some(variable_id) = used_variable_id(effect, script, signature_id) {
        let _outcome = write_value(
            script,
            variable_id,
            TriggerValue::Vector(TriggerVec3::new(value.x, value.y, value.z)),
        );
    }
}

#[cfg(test)]
mod tests;
