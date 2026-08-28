//! Authoritative implementations of supported retail trigger conditions.

use super::{
    Condition, ConditionMode, ConditionResult, ConditionType, TriggerScript, TriggerValue, VarId,
};
use crate::entities::SquadState;
use crate::entity_id::{EntityClass, EntityId};
use crate::player::{GAIA_PLAYER, PlayerType};
use crate::world::World;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

mod command_state;
mod dispatch;
mod events;
mod game_settings;
mod hitch;
mod iterators;
mod lifecycle;
mod list_selection;
mod queries;
mod sockets;
mod spatial;

use dispatch::evaluate_condition_with_database;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareOperator {
    NotEqual,
    Less,
    LessOrEqual,
    Equal,
    GreaterOrEqual,
    Greater,
}

pub(super) fn evaluate_conditions(
    conditions: &[Condition],
    mode: ConditionMode,
    activated_time: u32,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> ConditionResult {
    if conditions.is_empty() {
        return ConditionResult::True;
    }
    match mode {
        ConditionMode::All => evaluate_all(conditions, activated_time, script, world, database),
        ConditionMode::Any => evaluate_any(conditions, activated_time, script, world, database),
    }
}

fn evaluate_all(
    conditions: &[Condition],
    activated_time: u32,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> ConditionResult {
    for condition in conditions {
        let result =
            evaluate_condition_with_database(condition, activated_time, script, world, database);
        if result != ConditionResult::True {
            return result;
        }
    }
    ConditionResult::True
}

fn evaluate_any(
    conditions: &[Condition],
    activated_time: u32,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> ConditionResult {
    let mut deferred = None;
    for condition in conditions {
        match evaluate_condition_with_database(condition, activated_time, script, world, database) {
            ConditionResult::True => return ConditionResult::True,
            ConditionResult::False => {}
            other => {
                deferred.get_or_insert(other);
            }
        }
    }
    deferred.unwrap_or(ConditionResult::False)
}

#[cfg(test)]
fn evaluate_condition(
    condition: &Condition,
    activated_time: u32,
    script: &mut TriggerScript,
    world: &mut World,
) -> ConditionResult {
    evaluate_condition_with_database(condition, activated_time, script, world, None)
}

fn compare_bool(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(first) = value_at(condition, script, 1).and_then(TriggerValue::as_bool) else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 2) else {
        return false;
    };
    let Some(second) = value_at(condition, script, 3).and_then(TriggerValue::as_bool) else {
        return false;
    };
    match operator {
        CompareOperator::NotEqual => first != second,
        CompareOperator::Equal => first == second,
        _ => false,
    }
}

fn compare_i32(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(first) = value_at(condition, script, 1).and_then(as_i32) else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 2) else {
        return false;
    };
    let Some(second) = value_at(condition, script, 3).and_then(as_i32) else {
        return false;
    };
    compare_ord(&first, operator, &second)
}

fn compare_f32(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(first) = value_at(condition, script, 1).and_then(as_f32) else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 2) else {
        return false;
    };
    let Some(second) = value_at(condition, script, 3).and_then(as_f32) else {
        return false;
    };
    compare_partial(&first, operator, &second)
}

fn compare_time(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(first) = value_at(condition, script, 1).and_then(as_time) else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 2) else {
        return false;
    };
    let Some(second) = value_at(condition, script, 3).and_then(as_time) else {
        return false;
    };
    compare_ord(&first, operator, &second)
}

fn compare_string(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(first) = value_at(condition, script, 1).and_then(as_string) else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 2) else {
        return false;
    };
    let Some(second) = value_at(condition, script, 3).and_then(as_string) else {
        return false;
    };
    compare_ord(first, operator, second)
}

fn compare_cost(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(TriggerValue::Cost(first)) = value_at(condition, script, 1) else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 2) else {
        return false;
    };
    let use_or = value_at(condition, script, 3)
        .and_then(TriggerValue::as_bool)
        .unwrap_or(false);
    let Some(TriggerValue::Cost(second)) = value_at(condition, script, 4) else {
        return false;
    };
    let first = first.amounts();
    let second = second.amounts();
    let comparisons = first
        .iter()
        .zip(&second)
        .map(|(first, second)| compare_partial(first, operator, second));
    if use_or {
        comparisons.into_iter().any(|result| result)
    } else {
        comparisons.into_iter().all(|result| result)
    }
}

