//! Retail entity-filter construction and list-filtering effects.

use super::support::{EntityListKind, bool_at, entity_list, used_variable_id, variable_is_used};
use super::{EffectOutcome, value_at, write_value};
use crate::trigger::value::EntityFilterPredicate;
use crate::trigger::{Effect, EffectType, EntityFilterSet, TriggerScript, TriggerValue, VarId};
use crate::{EntityId, World};

pub(super) fn execute(effect: &Effect, script: &mut TriggerScript, world: &World) -> EffectOutcome {
    match effect.effect_type {
        EffectType::EntityFilterClear => clear(effect, script),
        EffectType::EntityFilterAddIsAlive => add_is_alive(effect, script),
        EffectType::EntityFilterAddInList => add_in_list(effect, script),
        EffectType::EntityFilterAddPlayers => add_players(effect, script),
        EffectType::EntityFilterAddTeams => add_teams(effect, script),
        EffectType::EntityFilterAddProtoObjects => add_proto_objects(effect, script),
        EffectType::EntityFilterAddProtoSquads => add_proto_squads(effect, script),
        EffectType::EntityFilterAddObjectTypes => add_object_types(effect, script),
        EffectType::EntityFilterAddIsIdle => add_is_idle(effect, script),
        EffectType::EntityFilterAddDiplomacy => add_diplomacy(effect, script, world),
        EffectType::UnitListFilter => filter_list(effect, script, world, EntityListKind::Unit),
        EffectType::SquadListFilter => filter_list(effect, script, world, EntityListKind::Squad),
        _ => EffectOutcome::Unsupported(effect.raw_type),
    }
}

pub(super) fn clear(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some(filter_set_id) = filter_set_id(effect, script) else {
        return EffectOutcome::Skipped;
    };
    let Some(filter_set) = filter_set_mut(script, filter_set_id) else {
        return EffectOutcome::Skipped;
    };
    filter_set.clear();
    EffectOutcome::Applied
}

pub(super) fn add_is_alive(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::IsAlive { invert },
    )
}

pub(super) fn add_is_idle(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::IsIdle { invert },
    )
}

pub(super) fn add_diplomacy(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 5) else {
        return EffectOutcome::Skipped;
    };
    let Some(relation_type) = relation_type_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(reference_team) = reference_team_at(effect, script, world) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::Diplomacy {
            invert,
            relation_type,
            reference_team,
        },
    )
}

pub(super) fn add_in_list(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Ok(units) = optional_list(effect, script, 2, EntityListKind::Unit) else {
        return EffectOutcome::Skipped;
    };
    let Ok(squads) = optional_list(effect, script, 3, EntityListKind::Squad) else {
        return EffectOutcome::Skipped;
    };
    if let Some(entities) = units {
        let _outcome = append(
            script,
            filter_set_id,
            EntityFilterPredicate::InList { invert, entities },
        );
    }
    if let Some(entities) = squads {
        let _outcome = append(
            script,
            filter_set_id,
            EntityFilterPredicate::InList { invert, entities },
        );
    }
    EffectOutcome::Applied
}

pub(super) fn add_players(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(players) = player_list_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::Players { invert, players },
    )
}

pub(super) fn add_teams(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(teams) = team_list_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::Teams { invert, teams },
    )
}

pub(super) fn add_proto_objects(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(prototypes) = proto_object_list_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::ProtoObjects { invert, prototypes },
    )
}

pub(super) fn add_proto_squads(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(prototypes) = proto_squad_list_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::ProtoSquads { invert, prototypes },
    )
}

