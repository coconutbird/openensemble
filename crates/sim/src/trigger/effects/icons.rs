//! Retail effect 520 (`CreateIconObject`) versions 2 and 3.

use super::support::{bool_at, player_at, used_variable_id, vector_at};
use super::{EffectOutcome, value_at, write_value};
use crate::spawn::object_prototype_id;
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::{EntityId, World};
use pipeline::database::hw1::Database;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> Option<EffectOutcome> {
    (effect.effect_type == EffectType::CreateIconObject)
        .then(|| create(effect, script, world, database))
}

fn create(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 2 | 3) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let color = color_at(effect, script, 4);
    let force_visible_to_all = effect.version == 3 && bool_at(effect, script, 10).unwrap_or(false);
    let created = player_at(effect, script, 2)
        .zip(vector_at(effect, script, 3))
        .zip(icon_prototype_id(effect, script, database))
        .and_then(|((player_id, position), prototype_id)| {
            world.create_icon_object(
                database,
                player_id,
                prototype_id,
                position,
                color,
                force_visible_to_all,
            )
        });

    write_object_output(effect, script, created);
    update_object_list(effect, script, created);
    EffectOutcome::Applied
}

fn icon_prototype_id(effect: &Effect, script: &TriggerScript, database: &Database) -> Option<i32> {
    match value_at(effect, script, 9)? {
        TriggerValue::String(name) => object_prototype_id(database, name),
        TriggerValue::ProtoObject(prototype_id) | TriggerValue::Int(prototype_id) => {
            Some(*prototype_id)
        }
        _ => None,
    }
}

fn color_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<[u8; 3]> {
    let TriggerValue::Color(color) = value_at(effect, script, slot)? else {
        return None;
    };
    Some([color.r, color.g, color.b])
}

fn write_object_output(effect: &Effect, script: &mut TriggerScript, created: Option<EntityId>) {
    let Some(output_id) = used_variable_id(effect, script, 6) else {
        return;
    };
    let _outcome = write_value(
        script,
        output_id,
        TriggerValue::Object(created.unwrap_or(EntityId::INVALID)),
    );
}

fn update_object_list(effect: &Effect, script: &mut TriggerScript, created: Option<EntityId>) {
    let Some(output_id) = used_variable_id(effect, script, 7) else {
        return;
    };
    let clear_existing = bool_at(effect, script, 5).unwrap_or(false);
    let Some(variable) = script.get_variable_mut(output_id) else {
        return;
    };
    let TriggerValue::ObjectList(values) = &mut variable.value else {
        return;
    };
    if clear_existing {
        values.clear();
    }
    if let Some(created) = created {
        values.push(created);
    }
    variable.is_null = false;
}

#[cfg(test)]
#[path = "icons/tests.rs"]
mod tests;
