//! Retail rally-point effects 717 through 719.

use super::support::{player_at, variable_is_used, vector_at};
use super::{EffectOutcome, value_at, write_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue, TriggerVec3};
use crate::{EntityId, World};

pub(super) fn set(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let (Some(player_id), Some(position)) = (
        player_at(effect, script, 1),
        vector_at(effect, script, 2).filter(|position| position.is_finite()),
    ) else {
        return EffectOutcome::Skipped;
    };
    if world.get_player(player_id).is_none() {
        return EffectOutcome::Skipped;
    }
    let target_entity_id = (effect.version == 2)
        .then(|| optional_entity(effect, script, 4))
        .flatten();
    let applied = if effect.version == 2 && variable_is_used(effect, script, 3) {
        let Some(unit_id) = optional_entity(effect, script, 3) else {
            return EffectOutcome::Skipped;
        };
        let Some(owner_id) = world.get_unit(unit_id).map(|unit| unit.base.player_id) else {
            return EffectOutcome::Skipped;
        };
        world.set_unit_rally_point(unit_id, owner_id, position, target_entity_id)
    } else {
        world.set_player_rally_point(player_id, position, target_entity_id)
    };
    if applied {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

pub(super) fn clear(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(player_id) = player_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    if world.get_player(player_id).is_none() {
        return EffectOutcome::Skipped;
    }
    let applied = if effect.version == 2 && variable_is_used(effect, script, 2) {
        let Some(unit_id) = optional_entity(effect, script, 2) else {
            return EffectOutcome::Skipped;
        };
        let Some(owner_id) = world.get_unit(unit_id).map(|unit| unit.base.player_id) else {
            return EffectOutcome::Skipped;
        };
        world.clear_unit_rally_point(unit_id, owner_id)
    } else {
        world.clear_player_rally_point(player_id)
    };
    if applied {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

pub(super) fn get(effect: &Effect, script: &mut TriggerScript, world: &World) -> EffectOutcome {
    let (Some(have_output), Some(position_output)) = (effect.variable_id(2), effect.variable_id(3))
    else {
        return EffectOutcome::Skipped;
    };
    if script.get_variable(have_output).is_none() || script.get_variable(position_output).is_none()
    {
        return EffectOutcome::Skipped;
    }
    let rally_point =
        player_at(effect, script, 1).and_then(|player_id| world.player_rally_point(player_id));
    let position = rally_point.map_or(glam::Vec3::ZERO, |rally_point| {
        world.resolve_rally_point(rally_point)
    });
    let _have = write_value(
        script,
        have_output,
        TriggerValue::Bool(rally_point.is_some()),
    );
    write_value(
        script,
        position_output,
        TriggerValue::Vector(TriggerVec3::new(position.x, position.y, position.z)),
    )
}

fn optional_entity(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    value_at(effect, script, signature_id).and_then(TriggerValue::as_entity)
}

#[cfg(test)]
mod tests;
