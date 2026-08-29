//! Retail trigger rumble start/stop bindings.

use super::{optional_bool, optional_float, optional_value, player_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::{RumbleMotor, World};

use super::super::EffectOutcome;

struct RumbleSpec {
    player_id: crate::player::PlayerId,
    left: RumbleMotor,
    right: RumbleMotor,
    duration_seconds: f32,
    looped: bool,
    pattern: Option<String>,
}

pub(super) fn start(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    write_output_id(effect, script, -1);
    let spec = match effect.version {
        1 => version_one(effect, script),
        2 => version_two(effect, script),
        3 | 4 => version_three_or_four(effect, script),
        _ => return EffectOutcome::Unsupported(effect.raw_type),
    };
    let Ok(spec) = spec else {
        return EffectOutcome::Skipped;
    };
    let Some(id) = world.start_rumble(
        spec.player_id,
        spec.left,
        spec.right,
        spec.duration_seconds,
        spec.looped,
        spec.pattern,
    ) else {
        return EffectOutcome::Presentation;
    };
    write_output_id(effect, script, id);
    EffectOutcome::Presentation
}

pub(super) fn stop(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let (Some(player_id), Some(rumble_id)) = (
        player_value(effect, script, 1),
        optional_value(effect, script, 2).and_then(TriggerValue::as_int),
    ) else {
        return EffectOutcome::Skipped;
    };
    world.stop_rumble(player_id, rumble_id);
    EffectOutcome::Presentation
}

fn version_one(effect: &Effect, script: &TriggerScript) -> Result<RumbleSpec, ()> {
    let player_id = player_value(effect, script, 8).ok_or(())?;
    let left_type = optional_string(effect, script, 1)?;
    let right_type = optional_string(effect, script, 2)?;
    let duration_seconds = optional_float(effect, script, 3)?.unwrap_or(1.0);
    let strength = optional_float(effect, script, 5)?.unwrap_or(1.0);
    let looped = optional_bool(effect, script, 6)?.unwrap_or(false);
    Ok(RumbleSpec {
        player_id,
        left: RumbleMotor::new(left_type, strength),
        right: RumbleMotor::new(right_type, strength),
        duration_seconds,
        looped,
        pattern: None,
    })
}

fn version_two(effect: &Effect, script: &TriggerScript) -> Result<RumbleSpec, ()> {
    let player_id = player_value(effect, script, 8).ok_or(())?;
    let motor = optional_string(effect, script, 1)?;
    let rumble_type = optional_string(effect, script, 2)?;
    let duration_seconds = optional_float(effect, script, 3)?.unwrap_or(1.0);
    let strength = optional_float(effect, script, 5)?.unwrap_or(1.0);
    let looped = optional_bool(effect, script, 6)?.unwrap_or(false);
    let disabled = || RumbleMotor::new(None, 0.0);
    let enabled = || RumbleMotor::new(rumble_type.clone(), strength);
    let (left, right) = match motor.as_deref().map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("Both") || value.ends_with("Both") => {
            (enabled(), enabled())
        }
        Some(value) if value.eq_ignore_ascii_case("Left") || value.ends_with("Left") => {
            (enabled(), disabled())
        }
        Some(value) if value.eq_ignore_ascii_case("Right") || value.ends_with("Right") => {
            (disabled(), enabled())
        }
        _ => return Err(()),
    };
    Ok(RumbleSpec {
        player_id,
        left,
        right,
        duration_seconds,
        looped,
        pattern: None,
    })
}

fn version_three_or_four(effect: &Effect, script: &TriggerScript) -> Result<RumbleSpec, ()> {
    let player_id = player_value(effect, script, 8).ok_or(())?;
    let left_type = optional_string(effect, script, 2)?;
    let left_strength = optional_float(effect, script, 5)?.unwrap_or(1.0);
    let right_type = optional_string(effect, script, 9)?;
    let right_strength = optional_float(effect, script, 11)?.unwrap_or(1.0);
    let duration_seconds = optional_float(effect, script, 3)?.unwrap_or(1.0);
    let looped = optional_bool(effect, script, 6)?.unwrap_or(false);
    let pattern = if effect.version >= 4 {
        optional_string(effect, script, 12)?.filter(|value| !value.trim().is_empty())
    } else {
        None
    };
    Ok(RumbleSpec {
        player_id,
        left: RumbleMotor::new(left_type, left_strength),
        right: RumbleMotor::new(right_type, right_strength),
        duration_seconds,
        looped,
        pattern,
    })
}

fn optional_string(
    effect: &Effect,
    script: &TriggerScript,
    slot: u16,
) -> Result<Option<String>, ()> {
    let Some(value) = optional_value(effect, script, slot) else {
        return Ok(None);
    };
    let TriggerValue::String(value) = value else {
        return Err(());
    };
    Ok(Some(value.clone()))
}

fn write_output_id(effect: &Effect, script: &mut TriggerScript, id: i32) {
    let Some(variable_id) = effect.variable_id(7) else {
        return;
    };
    let Some(variable) = script.get_variable_mut(variable_id) else {
        return;
    };
    variable.value = TriggerValue::Int(id);
    variable.is_null = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{EffectType, TriggerVar, VarType};

    #[test]
    fn version_four_starts_and_stops_a_trigger_visible_request() {
        let mut world = World::new();
        world.init_players(2);
        let mut script = TriggerScript::new(1);
        add(&mut script, 2, VarType::RumbleType, string("Fixed"));
        add(&mut script, 3, VarType::Float, TriggerValue::Float(1.0));
        add(&mut script, 5, VarType::Float, TriggerValue::Float(0.75));
        add(&mut script, 6, VarType::Bool, TriggerValue::Bool(false));
        add(&mut script, 7, VarType::Integer, TriggerValue::Int(-1));
        add(&mut script, 8, VarType::Player, TriggerValue::Player(2));
        add(&mut script, 9, VarType::RumbleType, string("Fixed"));
        add(&mut script, 11, VarType::Float, TriggerValue::Float(0.75));
        let mut effect = Effect::new(1, EffectType::RumbleStart).with_output_at(7, 7);
        effect.version = 4;
        for slot in [2_u16, 3, 5, 6, 8, 9, 11] {
            effect = effect.with_input_at(slot, u32::from(slot));
        }

        assert_eq!(
            start(&effect, &mut script, &mut world),
            EffectOutcome::Presentation
        );
        let request = world.rumble_requests(2).next().unwrap();
        assert_eq!(request.left().rumble_type(), Some("Fixed"));
        assert_eq!(request.right().strength().to_bits(), 0.75_f32.to_bits());
        assert_eq!(script.get_variable(7).unwrap().value, TriggerValue::Int(0));

        add(&mut script, 20, VarType::Player, TriggerValue::Player(2));
        add(&mut script, 21, VarType::Integer, TriggerValue::Int(0));
        let stop_effect = Effect::new(2, EffectType::RumbleStop)
            .with_input_at(1, 20)
            .with_input_at(2, 21);
        assert_eq!(
            stop(&stop_effect, &script, &mut world),
            EffectOutcome::Presentation
        );
        assert!(world.rumble_requests(2).next().is_none());
    }

    fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
        script.add_variable(TriggerVar::new(id, var_type).with_value(value));
    }

    fn string(value: &str) -> TriggerValue {
        TriggerValue::String(value.to_owned())
    }
}