pub(super) fn add_object_types(effect: &Effect, script: &mut TriggerScript) -> EffectOutcome {
    let Some((filter_set_id, invert)) = append_header(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    let Some(object_types) = object_type_list_at(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    append(
        script,
        filter_set_id,
        EntityFilterPredicate::ObjectTypes {
            invert,
            object_types,
        },
    )
}

pub(super) fn filter_list(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
    kind: EntityListKind,
) -> EffectOutcome {
    let Ok(passed_id) = optional_output(effect, script, 3, kind) else {
        return EffectOutcome::Skipped;
    };
    let Ok(failed_id) = optional_output(effect, script, 4, kind) else {
        return EffectOutcome::Skipped;
    };
    if passed_id.is_none() && failed_id.is_none() {
        return EffectOutcome::Applied;
    }
    let Some(source) = list_at(effect, script, 1, kind).cloned() else {
        return EffectOutcome::Skipped;
    };
    let Some(filter_set) = filter_set_at(effect, script, 2).cloned() else {
        return EffectOutcome::Skipped;
    };

    let mut passed = Vec::new();
    let mut failed = Vec::new();
    for entity_id in source {
        match validate_and_test(entity_id, kind, world, &filter_set) {
            Some(true) => passed.push(entity_id),
            Some(false) => failed.push(entity_id),
            None => {}
        }
    }
    if let Some(destination_id) = passed_id {
        let _outcome = write_value(script, destination_id, list_value(kind, passed));
    }
    if let Some(destination_id) = failed_id {
        let _outcome = write_value(script, destination_id, list_value(kind, failed));
    }
    EffectOutcome::Applied
}

fn validate_and_test(
    entity_id: EntityId,
    kind: EntityListKind,
    world: &World,
    filter_set: &EntityFilterSet,
) -> Option<bool> {
    match kind {
        EntityListKind::Unit => world
            .get_unit(entity_id)
            .map(|_| filter_set.matches_entity(entity_id, world)),
        EntityListKind::Squad => world
            .get_squad(entity_id)
            .map(|_| filter_set.matches_entity(entity_id, world)),
        EntityListKind::Object => None,
    }
}

fn append_header(
    effect: &Effect,
    script: &TriggerScript,
    invert_slot: u16,
) -> Option<(VarId, bool)> {
    let filter_set_id = filter_set_id(effect, script)?;
    let invert = if variable_is_used(effect, script, invert_slot) {
        bool_at(effect, script, invert_slot)?
    } else {
        false
    };
    Some((filter_set_id, invert))
}

fn append(
    script: &mut TriggerScript,
    filter_set_id: VarId,
    predicate: EntityFilterPredicate,
) -> EffectOutcome {
    let Some(filter_set) = filter_set_mut(script, filter_set_id) else {
        return EffectOutcome::Skipped;
    };
    filter_set.push(predicate);
    EffectOutcome::Applied
}

fn filter_set_id(effect: &Effect, script: &TriggerScript) -> Option<VarId> {
    let variable_id = used_variable_id(effect, script, 1)?;
    script
        .get_variable(variable_id)
        .and_then(|variable| match &variable.value {
            TriggerValue::EntityFilterSet(_) => Some(variable_id),
            _ => None,
        })
}

fn filter_set_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
) -> Option<&'a EntityFilterSet> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::EntityFilterSet(value) => Some(value),
        _ => None,
    }
}

fn filter_set_mut(script: &mut TriggerScript, variable_id: VarId) -> Option<&mut EntityFilterSet> {
    match &mut script.get_variable_mut(variable_id)?.value {
        TriggerValue::EntityFilterSet(value) => Some(value),
        _ => None,
    }
}

fn optional_list(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Result<Option<Vec<EntityId>>, ()> {
    if !variable_is_used(effect, script, signature_id) {
        return Ok(None);
    }
    list_at(effect, script, signature_id, kind)
        .cloned()
        .map(Some)
        .ok_or(())
}

fn optional_output(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Result<Option<VarId>, ()> {
    if !variable_is_used(effect, script, signature_id) {
        return Ok(None);
    }
    let variable_id = used_variable_id(effect, script, signature_id).ok_or(())?;
    script
        .get_variable(variable_id)
        .and_then(|variable| entity_list(kind, &variable.value))
        .map(|_| Some(variable_id))
        .ok_or(())
}

fn list_at<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<&'a Vec<EntityId>> {
    entity_list(kind, value_at(effect, script, signature_id)?)
}

fn list_value(kind: EntityListKind, entities: Vec<EntityId>) -> TriggerValue {
    match kind {
        EntityListKind::Unit => TriggerValue::UnitList(entities),
        EntityListKind::Squad => TriggerValue::SquadList(entities),
        EntityListKind::Object => TriggerValue::ObjectList(entities),
    }
}

fn player_list_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<Vec<i32>> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::PlayerList(values) => Some(values.clone()),
        _ => None,
    }
}

fn team_list_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<Vec<i32>> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::TeamList(values) => Some(values.clone()),
        _ => None,
    }
}

fn proto_object_list_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<Vec<i32>> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::ProtoObjectList(values) => Some(values.clone()),
        _ => None,
    }
}

fn proto_squad_list_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<Vec<i32>> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::ProtoSquadList(values) => Some(values.clone()),
        _ => None,
    }
}

fn object_type_list_at(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
) -> Option<Vec<String>> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::ObjectTypeList(values) => Some(values.clone()),
        _ => None,
    }
}

fn relation_type_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<i32> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Int(value) => Some(*value),
        _ => None,
    }
}

fn reference_team_at(effect: &Effect, script: &TriggerScript, world: &World) -> Option<i32> {
    if variable_is_used(effect, script, 3) {
        let player_id = match value_at(effect, script, 3)? {
            TriggerValue::Player(value) => u8::try_from(*value).ok()?,
            _ => return None,
        };
        return world
            .get_player(player_id)
            .map(|player| i32::from(player.team_id));
    }
    if variable_is_used(effect, script, 4) {
        return match value_at(effect, script, 4)? {
            TriggerValue::Team(value) if *value != -1 => Some(*value),
            _ => None,
        };
    }
    None
}
