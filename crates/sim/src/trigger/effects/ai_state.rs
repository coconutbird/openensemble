//! Retail skirmish AI effects that read or write plain player state.

use super::EffectOutcome;
use super::support::used_variable_id;
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::world::World;
use glam::Vec3;

/// Effect DBIDs that jump to the default case of the `BTriggerEffect_fire` switch
/// (`0x14044A0A0`). Retail does nothing for them.
///
/// - 208 and 212: debug variable displays (`DebugVar` family, 207 to 230).
/// - 855: `AIRegisterHook`.
/// - 957: `AIGetMemory`.
const RETAIL_DEFAULT_CASE_EFFECTS: [u16; 4] = [208, 212, 855, 957];

pub(super) fn execute(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::AISetFocus => set_focus(effect, script, world),
        // `BTriggerEffect_teAIBindLog` @ `0x1404828E0` only binds the AI debug log of the
        // trigger, and only with the `aiLog` config. The sim keeps no AI debug log.
        EffectType::AIBindLog => EffectOutcome::Presentation,
        EffectType::Custom if RETAIL_DEFAULT_CASE_EFFECTS.contains(&effect.raw_type) => {
            EffectOutcome::Presentation
        }
        _ => return None,
    };
    Some(outcome)
}

/// `AISetFocus` (711) version 2 (`BTriggerEffect_teAISetFocusV2` @ `0x140482800`).
///
/// The effect sets the look-at position (`mLookAtPos`, `BPlayer + 0`) of the player in
/// signature 1. Retail reads the player without a null check. The position is the first of:
///
/// 1. the location in a non-null signature 2,
/// 2. the position of the live squad in a non-null signature 4,
/// 3. the position of the live unit in a non-null signature 3.
///
/// If no source applies, the position does not change. Other versions do nothing.
fn set_focus(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if effect.version != 2 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(player_id) = effect
        .variable_id(1)
        .and_then(|variable_id| script.get_variable(variable_id))
        .and_then(|variable| match variable.value {
            TriggerValue::Int(value) | TriggerValue::Player(value) => u8::try_from(value).ok(),
            _ => None,
        })
    else {
        return EffectOutcome::Skipped;
    };
    if world.get_player(player_id).is_none() {
        return EffectOutcome::Skipped;
    }
    let value = |signature_id| {
        used_variable_id(effect, script, signature_id)
            .and_then(|variable_id| script.get_variable(variable_id))
            .map(|variable| &variable.value)
    };
    let position = if let Some(value) = value(2) {
        value
            .as_location()
            .map(|location| Vec3::new(location.x, location.y, location.z))
    } else {
        let squad = value(4)
            .and_then(TriggerValue::as_entity)
            .and_then(|squad_id| world.get_squad(squad_id))
            .map(|squad| squad.base.position);
        squad.or_else(|| {
            value(3)
                .and_then(TriggerValue::as_entity)
                .and_then(|unit_id| world.get_unit(unit_id))
                .map(|unit| unit.base.position)
        })
    };
    let Some(position) = position else {
        return EffectOutcome::Skipped;
    };
    if let Some(player) = world.get_player_mut(player_id) {
        player.look_at_position = position;
    }
    EffectOutcome::Applied
}

#[cfg(test)]
mod tests;
