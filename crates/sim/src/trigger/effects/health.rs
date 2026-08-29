//! Retail health query, direct repair/damage, and combat-damage effects.

use super::support::{
    EntityListKind, bool_at, entities_at, float_at, unique_add, used_variable_id, variable_is_used,
};
use super::{EffectOutcome, write_value};
use crate::gameplay::GameplayCatalog;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::UnitHealth;
use crate::{EntityId, World};

pub(super) fn get_health(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    if !matches!(effect.version, 2 | 3) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if effect.version == 3
        && !(1..=2)
            .chain(7..=8)
            .any(|slot| variable_is_used(effect, script, slot))
    {
        return EffectOutcome::Skipped;
    }

    let mut unit_ids = Vec::new();
    let mut maximum_hitpoints = 0.0;
    let mut maximum_shieldpoints = 0.0;
    if effect.version == 3 {
        if let Some(unit_list) = entities_at(effect, script, 8, EntityListKind::Unit) {
            for unit_id in &unit_list {
                add_unit_maximums(
                    world,
                    *unit_id,
                    &mut maximum_hitpoints,
                    &mut maximum_shieldpoints,
                );
            }
            unit_ids = unit_list;
        }
        if let Some(unit_id) = scalar_entity(effect, script, 7, EntityListKind::Unit) {
            unique_add(&mut unit_ids, unit_id);
            add_unit_maximums(
                world,
                unit_id,
                &mut maximum_hitpoints,
                &mut maximum_shieldpoints,
            );
        }
    }
    if let Some(squad_id) = scalar_entity(effect, script, 1, EntityListKind::Squad) {
        add_squad_health(
            world,
            squad_id,
            &mut unit_ids,
            &mut maximum_hitpoints,
            &mut maximum_shieldpoints,
        );
    }
    for squad_id in entities_at(effect, script, 2, EntityListKind::Squad).unwrap_or_default() {
        add_squad_health(
            world,
            squad_id,
            &mut unit_ids,
            &mut maximum_hitpoints,
            &mut maximum_shieldpoints,
        );
    }

    let (hitpoints, shieldpoints) = unit_ids
        .iter()
        .filter_map(|unit_id| world.unit_health(*unit_id))
        .fold((0.0, 0.0), |(hp, sp), health| {
            (hp + health.hitpoints, sp + health.shieldpoints)
        });
    write_float(effect, script, 3, hitpoints);
    write_float(effect, script, 4, safe_ratio(hitpoints, maximum_hitpoints));
    write_float(effect, script, 5, shieldpoints);
    write_float(
        effect,
        script,
        6,
        safe_ratio(shieldpoints, maximum_shieldpoints),
    );
    EffectOutcome::Applied
}

pub(super) fn repair_or_damage(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    repair: bool,
) -> EffectOutcome {
    let Some(unit_ids) = target_units(effect, script, world) else {
        return EffectOutcome::Skipped;
    };
    let hitpoints = health_value(effect, script, 5, 6);
    let shields = health_value(effect, script, 7, 8);
    if hitpoints.is_none() && shields.is_none() {
        return EffectOutcome::Skipped;
    }
    if unit_ids.is_empty() {
        return EffectOutcome::Applied;
    }
    let spread = bool_at(effect, script, 9).unwrap_or(false);
    let divisor = count_as_f32(unit_ids.len());
    for unit_id in unit_ids {
        let Some(health) = world.unit_health(unit_id) else {
            continue;
        };
        let hp_delta = resolve_delta(hitpoints, health.maximum_hitpoints, spread, divisor);
        let sp_delta = resolve_delta(shields, health.maximum_shieldpoints, spread, divisor);
        if repair {
            let _repaired = world.repair_unit(unit_id, hp_delta, sp_delta);
        } else {
            let _damaged = world.damage_unit_direct(unit_id, hp_delta, sp_delta);
        }
    }
    EffectOutcome::Applied
}

