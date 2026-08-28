//! Condition routing kept separate from individual retail implementations.

use super::{
    Condition, ConditionResult, ConditionType, Database, PlayerType, TriggerScript, World,
    can_pay_cost, check_diplomacy, check_resource_totals, command_state, compare_bool,
    compare_cost, compare_elapsed, compare_f32, compare_i32, compare_identity,
    compare_player_squad_count, compare_player_unit_count, compare_population, compare_string,
    compare_time, contains_garrisoned, events, game_settings, has_garrisoned, hitch, is_attacking,
    is_garrisoned, is_object_type, is_owned_by, is_proto_object, is_under_attack, iterators,
    lifecycle, list_selection, player_in_state, player_is_gaia, player_type, player_using_leader,
    queries, sockets, spatial, tech_status, time_reached,
};

pub(super) fn evaluate_condition_with_database(
    condition: &Condition,
    activated_time: u32,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> ConditionResult {
    if condition.is_async {
        return ConditionResult::Waiting;
    }
    let result = if let Some(result) =
        evaluate_core_condition(condition, activated_time, script, world, database)
    {
        result
    } else if let Some(result) = evaluate_query_condition(condition, script, world, database) {
        result
    } else {
        return ConditionResult::Unsupported(condition.raw_type);
    };
    let result = if condition.invert { !result } else { result };
    if result {
        ConditionResult::True
    } else {
        ConditionResult::False
    }
}

fn evaluate_core_condition(
    condition: &Condition,
    activated_time: u32,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> Option<bool> {
    let result = match condition.condition_type {
        ConditionType::CompareBool => compare_bool(condition, script),
        ConditionType::CompareCount
        | ConditionType::ComparePlayers
        | ConditionType::CompareTeams
        | ConditionType::CompareProtoSquad => compare_i32(condition, script),
        ConditionType::CompareFloat
        | ConditionType::ComparePercent
        | ConditionType::CompareHitpoints => compare_f32(condition, script),
        ConditionType::CompareTime => compare_time(condition, script),
        ConditionType::CompareString => compare_string(condition, script),
        ConditionType::CompareVector => spatial::compare_vector(condition, script),
        ConditionType::CompareCost => compare_cost(condition, script),
        ConditionType::CanPayCost => can_pay_cost(condition, script, world),
        ConditionType::CheckResourceTotals => check_resource_totals(condition, script, world),
        ConditionType::TechStatus => tech_status(condition, script, world, database),
        ConditionType::TriggerActiveTime => compare_elapsed(
            condition,
            script,
            world.game_time_ms.wrapping_sub(activated_time),
        ),
        ConditionType::GameTime => compare_elapsed(condition, script, world.game_time_ms),
        ConditionType::GameTimeReached => time_reached(condition, script, world.game_time_ms),
        ConditionType::TriggerActiveTimeReached => time_reached(
            condition,
            script,
            world.game_time_ms.wrapping_sub(activated_time),
        ),
        ConditionType::ComparePopulation => compare_population(condition, script, world),
        ConditionType::ComparePlayerUnitCount => {
            compare_player_unit_count(condition, script, world)
        }
        ConditionType::ComparePlayerSquadCount => {
            compare_player_squad_count(condition, script, world)
        }
        ConditionType::IsAlive => lifecycle::entity_liveness(condition, script, world, true),
        ConditionType::IsDead => lifecycle::entity_liveness(condition, script, world, false),
        ConditionType::IsBuilt => lifecycle::is_built(condition, script, world),
        ConditionType::IsSquadAtMaxSize => {
            lifecycle::is_squad_at_max_size(condition, script, world)
        }
        ConditionType::IsMoving => lifecycle::is_moving(condition, script, world),
        ConditionType::IsIdle => lifecycle::is_idle(condition, script, world),
        ConditionType::IsAttacking => is_attacking(condition, script, world),
        ConditionType::IsUnderAttack => is_under_attack(condition, script, world),
        ConditionType::ContainsGarrisoned => contains_garrisoned(condition, script, world),
        ConditionType::HasGarrisoned => has_garrisoned(condition, script, world),
        ConditionType::IsGarrisoned => is_garrisoned(condition, script, world),
        ConditionType::IsOwnedBy => is_owned_by(condition, script, world),
        ConditionType::IsProtoObject => is_proto_object(condition, script, world),
        ConditionType::IsObjectType => is_object_type(condition, script, world),
        ConditionType::PlayerInState => player_in_state(condition, script, world),
        ConditionType::CompareCiv | ConditionType::CompareLeader => {
            compare_identity(condition, script)
        }
        ConditionType::PlayerUsingLeader => player_using_leader(condition, script, world),
        ConditionType::CheckDiplomacy => check_diplomacy(condition, script, world),
        _ => return None,
    };
    Some(result)
}

fn evaluate_query_condition(
    condition: &Condition,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> Option<bool> {
    let result = match condition.condition_type {
        ConditionType::PlayerIsHuman => player_type(condition, script, world, PlayerType::Human),
        ConditionType::PlayerIsGaia => player_is_gaia(condition, script),
        ConditionType::PlayerIsComputerAI => {
            player_type(condition, script, world, PlayerType::ComputerAi)
        }
        ConditionType::IsCoop => game_settings::is_coop(world),
        ConditionType::IsConfigDefined => {
            game_settings::is_config_defined(condition, script, world)
        }
        ConditionType::CheckDifficulty => {
            game_settings::check_difficulty(condition, script, world, database)
        }
        ConditionType::CanGetUnits => queries::can_get_units(condition, script, world),
        ConditionType::CanGetSquads => queries::can_get_squads(condition, script, world),
        ConditionType::CanGetOneUnit => list_selection::can_get_one_unit(condition, script, world),
        ConditionType::CanGetOneSquad => {
            list_selection::can_get_one_squad(condition, script, world)
        }
        ConditionType::CanGetOnePlayer => {
            list_selection::can_get_one_player(condition, script, world)
        }
        ConditionType::CanGetOneTeam => list_selection::can_get_one_team(condition, script, world),
        ConditionType::CanGetOneProtoObject => {
            list_selection::can_get_one_proto_object(condition, script, world)
        }
        ConditionType::CanGetOneProtoSquad => {
            list_selection::can_get_one_proto_squad(condition, script, world)
        }
        ConditionType::CanGetOneObjectType => {
            list_selection::can_get_one_object_type(condition, script, world)
        }
        ConditionType::CanGetOneTech => list_selection::can_get_one_tech(condition, script, world),
        ConditionType::CanGetOneInteger => {
            list_selection::can_get_one_integer(condition, script, world)
        }
        ConditionType::CanGetOneLocation => {
            list_selection::can_get_one_location(condition, script, world)
        }
        ConditionType::SquadLocationDistance => {
            spatial::squad_location_distance(condition, script, world)
        }
        ConditionType::EventTriggered => events::event_triggered(condition, script, world),
        ConditionType::ChatCompleted => events::chat_completed(condition, script, world),
        ConditionType::CinematicCompleted => events::cinematic_completed(world),
        ConditionType::BuildingCommandDone => {
            command_state::building_command_done(condition, script, world)
        }
        ConditionType::CustomCommandCheck => {
            command_state::custom_command_check(condition, script, world)
        }
        ConditionType::IsHitched => hitch::is_hitched(condition, script, world),
        ConditionType::HasHitched => hitch::has_hitched(condition, script, world),
        ConditionType::CanGetSocketUnits => sockets::can_get_socket_units(condition, script, world),
        ConditionType::CanGetOneSocketUnit => {
            sockets::can_get_one_socket_unit(condition, script, world)
        }
        ConditionType::IsEmptySocketUnit => sockets::is_empty_socket_unit(condition, script, world),
        ConditionType::CanGetSocketParentBuilding => {
            sockets::can_get_socket_parent_building(condition, script, world)
        }
        ConditionType::CanGetSocketPlugUnit => {
            sockets::can_get_socket_plug_unit(condition, script, world)
        }
        ConditionType::NextPlayer => iterators::next_player(condition, script),
        ConditionType::NextTeam => iterators::next_team(condition, script),
        ConditionType::NextUnit => iterators::next_unit(condition, script),
        ConditionType::NextSquad => iterators::next_squad(condition, script),
        ConditionType::NextObject => iterators::next_object(condition, script),
        ConditionType::NextLocation => iterators::next_location(condition, script),
        _ => return None,
    };
    Some(result)
}
