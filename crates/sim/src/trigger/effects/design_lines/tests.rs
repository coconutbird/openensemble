use super::*;
use crate::trigger::{TriggerVar, VarType};
use glam::Vec3;

#[test]
fn get_points_reads_authoritative_world_geometry_and_clears_missing_lines() {
    let mut world = World::new();
    world.configure_design_lines([(
        17,
        vec![Vec3::new(30.0, 2.0, 10.0), Vec3::new(60.0, 5.0, 40.0)],
    )]);
    let mut script = TriggerScript::new(1);
    add_var(
        &mut script,
        1,
        VarType::DesignLine,
        TriggerValue::DesignLine(17),
    );
    add_var(
        &mut script,
        2,
        VarType::VectorList,
        TriggerValue::VectorList(vec![TriggerVec3::zero()]),
    );
    let mut effect = Effect::new(1, EffectType::DesignLineGetPoints)
        .with_input_at(1, 1)
        .with_output_at(2, 2);

    for version in [0, 1, u8::MAX] {
        effect.version = version;
        assert_eq!(
            execute(&effect, &mut script, &world),
            Some(EffectOutcome::Applied)
        );
        assert_eq!(
            script.get_variable(2).map(|variable| &variable.value),
            Some(&TriggerValue::VectorList(vec![
                TriggerVec3::new(30.0, 2.0, 10.0),
                TriggerVec3::new(60.0, 5.0, 40.0),
            ]))
        );
    }

    script.get_variable_mut(1).unwrap().value = TriggerValue::DesignLine(999);
    assert_eq!(
        execute(&effect, &mut script, &world),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(
        script.get_variable(2).map(|variable| &variable.value),
        Some(&TriggerValue::VectorList(Vec::new()))
    );
}

#[test]
fn list_effects_preserve_duplicates_and_retail_removal_modes() {
    let mut script = TriggerScript::new(1);
    add_var(
        &mut script,
        1,
        VarType::DesignLine,
        TriggerValue::DesignLine(7),
    );
    add_var(
        &mut script,
        2,
        VarType::DesignLineList,
        TriggerValue::DesignLineList(vec![8, 7]),
    );
    add_var(&mut script, 3, VarType::Bool, TriggerValue::Bool(true));
    add_var(
        &mut script,
        4,
        VarType::DesignLineList,
        TriggerValue::DesignLineList(vec![1]),
    );
    add_var(&mut script, 5, VarType::Integer, TriggerValue::Int(0));
    add_var(&mut script, 6, VarType::Bool, TriggerValue::Bool(false));
    add_var(&mut script, 7, VarType::Bool, TriggerValue::Bool(false));

    let add = Effect::new(1, EffectType::DesignLineListAdd)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_output_at(4, 4);
    assert_eq!(
        execute(&add, &mut script, &World::new()),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(lines(&script, 4), &[7, 8, 7]);

    script.get_variable_mut(3).unwrap().value = TriggerValue::Bool(false);
    assert_eq!(
        execute(&add, &mut script, &World::new()),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(lines(&script, 4), &[7, 8, 7, 7, 8, 7]);

    let remove = Effect::new(2, EffectType::DesignLineListRemove)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 6)
        .with_input_at(4, 7)
        .with_output_at(5, 4);
    assert_eq!(
        execute(&remove, &mut script, &World::new()),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(lines(&script, 4), &[7, 8, 7]);

    script.get_variable_mut(4).unwrap().value = TriggerValue::DesignLineList(vec![7, 7, 8]);
    script.get_variable_mut(7).unwrap().value = TriggerValue::Bool(true);
    assert_eq!(
        execute(&remove, &mut script, &World::new()),
        Some(EffectOutcome::Applied)
    );
    assert!(lines(&script, 4).is_empty());

    let size = Effect::new(3, EffectType::DesignLineListGetSize)
        .with_input_at(1, 2)
        .with_output_at(2, 5);
    assert_eq!(
        execute(&size, &mut script, &World::new()),
        Some(EffectOutcome::Applied)
    );
    assert_eq!(script.get_variable(5).unwrap().value, TriggerValue::Int(2));
}

#[test]
fn copy_effects_and_type_failures_are_routed_atomically() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add_var(
        &mut script,
        1,
        VarType::DesignLine,
        TriggerValue::DesignLine(44),
    );
    add_var(
        &mut script,
        2,
        VarType::DesignLine,
        TriggerValue::DesignLine(-1),
    );
    add_var(
        &mut script,
        3,
        VarType::DesignLineList,
        TriggerValue::DesignLineList(vec![1, 2, 2]),
    );
    add_var(
        &mut script,
        4,
        VarType::DesignLineList,
        TriggerValue::DesignLineList(Vec::new()),
    );
    for (effect_type, source, destination) in [
        (EffectType::CopyDesignLine, 1, 2),
        (EffectType::CopyDesignLineList, 3, 4),
    ] {
        let effect = Effect::new(1, effect_type)
            .with_input_at(1, source)
            .with_output_at(2, destination);
        assert_eq!(
            super::super::execute_effect(&effect, &mut script, &mut world, None, None).0,
            EffectOutcome::Applied
        );
    }
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::DesignLine(44)
    );
    assert_eq!(lines(&script, 4), &[1, 2, 2]);

    let get_points = Effect::new(2, EffectType::DesignLineGetPoints)
        .with_input_at(1, 3)
        .with_output_at(2, 4);
    let before = script.get_variable(4).unwrap().value.clone();
    assert_eq!(
        execute(&get_points, &mut script, &world),
        Some(EffectOutcome::Skipped)
    );
    assert_eq!(script.get_variable(4).unwrap().value, before);
}

fn add_var(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

fn lines(script: &TriggerScript, id: u32) -> &[i32] {
    match &script.get_variable(id).unwrap().value {
        TriggerValue::DesignLineList(values) => values,
        _ => panic!("expected design-line list"),
    }
}
