use super::*;
use crate::entities::SquadState;
use crate::trigger::{EffectType, TriggerValue, TriggerVar, VarType};
use glam::Vec3;

#[test]
fn set_mobile_resolves_unit_parents_and_removes_persistent_orders() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    let unit_id = world.create_unit(1);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .move_to(Vec3::new(12.0, 0.0, 0.0));

    let mut script = TriggerScript::new(1);
    script.add_variable(variable(1, VarType::Unit, TriggerValue::Unit(unit_id)));
    script.add_variable(variable(2, VarType::Bool, TriggerValue::Bool(false)));
    script.add_variable(variable(3, VarType::Bool, TriggerValue::Bool(false)));
    let mut effect = Effect::new(1, EffectType::SetMobile)
        .with_input_at(1, 1)
        .with_input_at(5, 2)
        .with_input_at(6, 3);
    effect.version = 2;

    assert_eq!(
        set_mobile(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    let squad = world.get_squad(squad_id).unwrap();
    assert!(!squad.base.is_mobile());
    assert_eq!(squad.state, SquadState::Idle);
    assert_eq!(squad.move_target, None);
}

#[test]
fn set_selectable_expands_squads_to_the_squad_and_every_live_child() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    let first_unit = world.create_unit(1);
    let second_unit = world.create_unit(1);
    let object_unit = world.create_unit(1);
    assert!(world.attach_unit_to_squad(first_unit, squad_id));
    assert!(world.attach_unit_to_squad(second_unit, squad_id));

    let mut script = TriggerScript::new(1);
    script.add_variable(variable(
        1,
        VarType::SquadList,
        TriggerValue::SquadList(vec![squad_id]),
    ));
    script.add_variable(variable(
        2,
        VarType::Object,
        TriggerValue::Object(object_unit),
    ));
    script.add_variable(variable(3, VarType::Bool, TriggerValue::Bool(false)));
    let effect = Effect::new(1, EffectType::SetSelectable)
        .with_input_at(4, 1)
        .with_input_at(5, 2)
        .with_input_at(7, 3);

    assert_eq!(
        set_selectable(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    for entity_id in [squad_id, first_unit, second_unit, object_unit] {
        assert_eq!(world.entity_is_selectable(entity_id), Some(false));
    }
}

#[test]
fn set_auto_attackable_expands_squads_to_children_but_not_the_squad_entity() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    let first_unit = world.create_unit(1);
    let second_unit = world.create_unit(1);
    let direct_unit = world.create_unit(1);
    assert!(world.attach_unit_to_squad(first_unit, squad_id));
    assert!(world.attach_unit_to_squad(second_unit, squad_id));

    let mut script = TriggerScript::new(1);
    script.add_variable(variable(1, VarType::Squad, TriggerValue::Squad(squad_id)));
    script.add_variable(variable(
        2,
        VarType::UnitList,
        TriggerValue::UnitList(vec![direct_unit]),
    ));
    script.add_variable(variable(3, VarType::Bool, TriggerValue::Bool(false)));
    let effect = Effect::new(1, EffectType::SetAutoAttackable)
        .with_input_at(2, 2)
        .with_input_at(3, 1)
        .with_input_at(7, 3);

    assert_eq!(
        set_auto_attackable(&effect, &script, &mut world),
        EffectOutcome::Applied
    );
    for unit_id in [first_unit, second_unit, direct_unit] {
        assert_eq!(world.unit_is_auto_attackable(unit_id), Some(false));
    }
    assert_eq!(world.entity_is_selectable(squad_id), Some(true));
}

#[test]
fn retail_control_flag_dbids_decode_to_typed_effects() {
    assert_eq!(EffectType::from_u16(510), Some(EffectType::SetMobile));
    assert_eq!(EffectType::from_u16(900), Some(EffectType::SetSelectable));
    assert_eq!(
        EffectType::from_u16(915),
        Some(EffectType::SetAutoAttackable)
    );
}

fn variable(id: u32, var_type: VarType, value: TriggerValue) -> TriggerVar {
    TriggerVar::new(id, var_type).with_value(value)
}
