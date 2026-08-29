//! Retail `CompareAmmoPercent` condition (DBID 256).

use super::{
    Condition, TriggerScript, TriggerValue, World, as_f32, compare_partial, entity_at, operator_at,
    value_at, write_trigger_value,
};
use crate::entities::UnitAmmunition;

pub(super) fn compare_percent(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
) -> bool {
    if condition.version != 2 {
        return false;
    }
    let current = entity_at(condition, script, 1)
        .and_then(|unit_id| world.unit_ammunition(unit_id))
        .map_or(0.0, UnitAmmunition::percentage);
    if let Some(variable_id) = condition.variable_id(4) {
        write_trigger_value(script, variable_id, TriggerValue::Float(current));
    }
    let (Some(operator), Some(expected)) = (
        operator_at(condition, script, 2),
        value_at(condition, script, 3).and_then(as_f32),
    ) else {
        return false;
    };
    compare_partial(&current, operator, &expected)
}

#[cfg(test)]
mod tests {
    use super::super::evaluate_condition;
    use super::*;
    use crate::entity_id::EntityId;
    use crate::trigger::{ConditionResult, ConditionType, TriggerVar, VarType};

    #[test]
    fn version_two_compares_and_writes_live_or_stale_unit_percentages() {
        let mut world = World::new();
        let unit_id = world.create_unit(1);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.ammunition.configure(200.0, 9.0, false);
        unit.ammunition.set_current(50.0);
        let mut script = script(unit_id, 0.25);
        let mut condition = Condition::new(1, ConditionType::CompareAmmoPercent)
            .with_input_at(1, 1)
            .with_input_at(2, 2)
            .with_input_at(3, 3)
            .with_output_at(4, 4);
        condition.version = 2;

        assert_eq!(
            evaluate_condition(&condition, 0, &mut script, &mut world),
            ConditionResult::True
        );
        assert_eq!(
            script.get_variable(4).unwrap().value,
            TriggerValue::Float(0.25)
        );

        script.get_variable_mut(1).unwrap().value = TriggerValue::Unit(EntityId::INVALID);
        script.get_variable_mut(3).unwrap().value = TriggerValue::Float(0.0);
        assert_eq!(
            evaluate_condition(&condition, 0, &mut script, &mut world),
            ConditionResult::True
        );
        assert_eq!(
            script.get_variable(4).unwrap().value,
            TriggerValue::Float(0.0)
        );
    }

    fn script(unit_id: EntityId, expected: f32) -> TriggerScript {
        let mut script = TriggerScript::new(1);
        script.add_variable(
            TriggerVar::new(1, VarType::Unit).with_value(TriggerValue::Unit(unit_id)),
        );
        script.add_variable(TriggerVar::new(2, VarType::Operator).with_value(TriggerValue::Int(3)));
        script.add_variable(
            TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(expected)),
        );
        script.add_variable(
            TriggerVar::new(4, VarType::Float)
                .with_value(TriggerValue::Float(-1.0))
                .as_output(),
        );
        script
    }
}
