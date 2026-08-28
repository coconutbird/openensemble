//! Retail parent/child entity relationship queries.

use super::support::{EntityListKind, entities_at, unique_add, used_variable_id};
use super::{EffectOutcome, write_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};

pub(super) fn get_child_units(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some(output_id) = used_variable_id(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let mut units = Vec::new();
    if let Some(squad_id) = entities_at(effect, script, 1, EntityListKind::Squad)
        .and_then(|squads| squads.into_iter().next())
    {
        add_children(world, squad_id, &mut units);
    }
    for squad_id in entities_at(effect, script, 2, EntityListKind::Squad).unwrap_or_default() {
        add_children(world, squad_id, &mut units);
    }
    write_value(script, output_id, TriggerValue::UnitList(units))
}

pub(super) fn get_parent_squad(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some(unit_id) = entities_at(effect, script, 1, EntityListKind::Unit)
        .and_then(|units| units.into_iter().next())
    else {
        return EffectOutcome::Skipped;
    };
    let Some(unit) = world.get_unit(unit_id) else {
        return EffectOutcome::Skipped;
    };
    let Some(output_id) = used_variable_id(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    write_value(
        script,
        output_id,
        TriggerValue::Squad(unit.squad_id.unwrap_or(EntityId::INVALID)),
    )
}

fn add_children(world: &World, squad_id: EntityId, units: &mut Vec<EntityId>) {
    let Some(squad) = world.get_squad(squad_id) else {
        return;
    };
    for &unit_id in &squad.unit_ids {
        if world.get_unit(unit_id).is_some() {
            unique_add(units, unit_id);
        }
    }
}

#[cfg(test)]
mod tests;