fn compare_elapsed(condition: &Condition, script: &TriggerScript, elapsed: u32) -> bool {
    let Some(operator) = operator_at(condition, script, 1) else {
        return false;
    };
    let Some(target) = value_at(condition, script, 2).and_then(as_time) else {
        return false;
    };
    compare_ord(&elapsed, operator, &target)
}

fn time_reached(condition: &Condition, script: &mut TriggerScript, elapsed: u32) -> bool {
    let Some(target) = value_at(condition, script, 1).and_then(as_time) else {
        return false;
    };
    let reached = elapsed >= target;
    if let Some(variable_id) = condition.variable_id(2) {
        write_time(script, variable_id, target.saturating_sub(elapsed));
    }
    reached
}

fn can_pay_cost(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    let Some(TriggerValue::Cost(cost)) = value_at(condition, script, 2) else {
        return false;
    };
    world.get_player(player_id).is_some_and(|player| {
        cost.amounts()
            .into_iter()
            .enumerate()
            .all(|(resource_id, amount)| player.resources.get(resource_id) >= amount)
    })
}

fn check_resource_totals(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    let Some(TriggerValue::Cost(cost)) = value_at(condition, script, 2) else {
        return false;
    };
    world.get_player(player_id).is_some_and(|player| {
        cost.amounts()
            .into_iter()
            .enumerate()
            .all(|(resource_id, amount)| player.get_total_resource(resource_id) >= amount)
    })
}

fn tech_status(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
    database: Option<&Database>,
) -> bool {
    if !matches!(condition.version, 1 | 2) {
        return false;
    }
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    if world.get_player(player_id).is_none() {
        return false;
    }
    let Some(technology_id) = value_at(condition, script, 2).and_then(as_i32) else {
        return false;
    };
    let Some(expected) = value_at(condition, script, 3).and_then(as_i32) else {
        return false;
    };
    let Some(database) = database else {
        return false;
    };
    let technology = usize::try_from(technology_id)
        .ok()
        .and_then(|index| database.techs.get(index));
    if condition.version == 2
        && entity_at(condition, script, 4).is_some_and(|unit_id| world.get_unit(unit_id).is_some())
        && technology.is_some_and(|technology| {
            technology
                .flags
                .iter()
                .any(|flag| flag.trim().eq_ignore_ascii_case("UniqueProtoUnitInstance"))
        })
    {
        return false;
    }
    let actual = world
        .technology_status(player_id, database, technology_id)
        .map_or(0, |status| i32::from(status as u8));
    actual == expected
}

fn compare_population(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    let Some(bucket) = value_at(condition, script, 2)
        .and_then(as_i32)
        .and_then(|value| usize::try_from(value).ok())
    else {
        return false;
    };
    let Some(operator) = operator_at(condition, script, 3) else {
        return false;
    };
    let Some(amount) = value_at(condition, script, 4).and_then(as_f32) else {
        return false;
    };
    world
        .get_player(player_id)
        .and_then(|player| player.get_population(bucket))
        .is_some_and(|population| compare_partial(&population.count, operator, &amount))
}

fn compare_player_unit_count(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    if !matches!(condition.version, 1 | 2) {
        return false;
    }
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    if world.get_player(player_id).is_none() {
        return false;
    }
    let object_type = value_at(condition, script, 2).and_then(as_object_type);
    let count = world.player_unit_count(player_id, object_type);

    // The shipped v2 executable checks slot 5, then uses the still-false local
    // bool as a slot index and reads slot 0. Retail therefore cannot safely add
    // queued units here. Our sparse safe representation treats slot 0 as absent.
    compare_roster_count(condition, script, count)
}

