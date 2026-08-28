//! Retail live mobility, selection, and automatic-targeting overrides.

use super::support::{
    EntityListKind, bool_at, entities_at, scalar_and_list, unique_add, variable_is_used,
};
use super::{EffectOutcome, TriggerScript};
use crate::World;
use crate::trigger::Effect;

pub(super) fn set_mobile(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if !(1..=4).any(|signature_id| variable_is_used(effect, script, signature_id)) {
        return EffectOutcome::Skipped;
    }
    let Some(mobile) = bool_at(effect, script, 5) else {
        return EffectOutcome::Skipped;
    };
    let temporary = effect.version == 2 && bool_at(effect, script, 6).unwrap_or(false);
    let unit_ids = scalar_and_list(effect, script, 1, 2, EntityListKind::Unit);
    let mut squad_ids = scalar_and_list(effect, script, 3, 4, EntityListKind::Squad);
    for unit_id in unit_ids {
        if let Some(squad_id) = world.get_unit(unit_id).and_then(|unit| unit.squad_id) {
            unique_add(&mut squad_ids, squad_id);
        }
    }
    for squad_id in squad_ids {
        let _changed = world.set_squad_mobile(squad_id, mobile, temporary);
    }
    EffectOutcome::Applied
}

pub(super) fn set_selectable(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(selectable) = bool_at(effect, script, 7) else {
        return EffectOutcome::Skipped;
    };
    let mut entity_ids = scalar_and_list(effect, script, 1, 2, EntityListKind::Unit);
    let squad_ids = scalar_and_list(effect, script, 3, 4, EntityListKind::Squad);
    for squad_id in squad_ids {
        let Some(unit_ids) = world
            .get_squad(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            continue;
        };
        unique_add(&mut entity_ids, squad_id);
        for unit_id in unit_ids {
            unique_add(&mut entity_ids, unit_id);
        }
    }
    entity_ids.extend(entities_at(effect, script, 6, EntityListKind::Object).unwrap_or_default());
    for entity_id in entities_at(effect, script, 5, EntityListKind::Object).unwrap_or_default() {
        unique_add(&mut entity_ids, entity_id);
    }
    for entity_id in entity_ids {
        let _changed = world.set_entity_selectable(entity_id, selectable);
    }
    EffectOutcome::Applied
}

pub(super) fn set_auto_attackable(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(auto_attackable) = bool_at(effect, script, 7) else {
        return EffectOutcome::Skipped;
    };
    let mut unit_ids = scalar_and_list(effect, script, 1, 2, EntityListKind::Unit);
    let squad_ids = scalar_and_list(effect, script, 3, 4, EntityListKind::Squad);
    for squad_id in squad_ids {
        if let Some(squad) = world.get_squad(squad_id) {
            unit_ids.extend_from_slice(&squad.unit_ids);
        }
    }
    for unit_id in unit_ids {
        let _changed = world.set_unit_auto_attackable(unit_id, auto_attackable);
    }
    EffectOutcome::Applied
}

#[cfg(test)]
mod tests;
