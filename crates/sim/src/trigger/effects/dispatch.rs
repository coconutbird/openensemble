//! Effect routing kept separate from individual retail implementations.

use super::{
    ControlAction, Database, Effect, EffectOutcome, EffectType, GameplayCatalog, TriggerScript,
    World, adjust_count, ai, commands, copy_value, design_lines, economy, entities, entity_flags,
    entity_visuals, events, execute_control_effect, filters, fog, forbids, game_state, health,
    is_copy_effect, iterators, list_processing, lists, math, objectives, orders, ownership, powers,
    presentation, proto_data, rally_points, relationships, resources, revealers, set_teleporter,
    spatial, support, timers, tower_walls, unit_data, value_lists,
};

pub(crate) fn execute_effect(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
    gameplay: Option<&GameplayCatalog>,
) -> (EffectOutcome, Option<ControlAction>) {
    if let Some(control_result) = execute_control_effect(effect, script) {
        return control_result;
    }
    if let Some(outcome) = events::execute(effect, script, world) {
        return (outcome, None);
    }
    if let Some(outcome) = game_state::execute(effect, script, world) {
        return (outcome, None);
    }
    if let Some(outcome) = objectives::execute(effect, script, world) {
        return (outcome, None);
    }
    if let Some(outcome) = entity_visuals::execute(effect, script, world) {
        return (outcome, None);
    }
    if let Some(outcome) = entities::execute(effect, script, world, database) {
        return (outcome, None);
    }
    if let Some(outcome) = design_lines::execute(effect, script, world) {
        return (outcome, None);
    }
    if let Some(outcome) = presentation::execute(effect, script, world) {
        return (outcome, None);
    }
    let outcome =
        if let Some(outcome) = execute_world_effect(effect, script, world, database, gameplay) {
            outcome
        } else if let Some(outcome) = execute_collection_effect(effect, script, world, database) {
            outcome
        } else {
            EffectOutcome::Unsupported(effect.raw_type)
        };
    (outcome, None)
}

