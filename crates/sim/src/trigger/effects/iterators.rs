//! Retail list-iterator attachment effects.

use super::EffectOutcome;
use super::support::{EntityListKind, entity_list};
use crate::trigger::{Effect, TriggerIterator, TriggerScript, TriggerValue, VarType};

#[derive(Debug, Clone, Copy)]
pub(super) enum ScalarListKind {
    Player,
    Team,
}

pub(super) fn attach(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: EntityListKind,
) -> EffectOutcome {
    let Some(source_id) = effect.variable_id(1) else {
        return EffectOutcome::Skipped;
    };
    let source_is_valid = script
        .get_variable(source_id)
        .is_some_and(|variable| !variable.is_null && entity_list(kind, &variable.value).is_some());
    if !source_is_valid {
        return EffectOutcome::Skipped;
    }
    attach_to_iterator(effect, script, source_id)
}

pub(super) fn attach_scalar(
    effect: &Effect,
    script: &mut TriggerScript,
    kind: ScalarListKind,
) -> EffectOutcome {
    let Some(source_id) = effect.variable_id(1) else {
        return EffectOutcome::Skipped;
    };
    let source_is_valid = script
        .get_variable(source_id)
        .is_some_and(|variable| !variable.is_null && scalar_list_matches(&variable.value, kind));
    if !source_is_valid {
        return EffectOutcome::Skipped;
    }
    attach_to_iterator(effect, script, source_id)
}

pub(super) fn attach_vector(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(source_id) = effect.variable_id(1) else {
        return EffectOutcome::Skipped;
    };
    let source_is_valid = script.get_variable(source_id).is_some_and(|variable| {
        !variable.is_null
            && matches!(
                variable.value,
                TriggerValue::LocationList(_) | TriggerValue::VectorList(_)
            )
    });
    if !source_is_valid {
        return EffectOutcome::Skipped;
    }
    attach_to_iterator(effect, script, source_id)
}

fn scalar_list_matches(value: &TriggerValue, kind: ScalarListKind) -> bool {
    matches!(
        (kind, value),
        (ScalarListKind::Player, TriggerValue::PlayerList(_))
            | (ScalarListKind::Team, TriggerValue::TeamList(_))
    )
}

fn attach_to_iterator(
    effect: &Effect,
    script: &mut TriggerScript,
    source_id: u32,
) -> EffectOutcome {
    let Some(iterator_id) = effect.variable_id(2) else {
        return EffectOutcome::Skipped;
    };
    let Some(iterator_variable) = script.get_variable_mut(iterator_id) else {
        return EffectOutcome::Skipped;
    };
    if iterator_variable.var_type != VarType::Iterator {
        return EffectOutcome::Skipped;
    }
    if !matches!(iterator_variable.value, TriggerValue::Iterator(_)) {
        iterator_variable.value = TriggerValue::Iterator(TriggerIterator::default());
    }
    let TriggerValue::Iterator(iterator) = &mut iterator_variable.value else {
        return EffectOutcome::Skipped;
    };
    iterator.attach(source_id);
    iterator_variable.is_null = false;
    EffectOutcome::Applied
}
