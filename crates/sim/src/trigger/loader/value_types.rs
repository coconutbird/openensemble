//! Parsers for authored trigger value types with compound wire formats.

use super::{TriggerLoadContext, Vec3};
use crate::EntityId;
use std::collections::HashMap;

pub(super) fn parse_entity_reference(text: &str, context: TriggerLoadContext<'_>) -> EntityId {
    parse_entity_reference_with_map(text, context.scenario_entities)
}

pub(super) fn parse_unit_reference(text: &str, context: TriggerLoadContext<'_>) -> EntityId {
    parse_entity_reference_with_map(text, context.scenario_units.or(context.scenario_entities))
}

/// Map an authored scenario player number to its world player.
///
/// Cases 12 (`Player`) and 23 (`PlayerList`) of `BTriggerVar_loadValueFromText` @
/// `0x14048B280` read a number in `0..=8` through the scenario player map
/// (`gScenarioPlayerToWorldPlayer`) and store `-1` for any other number or an unmapped
/// player.
pub(super) fn scenario_player(player: i32, context: TriggerLoadContext<'_>) -> i32 {
    let Some(map) = context.scenario_players else {
        return player;
    };
    usize::try_from(player)
        .ok()
        .and_then(|index| map.get(index).copied().flatten())
        .map_or(-1, i32::from)
}

fn parse_entity_reference_with_map(
    text: &str,
    scenario_entities: Option<&HashMap<i32, EntityId>>,
) -> EntityId {
    let Ok(raw_id) = text.trim().parse::<u32>() else {
        return EntityId::INVALID;
    };
    if let Some(entities) = scenario_entities {
        return i32::try_from(raw_id)
            .ok()
            .and_then(|scenario_id| entities.get(&scenario_id).copied())
            .unwrap_or(EntityId::INVALID);
    }
    EntityId::from_u32(raw_id)
}

pub(super) fn parse_entity_list(text: &str, context: TriggerLoadContext<'_>) -> Vec<EntityId> {
    parse_reference_list(text, |token| parse_entity_reference(token, context))
}

pub(super) fn parse_unit_list(text: &str, context: TriggerLoadContext<'_>) -> Vec<EntityId> {
    parse_reference_list(text, |token| parse_unit_reference(token, context))
}

fn parse_reference_list(
    text: &str,
    mut parse_reference: impl FnMut(&str) -> EntityId,
) -> Vec<EntityId> {
    let mut entities = text
        .split(',')
        .map(&mut parse_reference)
        .filter(|entity_id| !entity_id.is_invalid())
        .collect::<Vec<_>>();
    entities.sort_unstable();
    entities.dedup();
    entities
}

pub(super) fn parse_list_position(text: &str) -> i32 {
    match text.trim() {
        value if value.eq_ignore_ascii_case("Last") => 1,
        value if value.eq_ignore_ascii_case("Random") => 2,
        value => value
            .parse()
            .ok()
            .filter(|value| (0..=2).contains(value))
            .unwrap_or(0),
    }
}

pub(super) fn parse_vector_list(text: &str) -> Vec<Vec3> {
    text.split('|')
        .filter_map(|token| {
            let mut components = token.split(',').map(str::trim);
            Some(Vec3::new(
                components.next()?.parse().ok()?,
                components.next()?.parse().ok()?,
                components.next()?.parse().ok()?,
            ))
        })
        .collect()
}
