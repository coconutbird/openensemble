//! Retail conditions backed by synchronized game and session settings.

use super::{
    Condition, TriggerScript, TriggerValue, as_i32, as_player_id, compare_ord, operator_at,
};
use crate::world::World;
use pipeline::database::hw1::Database;

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

#[cfg(test)]
mod tests;
