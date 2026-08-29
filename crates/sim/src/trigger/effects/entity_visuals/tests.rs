use super::*;
use crate::trigger::{EffectType, TriggerVar, VarType};

#[test]
fn retail_entity_visual_dbids_are_typed() {
    assert_eq!(EffectType::from_u16(1000), Some(EffectType::FlashEntity));
    assert_eq!(EffectType::from_u16(1007), Some(EffectType::ResetDopple));
}

#[test]
fn flash_expands_squad_aliases_and_uses_versioned_intensity() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    let first = world.create_unit(1);
    let second = world.create_unit(1);
    assert!(world.attach_unit_to_squad(first, squad_id));
    assert!(world.attach_unit_to_squad(second, squad_id));

    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Unit, TriggerValue::Unit(squad_id));
    add_value(&mut script, 2, VarType::Time, TriggerValue::Time(500));
    add_value(&mut script, 3, VarType::Time, TriggerValue::Time(3_000));
    add_value(
        &mut script,
        4,
        VarType::Color,
        TriggerValue::Color(TriggerColor::new(255, 255, 0, 255)),
    );
    add_value(&mut script, 5, VarType::Float, TriggerValue::Float(80.0));
    let mut effect = Effect::new(1, EffectType::FlashEntity)
        .with_input_at(1, 1)
        .with_input_at(7, 2)
        .with_input_at(8, 3)
        .with_input_at(9, 4)
        .with_input_at(10, 5);
    effect.version = 2;

    assert_eq!(
        execute(&effect, &script, &mut world),
        Some(EffectOutcome::Applied)
    );
    for unit_id in [first, second] {
        let selection = world.entity_targeting_selection(unit_id).unwrap();
        assert_eq!(selection.color(), [255, 255, 0, 255]);
        assert_eq!(selection.started_at_ms(), 0);
        assert_eq!(selection.expires_at_ms(), Some(3_000));
        assert_eq!(selection.scroll_speed().to_bits(), (-4.0_f32).to_bits());
        assert_eq!(selection.intensity().to_bits(), 80.0_f32.to_bits());
    }
    assert!(world.entity_targeting_selection(squad_id).is_none());

    effect.version = 1;
    world.game_time_ms = 100;
    assert_eq!(
        execute(&effect, &script, &mut world),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(
        world
            .entity_targeting_selection(first)
            .unwrap()
            .intensity()
            .to_bits(),
        20.0_f32.to_bits()
    );
}

#[test]
fn reset_dopple_changes_policy_and_invalidates_existing_team_ghosts() {
    let mut world = World::new();
    let squad_id = world.create_squad(1);
    let unit_id = world.create_unit(1);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));

    let mut script = TriggerScript::new(1);
    add_value(
        &mut script,
        1,
        VarType::UnitList,
        TriggerValue::UnitList(vec![squad_id]),
    );
    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(true));
    add_value(&mut script, 3, VarType::Bool, TriggerValue::Bool(false));
    let effect = Effect::new(1, EffectType::ResetDopple)
        .with_input_at(2, 1)
        .with_input_at(7, 2)
        .with_input_at(8, 3);

    assert_eq!(
        execute(&effect, &script, &mut world),
        Some(EffectOutcome::Applied)
    );
    let policy = world.entity_dopple_policy(unit_id).unwrap();
    assert!(policy.gray_map_dopples());
    assert!(!policy.dopples());
    assert_eq!(policy.reset_revision(), 1);
    assert!(policy.visibility_update_pending());
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