fn execute_world_effect(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
    gameplay: Option<&GameplayCatalog>,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::CountIncrement => adjust_count(effect, script, 1),
        EffectType::CountDecrement => adjust_count(effect, script, -1),
        EffectType::PayCost => resources::pay_cost(effect, script, world),
        EffectType::RefundCost => resources::refund_cost(effect, script, world),
        EffectType::SetResources => resources::set_resources(effect, script, world, false),
        EffectType::GetResources => resources::get_resources(effect, script, world, false),
        EffectType::SetResourcesTotals => resources::set_resources(effect, script, world, true),
        EffectType::GetResourcesTotals => resources::get_resources(effect, script, world, true),
        EffectType::SetTrickleRate => resources::set_trickle_rate(effect, script, world),
        EffectType::GetTrickleRate => resources::get_trickle_rate(effect, script, world),
        EffectType::GetPlayerPop => resources::get_player_pop(effect, script, world, database),
        EffectType::SetPlayerPop => resources::set_player_pop(effect, script, world, database),
        EffectType::GetPlayerEconomy => {
            economy::get_player_economy(effect, script, world, database)
        }
        EffectType::GetCost => economy::get_cost(effect, script, world, database),
        EffectType::GetPop => economy::get_pop(effect, script, database),
        EffectType::CostToFloat => economy::cost_to_float(effect, script, database),
        EffectType::AIAnalyzeSquadList => {
            ai::analyze_squad_list(effect, script, world, database, gameplay)
        }
        EffectType::AIAnalyzeOffenseAToB => ai::analyze_offense(effect, script),
        EffectType::AISAGetComponent => ai::get_component(effect, script),
        EffectType::AIAnalyzeProtoSquadList => {
            ai::analyze_proto_squad_list(effect, script, world, database, gameplay)
        }
        EffectType::AICalculateOffenseRatioAToB => ai::calculate_offense_ratio(effect, script),
        EffectType::TechActivate => {
            resources::change_technology(effect, script, world, database, true)
        }
        EffectType::TechDeactivate => {
            resources::change_technology(effect, script, world, database, false)
        }
        EffectType::GetUnits => lists::get_units(effect, script, world),
        EffectType::GetSquads => lists::get_squads(effect, script, world),
        EffectType::GetProtoSquad => lists::get_proto_squad(effect, script, world),
        EffectType::IteratorPlayerList => {
            iterators::attach_scalar(effect, script, iterators::ScalarListKind::Player)
        }
        EffectType::IteratorTeamList => {
            iterators::attach_scalar(effect, script, iterators::ScalarListKind::Team)
        }
        EffectType::IteratorObjectList => {
            iterators::attach(effect, script, support::EntityListKind::Object)
        }
        EffectType::IteratorLocationList => iterators::attach_vector(effect, script),
        EffectType::IteratorUnitList => {
            iterators::attach(effect, script, support::EntityListKind::Unit)
        }
        EffectType::IteratorSquadList => {
            iterators::attach(effect, script, support::EntityListKind::Squad)
        }
        EffectType::Unload => orders::unload(effect, script, world),
        EffectType::Move => orders::move_squads(effect, script, world),
        EffectType::MovePath => orders::move_path(effect, script, world),
        EffectType::ChangeOwner => ownership::change_owner(effect, script, world),
        EffectType::GetHealth => health::get_health(effect, script, world),
        EffectType::RandomLocation => spatial::random_location(effect, script, world),
        EffectType::GetLocation => spatial::get_location(effect, script, world),
        EffectType::GetMeanLocation => spatial::get_mean_location(effect, script, world),
        EffectType::GetOwner => ownership::get_owner(effect, script, world),
        EffectType::GetChildUnits => relationships::get_child_units(effect, script, world),
        EffectType::GetParentSquad => relationships::get_parent_squad(effect, script, world),
        EffectType::Work => orders::work(effect, script, world, gameplay),
        EffectType::Repair => health::repair_or_damage(effect, script, world, true),
        EffectType::Damage => health::repair_or_damage(effect, script, world, false),
        EffectType::CombatDamage => health::combat_damage(effect, script, world),
        EffectType::Teleport => spatial::teleport(effect, script, world),
        EffectType::SetPlayableBounds => spatial::set_playable_bounds(effect, script, world),
        EffectType::SetDirection => spatial::set_direction(effect, script, world),
        EffectType::SetMobile => entity_flags::set_mobile(effect, script, world),
        EffectType::SetSelectable => entity_flags::set_selectable(effect, script, world),
        EffectType::SetAutoAttackable => entity_flags::set_auto_attackable(effect, script, world),
        EffectType::ModifyDataScalar => unit_data::modify_data_scalar(effect, script, world),
        EffectType::ModifyProtoData => {
            proto_data::modify_proto_data(effect, script, world, database)
        }
        EffectType::EnableFogOfWar => fog::set_enabled(effect, script, world),
        EffectType::ClearBlackMap | EffectType::ResetBlackMap => fog::black_map(effect, world),
        EffectType::SetTowerWallDestination => {
            tower_walls::set_destination(effect, script, world, gameplay)
        }
        EffectType::Forbid => forbids::set_forbidden(effect, script, world, database),
        EffectType::Revealer => revealers::create(effect, script, world, database),
        EffectType::PowerGrant => powers::grant(effect, script, world, database),
        EffectType::PowerRevoke => powers::revoke(effect, script, world, database),
        EffectType::RallyPointSet => rally_points::set(effect, script, world),
        EffectType::RallyPointClear => rally_points::clear(effect, script, world),
        EffectType::RallyPointGet => rally_points::get(effect, script, world),
        EffectType::GetDirection => spatial::get_direction(effect, script, world),
        EffectType::GetDirectionFromLocations => {
            spatial::get_direction_from_locations(effect, script)
        }
        EffectType::CreateTimer => timers::create(effect, script, world),
        EffectType::DestroyTimer => timers::destroy(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

fn execute_collection_effect(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::UnitListGetSize => {
            lists::list_get_size(effect, script, support::EntityListKind::Unit)
        }
        EffectType::SquadListGetSize => {
            lists::list_get_size(effect, script, support::EntityListKind::Squad)
        }
        EffectType::LocationListAdd
        | EffectType::LocationListRemove
        | EffectType::LocationListGetSize
        | EffectType::ProtoObjectListAdd
        | EffectType::ProtoObjectListRemove
        | EffectType::ProtoSquadListAdd
        | EffectType::ProtoSquadListRemove
        | EffectType::TechListAdd
        | EffectType::TechListRemove
        | EffectType::IntegerListAdd
        | EffectType::IntegerListRemove
        | EffectType::IntegerListGetSize => value_lists::execute(effect, script),
        EffectType::UnitListAdd => {
            lists::list_add(effect, script, world, support::EntityListKind::Unit)
        }
        EffectType::SquadListAdd => {
            lists::list_add(effect, script, world, support::EntityListKind::Squad)
        }
        EffectType::UnitListRemove => {
            lists::list_remove(effect, script, support::EntityListKind::Unit)
        }
        EffectType::SquadListRemove => {
            lists::list_remove(effect, script, support::EntityListKind::Squad)
        }
        EffectType::ObjectListRemove => {
            lists::list_remove(effect, script, support::EntityListKind::Object)
        }
        EffectType::RandomCount => math::random_count(effect, script, world),
        EffectType::MathCount => math::math_count(effect, script),
        EffectType::LerpCount => math::lerp_count(effect, script),
        EffectType::LerpPercent => math::lerp_percent(effect, script),
        EffectType::LerpTime => math::lerp_time(effect, script),
        EffectType::MathFloat => math::math_float(effect, script),
        EffectType::AsFloat => math::as_float(effect, script),
        EffectType::MathTime => math::math_time(effect, script),
        EffectType::RandomTime => math::random_time(effect, script, world),
        EffectType::BuildingCommand => commands::building_command(effect, script, world, database),
        EffectType::ClearBuildingCommandState => {
            commands::clear_building_command_state(effect, script)
        }
        EffectType::CustomCommandAdd => commands::custom_command_add(effect, script, world),
        EffectType::CustomCommandRemove => commands::custom_command_remove(effect, script, world),
        EffectType::SquadListPartition
        | EffectType::UnitListPartition
        | EffectType::SquadListShuffle
        | EffectType::LocationListShuffle
        | EffectType::EntityListShuffle
        | EffectType::PlayerListShuffle
        | EffectType::TeamListShuffle
        | EffectType::UnitListShuffle
        | EffectType::ProtoObjectListShuffle
        | EffectType::ObjectTypeListShuffle
        | EffectType::ProtoSquadListShuffle
        | EffectType::TechListShuffle
        | EffectType::SquadListDiff
        | EffectType::UnitListDiff => list_processing::execute(effect, script, world),
        EffectType::EntityFilterClear
        | EffectType::EntityFilterAddIsAlive
        | EffectType::EntityFilterAddInList
        | EffectType::EntityFilterAddPlayers
        | EffectType::EntityFilterAddTeams
        | EffectType::EntityFilterAddProtoObjects
        | EffectType::EntityFilterAddProtoSquads
        | EffectType::EntityFilterAddObjectTypes
        | EffectType::EntityFilterAddIsIdle
        | EffectType::EntityFilterAddDiplomacy
        | EffectType::UnitListFilter
        | EffectType::SquadListFilter => filters::execute(effect, script, world),
        EffectType::SetTeleporterDestination => set_teleporter(effect, script, world),
        EffectType::PlaySound
        | EffectType::DebugVarTime
        | EffectType::DebugVarCount
        | EffectType::DebugVarFloat
        | EffectType::DebugVarPlayerList
        | EffectType::DebugVarString => EffectOutcome::Presentation,
        _ if is_copy_effect(effect.raw_type) => copy_value(effect, script),
        _ => return None,
    };
    Some(outcome)
}

#[cfg(test)]
mod tests;
