//! Retail scripted squad-order effects backed by authoritative sim state.

use super::support::{
    EntityListKind, bool_at, entity_list, unique_add, variable_is_used, vector_at,
};
use super::{EffectOutcome, value_at};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};

pub(super) fn unload(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let use_squad = variable_is_used(effect, script, 3);
    let use_squad_list = variable_is_used(effect, script, 5);
    if !use_squad && !use_squad_list {
        return EffectOutcome::Skipped;
    }
    let mut containers = if use_squad_list {
        list_at(effect, script, 5).cloned().unwrap_or_default()
    } else {
        Vec::new()
    };
    if use_squad && let Some(squad_id) = squad_at(effect, script, 3) {
        unique_add(&mut containers, squad_id);
    }
    let passenger_filter = if effect.version == 4 {
        let mut passengers = if variable_is_used(effect, script, 7) {
            list_at(effect, script, 7).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        if variable_is_used(effect, script, 6)
            && let Some(squad_id) = squad_at(effect, script, 6)
        {
            passengers.push(squad_id);
        }
        passengers
    } else {
        Vec::new()
    };
    for container_id in containers {
        let _accepted = world.trigger_unload_squad(container_id, &passenger_filter);
    }
    EffectOutcome::Applied
}

pub(super) fn move_squads(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if effect.version != 6 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    execute_location_order(
        effect,
        script,
        world,
        OrderSlots {
            squad: 1,
            squad_list: 5,
            target_unit: 6,
            target_location: 2,
            target_squad: 7,
            attack_move: 8,
            queue_order: 9,
            do_ability: None,
        },
    )
}

pub(super) fn work(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    execute_location_order(
        effect,
        script,
        world,
        OrderSlots {
            squad: 1,
            squad_list: 2,
            target_unit: 3,
            target_location: 4,
            target_squad: 5,
            attack_move: 6,
            queue_order: 7,
            do_ability: (effect.version == 4).then_some(8),
        },
    )
}

#[derive(Debug, Clone, Copy)]
struct OrderSlots {
    squad: u16,
    squad_list: u16,
    target_unit: u16,
    target_location: u16,
    target_squad: u16,
    attack_move: u16,
    queue_order: u16,
    do_ability: Option<u16>,
}

fn execute_location_order(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    slots: OrderSlots,
) -> EffectOutcome {
    if bool_at(effect, script, slots.attack_move).unwrap_or(false)
        || bool_at(effect, script, slots.queue_order).unwrap_or(false)
        || slots
            .do_ability
            .is_some_and(|slot| bool_at(effect, script, slot).unwrap_or(false))
    {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let use_squad = variable_is_used(effect, script, slots.squad);
    let use_squad_list = variable_is_used(effect, script, slots.squad_list);
    if !use_squad && !use_squad_list {
        return EffectOutcome::Skipped;
    }

    let unit_target = variable_is_used(effect, script, slots.target_unit)
        .then(|| unit_at(effect, script, slots.target_unit))
        .flatten()
        .filter(|unit_id| world.get_unit(*unit_id).is_some());
    let squad_target = variable_is_used(effect, script, slots.target_squad)
        .then(|| squad_at(effect, script, slots.target_squad))
        .flatten()
        .filter(|squad_id| world.get_squad(*squad_id).is_some());
    if unit_target.is_some() || squad_target.is_some() {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(target) = variable_is_used(effect, script, slots.target_location)
        .then(|| vector_at(effect, script, slots.target_location))
        .flatten()
    else {
        return EffectOutcome::Skipped;
    };
    if !target.is_finite() {
        return EffectOutcome::Skipped;
    }

    let mut working_squads = if use_squad_list {
        list_at(effect, script, slots.squad_list)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if use_squad
        && let Some(squad_id) = squad_at(effect, script, slots.squad)
        && world.get_squad(squad_id).is_some()
    {
        unique_add(&mut working_squads, squad_id);
    }
    let mut issued = false;
    let mut unique_squads = Vec::new();
    for squad_id in working_squads {
        unique_add(&mut unique_squads, squad_id);
    }
    for squad_id in unique_squads {
        let Some(player_id) = world.get_squad(squad_id).map(|squad| squad.base.player_id) else {
            continue;
        };
        issued |= world.issue_move_order(player_id, squad_id, target);
    }
    if issued {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

fn unit_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Unit(value) => Some(*value),
        _ => None,
    }
}

fn squad_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Squad(value) => Some(*value),
        _ => None,
    }
}

fn list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a Vec<EntityId>> {
    entity_list(
        EntityListKind::Squad,
        value_at(effect, script, signature_id)?,
    )
}
