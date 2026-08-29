//! Retail unit and squad ammunition query/mutation effects.

use super::support::{EntityListKind, entities_at, float_at, used_variable_id, variable_is_used};
use super::{EffectOutcome, write_value};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::{EntityId, World};

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    match effect.effect_type {
        EffectType::GetAmmo => Some(get_ammunition(effect, script, world)),
        EffectType::SetAmmo => Some(set_ammunition(effect, script, world)),
        _ => None,
    }
}

fn get_ammunition(effect: &Effect, script: &mut TriggerScript, world: &World) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let values = if variable_is_used(effect, script, 1) {
        unit_at(effect, script, 1).and_then(|unit_id| {
            world
                .unit_ammunition(unit_id)
                .map(|ammunition| (ammunition.current(), ammunition.percentage()))
        })
    } else if effect.version == 2 && variable_is_used(effect, script, 4) {
        squad_at(effect, script, 4).and_then(|squad_id| {
            world.squad_ammunition(squad_id).map(|(current, maximum)| {
                let percentage = if maximum >= f32::EPSILON {
                    current / maximum
                } else {
                    0.0
                };
                (current, percentage)
            })
        })
    } else {
        None
    };
    if let Some((amount, percentage)) = values {
        write_float(effect, script, 2, amount);
        write_float(effect, script, 3, percentage);
    }
    EffectOutcome::Applied
}

fn set_ammunition(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if !matches!(effect.version, 1 | 2) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if variable_is_used(effect, script, 1) {
        if let Some(unit_id) = unit_at(effect, script, 1) {
            set_unit(effect, script, world, unit_id);
        }
    } else if effect.version == 2
        && variable_is_used(effect, script, 4)
        && let Some(squad_id) = squad_at(effect, script, 4)
    {
        set_squad(effect, script, world, squad_id);
    }
    EffectOutcome::Applied
}

fn set_unit(effect: &Effect, script: &TriggerScript, world: &mut World, unit_id: EntityId) {
    if variable_is_used(effect, script, 2) {
        if let Some(amount) = float_at(effect, script, 2) {
            let _changed = world.set_unit_ammunition(unit_id, amount);
        }
    } else if variable_is_used(effect, script, 3)
        && let (Some(percentage), Some(ammunition)) =
            (float_at(effect, script, 3), world.unit_ammunition(unit_id))
    {
        let _changed = world.set_unit_ammunition(unit_id, ammunition.maximum() * percentage);
    }
}

fn set_squad(effect: &Effect, script: &TriggerScript, world: &mut World, squad_id: EntityId) {
    if variable_is_used(effect, script, 2) {
        if let Some(amount) = float_at(effect, script, 2) {
            let _changed = world.set_squad_ammunition_amount(squad_id, amount);
        }
    } else if variable_is_used(effect, script, 3)
        && let Some(percentage) = float_at(effect, script, 3)
    {
        let _changed = world.set_squad_ammunition_percentage(squad_id, percentage);
    }
}

fn unit_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<EntityId> {
    entities_at(effect, script, slot, EntityListKind::Unit)?
        .into_iter()
        .next()
}

fn squad_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<EntityId> {
    entities_at(effect, script, slot, EntityListKind::Squad)?
        .into_iter()
        .next()
}

fn write_float(effect: &Effect, script: &mut TriggerScript, slot: u16, value: f32) {
    if let Some(variable_id) = used_variable_id(effect, script, slot) {
        let _outcome = write_value(script, variable_id, TriggerValue::Float(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{EffectType, TriggerVar, VarType};

    #[test]
    fn version_two_get_and_set_preserve_unit_precedence_and_squad_ratios() {
        let (mut world, squad_id, first, second) = ammunition_world();
        let mut script = script(first, squad_id);
        let mut get_unit = Effect::new(1, EffectType::GetAmmo)
            .with_input_at(1, 1)
            .with_input_at(4, 2)
            .with_output_at(2, 5)
            .with_output_at(3, 6);
        get_unit.version = 2;
        assert_eq!(
            get_ammunition(&get_unit, &mut script, &world),
            EffectOutcome::Applied
        );
        assert_float(&script, 5, 25.0);
        assert_float(&script, 6, 0.25);

        let mut set_squad = Effect::new(2, EffectType::SetAmmo)
            .with_input_at(3, 4)
            .with_input_at(4, 2);
        set_squad.version = 2;
        assert_eq!(
            set_ammunition(&set_squad, &script, &mut world),
            EffectOutcome::Applied
        );
        assert_close(world.unit_ammunition(first).unwrap().current(), 50.0);
        assert_close(world.unit_ammunition(second).unwrap().current(), 150.0);
        assert_eq!(world.squad_ammunition(squad_id), Some((200.0, 400.0)));

        script.get_variable_mut(3).unwrap().value = TriggerValue::Float(100.0);
        let mut set_amount = Effect::new(3, EffectType::SetAmmo)
            .with_input_at(2, 3)
            .with_input_at(4, 2);
        set_amount.version = 2;
        assert_eq!(
            set_ammunition(&set_amount, &script, &mut world),
            EffectOutcome::Applied
        );
        assert_close(world.unit_ammunition(first).unwrap().current(), 25.0);
        assert_close(world.unit_ammunition(second).unwrap().current(), 75.0);
    }

    fn ammunition_world() -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        let squad_id = world.create_squad(1);
        world
            .get_squad_mut(squad_id)
            .unwrap()
            .set_ammunition_maximum(400.0);
        let first = world.create_unit(1);
        let second = world.create_unit(1);
        assert!(world.attach_unit_to_squad(first, squad_id));
        assert!(world.attach_unit_to_squad(second, squad_id));
        let ammunition = &mut world.get_unit_mut(first).unwrap().ammunition;
        ammunition.configure(100.0, 1.0, false);
        ammunition.set_current(25.0);
        let ammunition = &mut world.get_unit_mut(second).unwrap().ammunition;
        ammunition.configure(300.0, 1.0, false);
        ammunition.set_current(150.0);
        (world, squad_id, first, second)
    }

    fn script(unit_id: EntityId, squad_id: EntityId) -> TriggerScript {
        let mut script = TriggerScript::new(1);
        script.add_variable(
            TriggerVar::new(1, VarType::Unit).with_value(TriggerValue::Unit(unit_id)),
        );
        script.add_variable(
            TriggerVar::new(2, VarType::Squad).with_value(TriggerValue::Squad(squad_id)),
        );
        script
            .add_variable(TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(0.0)));
        script
            .add_variable(TriggerVar::new(4, VarType::Float).with_value(TriggerValue::Float(0.5)));
        for id in 5..=6 {
            script.add_variable(
                TriggerVar::new(id, VarType::Float)
                    .with_value(TriggerValue::Float(-1.0))
                    .as_output(),
            );
        }
        script
    }

    fn assert_float(script: &TriggerScript, variable_id: u32, expected: f32) {
        let TriggerValue::Float(actual) = script.get_variable(variable_id).unwrap().value else {
            panic!("expected float output");
        };
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }
}
