//! Retail campaign end effects: the mission result and achievements.

use super::support::variable_is_used;
use super::{EffectOutcome, value_at};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::world::World;

pub(super) fn execute(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::MissionResult => mission_result(effect, script, world),
        EffectType::GrantAchievement => grant_achievement(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

/// `MissionResult` (1002, `BTriggerEffect_teMissionResult` `0x14047D510`). Slot 1 is the
/// player state that the end screen shows.
fn mission_result(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(result) = value_at(effect, script, 1).and_then(TriggerValue::as_int) else {
        return EffectOutcome::Skipped;
    };
    world.set_mission_result(result);
    EffectOutcome::Applied
}

/// `GrantAchievement` (1030, `BTriggerEffect_teGrantAchievement` `0x14047FF40`). A used slot 1
/// is the achievement name. Otherwise the name is empty.
fn grant_achievement(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let name = if variable_is_used(effect, script, 1) {
        let Some(TriggerValue::String(name)) = value_at(effect, script, 1) else {
            return EffectOutcome::Skipped;
        };
        name.clone()
    } else {
        String::new()
    };
    world.grant_achievement(name);
    EffectOutcome::Applied
}

#[cfg(test)]
#[path = "campaign/tests.rs"]
mod tests;