fn compare_player_squad_count(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> bool {
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    if world.get_player(player_id).is_none() {
        return false;
    }
    let prototype_id = value_at(condition, script, 2).and_then(as_i32);
    let include_training = value_at(condition, script, 5)
        .and_then(TriggerValue::as_bool)
        .unwrap_or(false);
    let mut count = world.player_squad_count(player_id, prototype_id);
    if include_training {
        count = count.saturating_add(world.player_future_squad_count(player_id, prototype_id));
    }
    compare_roster_count(condition, script, count)
}

fn compare_roster_count(condition: &Condition, script: &TriggerScript, count: u32) -> bool {
    let Some(operator) = operator_at(condition, script, 3) else {
        return false;
    };
    let Some(expected) = value_at(condition, script, 4).and_then(as_i32) else {
        return false;
    };
    let count = i32::try_from(count).unwrap_or(i32::MAX);
    compare_ord(&count, operator, &expected)
}

fn is_attacking(condition: &Condition, script: &mut TriggerScript, world: &World) -> bool {
    let mut squad_ids = entities_at(condition, script, 4);
    if let Some(squad_id) = entity_at(condition, script, 1) {
        unique_entity(&mut squad_ids, squad_id);
    }
    let targets = squad_ids
        .iter()
        .filter_map(|squad_id| world.get_squad(*squad_id))
        .filter(|squad| squad.state == SquadState::Attacking)
        .filter_map(|squad| squad.attack_target)
        .collect::<Vec<_>>();
    write_attack_targets(condition, script, &targets);
    !targets.is_empty()
}

fn is_under_attack(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(interval) = value_at(condition, script, 2).and_then(as_time) else {
        return false;
    };
    match condition.version {
        1 => entity_at(condition, script, 1)
            .is_some_and(|squad_id| squad_was_recently_damaged(world, squad_id, interval)),
        2 => {
            let mut squad_ids = entities_at(condition, script, 3);
            if let Some(squad_id) = entity_at(condition, script, 1) {
                unique_entity(&mut squad_ids, squad_id);
            }
            squad_ids
                .into_iter()
                .any(|squad_id| squad_was_recently_damaged(world, squad_id, interval))
        }
        _ => false,
    }
}

fn squad_was_recently_damaged(world: &World, squad_id: EntityId, interval: u32) -> bool {
    world.get_squad(squad_id).is_some_and(|squad| {
        squad.last_damaged_time > 0
            && world.game_time_ms.wrapping_sub(squad.last_damaged_time) <= interval
    })
}

fn has_garrisoned(condition: &Condition, script: &mut TriggerScript, world: &World) -> bool {
    let count = entity_at(condition, script, 1)
        .and_then(|squad_id| world.get_squad(squad_id))
        .map_or(0, |squad| squad.garrison.contained_squad_ids().len());
    if let Some(variable_id) = condition.variable_id(2) {
        write_trigger_value(
            script,
            variable_id,
            TriggerValue::Int(i32::try_from(count).unwrap_or(i32::MAX)),
        );
    }
    count > 0
}

fn contains_garrisoned(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(container) = entity_at(condition, script, 1).and_then(|id| world.get_unit(id)) else {
        return false;
    };
    let player_id = value_at(condition, script, 2).and_then(as_player_id);
    let object_type = value_at(condition, script, 3).and_then(as_object_type);

    container
        .garrison
        .contained_unit_ids()
        .iter()
        .any(|unit_id| {
            world.get_unit(*unit_id).is_some_and(|unit| {
                player_id.is_none_or(|expected| unit.base.player_id == expected)
                    && object_type.is_none_or(|expected| unit.is_object_type(expected))
            })
        })
}

fn is_garrisoned(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let mut squad_ids = entities_at(condition, script, 2);
    if let Some(squad_id) = entity_at(condition, script, 1) {
        unique_entity(&mut squad_ids, squad_id);
    }
    squad_ids.into_iter().any(|squad_id| {
        world
            .get_squad(squad_id)
            .is_some_and(|squad| squad.garrison.is_garrisoned())
    })
}

fn write_attack_targets(condition: &Condition, script: &mut TriggerScript, targets: &[EntityId]) {
    let unit_targets = targets
        .iter()
        .copied()
        .filter(|target| target.class() == Some(EntityClass::Unit))
        .collect::<Vec<_>>();
    let squad_targets = targets
        .iter()
        .copied()
        .filter(|target| target.class() == Some(EntityClass::Squad))
        .collect::<Vec<_>>();
    if let Some(variable_id) = condition.variable_id(2)
        && let Some(target) = unit_targets.first()
    {
        write_trigger_value(script, variable_id, TriggerValue::Unit(*target));
    }
    if let Some(variable_id) = condition.variable_id(3)
        && let Some(target) = squad_targets.first()
    {
        write_trigger_value(script, variable_id, TriggerValue::Squad(*target));
    }
    if let Some(variable_id) = condition.variable_id(5) {
        write_trigger_value(script, variable_id, TriggerValue::UnitList(unit_targets));
    }
    if let Some(variable_id) = condition.variable_id(6) {
        write_trigger_value(script, variable_id, TriggerValue::SquadList(squad_targets));
    }
}

fn is_owned_by(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let player = value_at(condition, script, 2).and_then(as_player_id);
    let team = value_at(condition, script, 8).and_then(as_i32);
    let entities = [4, 5, 6, 7]
        .into_iter()
        .flat_map(|signature_id| entities_at(condition, script, signature_id))
        .collect::<Vec<_>>();
    entities.into_iter().all(|entity_id| {
        let Some(owner) = entity_owner(world, entity_id) else {
            return false;
        };
        let player_matches = player.is_some_and(|player_id| owner == player_id);
        let team_matches = team.is_some_and(|team_id| {
            world
                .get_player(owner)
                .is_some_and(|owner| i32::from(owner.team_id) == team_id)
        });
        player_matches || team_matches
    })
}

fn is_proto_object(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(proto_id) = value_at(condition, script, 2).and_then(as_i32) else {
        return false;
    };
    entity_at(condition, script, 3)
        .and_then(|unit_id| world.get_unit(unit_id))
        .is_some_and(|unit| unit.proto_object_id == proto_id)
}

fn is_object_type(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(object_type) = value_at(condition, script, 2).and_then(as_object_type) else {
        return false;
    };
    match condition.version {
        1 => entity_at(condition, script, 1).is_some_and(|squad_id| {
            world.squad_object_type_match(squad_id, object_type) == Some(true)
        }),
        2 => is_object_type_v2(condition, script, world, object_type),
        _ => false,
    }
}

fn is_object_type_v2(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
    object_type: &str,
) -> bool {
    let mut one_used = false;
    let mut all_match = true;

    if let Some(unit_id) = entity_at(condition, script, 3) {
        one_used = true;
        if let Some(matches) = world.unit_object_type_match(unit_id, object_type) {
            all_match = matches;
        }
    }
    if all_match && let Some(squad_id) = entity_at(condition, script, 1) {
        one_used = true;
        if let Some(matches) = world.squad_object_type_match(squad_id, object_type) {
            all_match = matches;
        }
    }
    if all_match && let Some(object_id) = entity_at(condition, script, 4) {
        one_used = true;
        if let Some(matches) = world.entity_object_type_match(object_id, object_type) {
            all_match = matches;
        }
    }
    if all_match && let Some(prototype_id) = value_at(condition, script, 5).and_then(as_i32) {
        one_used = true;
        if let Some(matches) = world.prototype_object_type_match(prototype_id, object_type) {
            all_match = matches;
        }
    }

    one_used && all_match
}

fn player_in_state(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    if condition.version != 2 {
        return false;
    }
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    let Some(expected) = value_at(condition, script, 3).and_then(as_i32) else {
        return false;
    };
    world
        .get_player(player_id)
        .is_some_and(|player| i32::from(player.state as u8) == expected)
}

fn compare_identity(condition: &Condition, script: &TriggerScript) -> bool {
    let Some(first) = value_at(condition, script, 1).and_then(as_i32) else {
        return false;
    };
    let Some(operator @ (CompareOperator::Equal | CompareOperator::NotEqual)) =
        operator_at(condition, script, 2)
    else {
        return false;
    };
    let Some(second) = value_at(condition, script, 3).and_then(as_i32) else {
        return false;
    };
    compare_ord(&first, operator, &second)
}

fn player_using_leader(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    let Some(player_id) = value_at(condition, script, 1).and_then(as_player_id) else {
        return false;
    };
    let Some(leader_id) = value_at(condition, script, 2).and_then(as_i32) else {
        return false;
    };
    world
        .get_player(player_id)
        .is_some_and(|player| player.leader_id == leader_id)
}

fn check_diplomacy(condition: &Condition, script: &TriggerScript, world: &World) -> bool {
    if !(1..=4).any(|signature_id| value_at(condition, script, signature_id).is_some()) {
        return false;
    }
    let Some(reference_team) = diplomacy_reference_team(condition, script, world) else {
        return false;
    };
    let Some(relation_type) = value_at(condition, script, 5).and_then(as_i32) else {
        return false;
    };
    diplomacy_teams(condition, script, world)
        .into_iter()
        .all(|team_id| {
            i32::from(world.team_relation(reference_team, team_id) as u8) == relation_type
        })
}

fn diplomacy_reference_team(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
) -> Option<u8> {
    if value_at(condition, script, 6).is_some() {
        return value_at(condition, script, 6)
            .and_then(as_player_id)
            .and_then(|player_id| world.get_player(player_id))
            .map(|player| player.team_id);
    }
    value_at(condition, script, 7)
        .and_then(as_i32)
        .and_then(|team_id| u8::try_from(team_id).ok())
}

fn diplomacy_teams(condition: &Condition, script: &TriggerScript, world: &World) -> Vec<u8> {
    let mut teams = Vec::new();
    for entity_id in entities_at(condition, script, 1)
        .into_iter()
        .chain(entities_at(condition, script, 2))
    {
        if let Some(team_id) = world
            .get_unit(entity_id)
            .and_then(|unit| world.get_player(unit.base.player_id))
            .map(|player| player.team_id)
        {
            unique_team(&mut teams, team_id);
        }
    }
    for entity_id in entities_at(condition, script, 3)
        .into_iter()
        .chain(entities_at(condition, script, 4))
    {
        if let Some(team_id) = world
            .get_squad(entity_id)
            .and_then(|squad| world.get_player(squad.base.player_id))
            .map(|player| player.team_id)
        {
            unique_team(&mut teams, team_id);
        }
    }
    teams
}

fn unique_team(teams: &mut Vec<u8>, team_id: u8) {
    if let Err(index) = teams.binary_search(&team_id) {
        teams.insert(index, team_id);
    }
}

fn player_type(
    condition: &Condition,
    script: &TriggerScript,
    world: &World,
    expected: PlayerType,
) -> bool {
    value_at(condition, script, 1)
        .and_then(as_player_id)
        .and_then(|player_id| world.get_player(player_id))
        .is_some_and(|player| player.player_type == expected)
}

fn player_is_gaia(condition: &Condition, script: &TriggerScript) -> bool {
    value_at(condition, script, 1).and_then(as_player_id) == Some(GAIA_PLAYER)
}

fn value_at<'a>(
    condition: &Condition,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a TriggerValue> {
    let variable_id = condition.variable_id(signature_id)?;
    script
        .get_variable(variable_id)
        .filter(|variable| !variable.is_null)
        .map(|variable| &variable.value)
}

fn operator_at(
    condition: &Condition,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<CompareOperator> {
    value_at(condition, script, signature_id).and_then(parse_operator)
}

fn parse_operator(value: &TriggerValue) -> Option<CompareOperator> {
    match value {
        TriggerValue::Int(value) => match value {
            0 => Some(CompareOperator::NotEqual),
            1 => Some(CompareOperator::Less),
            2 => Some(CompareOperator::LessOrEqual),
            3 => Some(CompareOperator::Equal),
            4 => Some(CompareOperator::GreaterOrEqual),
            5 => Some(CompareOperator::Greater),
            _ => None,
        },
        TriggerValue::String(value) => match value.as_str() {
            "NotEqualTo" => Some(CompareOperator::NotEqual),
            "LessThan" => Some(CompareOperator::Less),
            "LessThanOrEqualTo" => Some(CompareOperator::LessOrEqual),
            "EqualTo" => Some(CompareOperator::Equal),
            "GreaterThanOrEqualTo" => Some(CompareOperator::GreaterOrEqual),
            "GreaterThan" => Some(CompareOperator::Greater),
            _ => None,
        },
        _ => None,
    }
}

fn as_i32(value: &TriggerValue) -> Option<i32> {
    match value {
        TriggerValue::Int(value)
        | TriggerValue::Player(value)
        | TriggerValue::Team(value)
        | TriggerValue::ProtoObject(value)
        | TriggerValue::ProtoSquad(value)
        | TriggerValue::Tech(value)
        | TriggerValue::Objective(value) => Some(*value),
        TriggerValue::Trigger(value) => i32::try_from(*value).ok(),
        _ => None,
    }
}

fn as_f32(value: &TriggerValue) -> Option<f32> {
    match value {
        TriggerValue::Float(value) => Some(*value),
        TriggerValue::Int(value) => value.to_f32(),
        _ => None,
    }
}

fn as_time(value: &TriggerValue) -> Option<u32> {
    match value {
        TriggerValue::Time(value) => Some(*value),
        TriggerValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn as_string(value: &TriggerValue) -> Option<&str> {
    match value {
        TriggerValue::String(value) => Some(value),
        _ => None,
    }
}

fn as_object_type(value: &TriggerValue) -> Option<&str> {
    match value {
        TriggerValue::ObjectType(value) => Some(value),
        _ => None,
    }
}

fn as_player_id(value: &TriggerValue) -> Option<u8> {
    as_i32(value).and_then(|value| u8::try_from(value).ok())
}

fn entity_at(condition: &Condition, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    value_at(condition, script, signature_id).and_then(TriggerValue::as_entity)
}

fn entities_at(condition: &Condition, script: &TriggerScript, signature_id: u16) -> Vec<EntityId> {
    match value_at(condition, script, signature_id) {
        Some(
            TriggerValue::EntityList(values)
            | TriggerValue::UnitList(values)
            | TriggerValue::SquadList(values)
            | TriggerValue::ObjectList(values),
        ) => values.clone(),
        Some(value) => value.as_entity().into_iter().collect(),
        None => Vec::new(),
    }
}

fn entity_inputs(condition: &Condition, script: &TriggerScript) -> Vec<EntityId> {
    let mut entities = Vec::new();
    for signature_id in 3..=6 {
        for entity_id in entities_at(condition, script, signature_id) {
            unique_entity(&mut entities, entity_id);
        }
    }
    entities
}

fn unique_entity(entities: &mut Vec<EntityId>, entity_id: EntityId) {
    if let Err(index) = entities.binary_search(&entity_id) {
        entities.insert(index, entity_id);
    }
}

fn entity_owner(world: &World, entity_id: EntityId) -> Option<u8> {
    match entity_id.class() {
        Some(EntityClass::Unit) => world.get_unit(entity_id).map(|unit| unit.base.player_id),
        Some(EntityClass::Squad) => world.get_squad(entity_id).map(|squad| squad.base.player_id),
        _ => None,
    }
}

fn write_time(script: &mut TriggerScript, variable_id: VarId, value: u32) {
    write_trigger_value(script, variable_id, TriggerValue::Time(value));
}

fn write_trigger_value(script: &mut TriggerScript, variable_id: VarId, value: TriggerValue) {
    if let Some(variable) = script.get_variable_mut(variable_id) {
        variable.value = value;
        variable.is_null = false;
    }
}

fn compare_ord<T: Ord + ?Sized>(first: &T, operator: CompareOperator, second: &T) -> bool {
    match operator {
        CompareOperator::NotEqual => first != second,
        CompareOperator::Less => first < second,
        CompareOperator::LessOrEqual => first <= second,
        CompareOperator::Equal => first == second,
        CompareOperator::GreaterOrEqual => first >= second,
        CompareOperator::Greater => first > second,
    }
}

fn compare_partial<T: PartialOrd + ?Sized>(
    first: &T,
    operator: CompareOperator,
    second: &T,
) -> bool {
    match operator {
        CompareOperator::NotEqual => first != second,
        CompareOperator::Less => first < second,
        CompareOperator::LessOrEqual => first <= second,
        CompareOperator::Equal => first == second,
        CompareOperator::GreaterOrEqual => first >= second,
        CompareOperator::Greater => first > second,
    }
}

#[cfg(test)]
mod tests;
