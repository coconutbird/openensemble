//! Retail conditions backed by synchronized game and session settings.

use super::{
    Condition, TriggerScript, TriggerValue, as_i32, as_player_id, compare_ord, operator_at,
    used_variable_id, write_trigger_value,
};
use crate::player::{GAIA_PLAYER, PlayerType};
use crate::world::World;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::aidifficultysettings::PiecewiseDataPoint;

/// Size of the `BAI` array of the world (`gWorld + 648`).
const MAX_AI_PLAYERS: u8 = 9;

const RETAIL_DIFFICULTY_NORMAL: f32 = 0.34;
const RETAIL_DIFFICULTY_HARD: f32 = 0.67;
const RETAIL_DIFFICULTY_LEGENDARY: f32 = 1.0;

pub(super) fn is_coop(world: &World) -> bool {
    world.is_coop()
}

pub(super) fn is_config_defined(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> bool {
    let Some(TriggerValue::String(name)) = super::value_at(condition, script, 1) else {
        return false;
    };
    !name.is_empty() && name.is_ascii() && world.is_config_defined(name)
}

pub(super) fn check_difficulty(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
    database: Option<&Database>,
) -> bool {
    let Some(player_id) = super::value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    let Some(player) = world.get_player(player_id) else {
        return false;
    };
    let Some(expected) = super::value_at(condition, script, 2).and_then(as_i32) else {
        return false;
    };
    let actual = difficulty_type(player.difficulty, database);

    match condition.version {
        1 => actual == expected,
        2 => operator_at(condition, script, 3)
            .is_some_and(|operator| compare_ord(&actual, operator, &expected)),
        _ => false,
    }
}

fn difficulty_type(difficulty: f32, database: Option<&Database>) -> i32 {
    let game_data = database.and_then(|database| database.game_data.as_ref());
    let normal = game_data
        .and_then(|data| data.difficulty_normal)
        .unwrap_or(RETAIL_DIFFICULTY_NORMAL);
    let hard = game_data
        .and_then(|data| data.difficulty_hard)
        .unwrap_or(RETAIL_DIFFICULTY_HARD);
    let legendary = game_data
        .and_then(|data| data.difficulty_legendary)
        .unwrap_or(RETAIL_DIFFICULTY_LEGENDARY);

    if difficulty < normal {
        0
    } else if difficulty >= normal && difficulty < hard {
        1
    } else if difficulty >= hard && difficulty < legendary {
        2
    } else {
        3
    }
}

/// `AICanGetDifficultySetting` (920, `BTriggerCondition_tcAICanGetDifficultySetting`
/// @ `0x14042FED0`).
///
/// The player in signature 1 must have a `BAI`: `BWorld_initPlayerAIs` gives one to each player
/// other than Gaia that is not human. The condition finds the AI difficulty setting with the name
/// in signature 2 (`BAI_findDifficultySetting`, the name test ignores case).
///
/// - A null signature 3 writes the setting function of the player difficulty to signature 4
///   (`BPiecewiseFunc_evaluate`) and gives true.
/// - Otherwise signature 4 gets the `DefaultValue` with the name in signature 3. The condition
///   gives false if the setting has no value with that name.
///
/// A null signature 4 gets no value. Retail reads signatures 1 to 3 without a null check.
pub(super) fn ai_can_get_difficulty_setting(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &World,
    database: Option<&Database>,
) -> bool {
    let raw = |signature_id| {
        condition
            .variable_id(signature_id)
            .and_then(|variable_id| script.get_variable(variable_id))
            .map(|variable| &variable.value)
    };
    let Some(player) = raw(1)
        .and_then(as_player_id)
        .filter(|player_id| *player_id < MAX_AI_PLAYERS)
        .and_then(|player_id| world.get_player(player_id))
        .filter(|player| player.id != GAIA_PLAYER && player.player_type != PlayerType::Human)
    else {
        return false;
    };
    let Some(TriggerValue::String(name)) = raw(2) else {
        return false;
    };
    let Some(setting) = database.and_then(|database| {
        database
            .ai_difficulty_settings
            .iter()
            .find(|setting| setting.name.eq_ignore_ascii_case(name))
    }) else {
        return false;
    };
    let value = if used_variable_id(condition, script, 3).is_none() {
        let points = setting
            .function
            .as_ref()
            .map_or(&[][..], |function| &function.points);
        piecewise_value(points, player.difficulty)
    } else {
        let Some(TriggerValue::String(default_name)) = raw(3) else {
            return false;
        };
        let Some(default) = setting
            .default_values
            .iter()
            .find(|default| default.name.eq_ignore_ascii_case(default_name))
        else {
            return false;
        };
        default.value
    };
    if let Some(output_id) = used_variable_id(condition, script, 4) {
        write_trigger_value(script, output_id, TriggerValue::Float(value));
    }
    true
}

/// The value of a piecewise linear function at `input` (`BPiecewiseFunc_evaluate`
/// @ `0x1402DA620`).
///
/// `BPiecewiseFunc_load` @ `0x1402DA220` marks the function as valid only if it has two or more
/// points, the inputs do not decrease and no input occurs more than twice. An invalid function
/// gives 0. The function clamps an input outside the points to the first or last output.
fn piecewise_value(points: &[PiecewiseDataPoint], input: f32) -> f32 {
    // With inputs that do not decrease, `a < b || b < c` is false only for three equal inputs.
    let valid = points.len() >= 2
        && points.windows(2).all(|pair| pair[0].input <= pair[1].input)
        && points
            .windows(3)
            .all(|run| run[0].input < run[1].input || run[1].input < run[2].input);
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return 0.0;
    };
    if !valid {
        return 0.0;
    }
    if first.input > input {
        return first.output;
    }
    if input <= last.input {
        for pair in points.windows(2) {
            let (low, high) = (pair[0], pair[1]);
            if high.input >= input {
                let width = high.input - low.input;
                let fraction = if width <= 0.0 {
                    1.0
                } else {
                    (input - low.input) / width
                };
                return (high.output - low.output) * fraction + low.output;
            }
        }
        return 0.0;
    }
    last.output
}

#[cfg(test)]
mod tests;