pub(super) fn combat_damage(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let override_revive = effect.version == 2 && bool_at(effect, script, 7).unwrap_or(false);
    let Some(unit_ids) = target_units(effect, script, world) else {
        return EffectOutcome::Skipped;
    };
    let Some(mut damage) = float_at(effect, script, 5).filter(|value| value.is_finite()) else {
        return EffectOutcome::Skipped;
    };
    if unit_ids.is_empty() {
        return EffectOutcome::Applied;
    }
    if bool_at(effect, script, 6).unwrap_or(false) {
        damage /= count_as_f32(unit_ids.len());
    }
    for unit_id in unit_ids {
        if let Some(gameplay) = gameplay {
            let _damaged = world.damage_unit_with_gameplay_override(
                unit_id,
                damage,
                gameplay,
                override_revive,
            );
        } else {
            let _damaged = world.damage_unit_with_override(unit_id, damage, override_revive);
        }
    }
    EffectOutcome::Applied
}

#[derive(Debug, Clone, Copy)]
enum HealthValue {
    Absolute(f32),
    Percent(f32),
}

fn health_value(
    effect: &Effect,
    script: &TriggerScript,
    absolute_slot: u16,
    percent_slot: u16,
) -> Option<HealthValue> {
    float_at(effect, script, absolute_slot)
        .filter(|value| value.is_finite())
        .map(HealthValue::Absolute)
        .or_else(|| {
            float_at(effect, script, percent_slot)
                .filter(|value| value.is_finite())
                .map(HealthValue::Percent)
        })
}

fn resolve_delta(value: Option<HealthValue>, maximum: f32, spread: bool, divisor: f32) -> f32 {
    match value {
        Some(HealthValue::Absolute(value)) if spread => value / divisor,
        Some(HealthValue::Absolute(value)) => value,
        Some(HealthValue::Percent(value)) => value * maximum,
        None => 0.0,
    }
}

fn target_units(effect: &Effect, script: &TriggerScript, world: &World) -> Option<Vec<EntityId>> {
    if !(1..=4).any(|slot| variable_is_used(effect, script, slot)) {
        return None;
    }
    let mut unit_ids = entities_at(effect, script, 4, EntityListKind::Unit).unwrap_or_default();
    if let Some(unit_id) = scalar_entity(effect, script, 3, EntityListKind::Unit) {
        unique_add(&mut unit_ids, unit_id);
    }
    if let Some(squad_id) = scalar_entity(effect, script, 1, EntityListKind::Squad) {
        add_squad_members(world, squad_id, &mut unit_ids);
    }
    for squad_id in entities_at(effect, script, 2, EntityListKind::Squad).unwrap_or_default() {
        add_squad_members(world, squad_id, &mut unit_ids);
    }
    Some(unit_ids)
}

fn add_squad_members(world: &World, squad_id: EntityId, unit_ids: &mut Vec<EntityId>) {
    let Some(squad) = world.get_squad(squad_id) else {
        return;
    };
    for unit_id in &squad.unit_ids {
        unique_add(unit_ids, *unit_id);
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

fn add_squad_health(
    world: &World,
    squad_id: EntityId,
    unit_ids: &mut Vec<EntityId>,
    maximum_hitpoints: &mut f32,
    maximum_shieldpoints: &mut f32,
) {
    let Some(squad) = world.get_squad(squad_id) else {
        return;
    };
    for unit_id in &squad.unit_ids {
        unique_add(unit_ids, *unit_id);
        add_unit_maximums(world, *unit_id, maximum_hitpoints, maximum_shieldpoints);
    }
}

fn add_unit_maximums(
    world: &World,
    unit_id: EntityId,
    maximum_hitpoints: &mut f32,
    maximum_shieldpoints: &mut f32,
) {
    if let Some(UnitHealth {
        maximum_hitpoints: hp,
        maximum_shieldpoints: sp,
        ..
    }) = world.unit_health(unit_id)
    {
        *maximum_hitpoints += hp;
        *maximum_shieldpoints += sp;
    }
}

fn write_float(effect: &Effect, script: &mut TriggerScript, signature_id: u16, value: f32) {
    if let Some(variable_id) = used_variable_id(effect, script, signature_id) {
        let _outcome = write_value(script, variable_id, TriggerValue::Float(value));
    }
}

fn safe_ratio(current: f32, maximum: f32) -> f32 {
    if maximum == 0.0 {
        0.0
    } else {
        current / maximum
    }
}

fn count_as_f32(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}
