//! Retail owner queries and squad ownership transfer.

use super::support::{
    EntityListKind, combine_entities, entities_at, player_at, used_variable_id, variable_is_used,
};
use super::{EffectOutcome, write_value};
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::{EntityId, World};

pub(super) fn change_owner(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if effect.version != 3 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    if !variable_is_used(effect, script, 6) && !variable_is_used(effect, script, 7) {
        return EffectOutcome::Skipped;
    }
    let Some(new_owner) = player_at(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    if world.get_player(new_owner).is_none() {
        return EffectOutcome::Skipped;
    }
    let squads = combine_entities(
        entities_at(effect, script, 7, EntityListKind::Squad),
        entities_at(effect, script, 6, EntityListKind::Squad),
    );
    for squad_id in squads {
        let _changed = world.change_squad_owner(squad_id, new_owner);
    }
    EffectOutcome::Applied
}

pub(super) fn get_owner(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &World,
) -> EffectOutcome {
    let unit_used = variable_is_used(effect, script, 1);
    let squad_used = variable_is_used(effect, script, 2);
    if !unit_used && !squad_used {
        return EffectOutcome::Skipped;
    }
    let owner = if unit_used {
        scalar_entity(effect, script, 1, EntityListKind::Unit)
            .and_then(|unit_id| world.entity_owner(unit_id))
    } else {
        None
    }
    .or_else(|| {
        squad_used
            .then(|| scalar_entity(effect, script, 2, EntityListKind::Squad))
            .flatten()
            .and_then(|squad_id| world.entity_owner(squad_id))
    });
    if let (Some(owner), Some(variable_id)) = (owner, used_variable_id(effect, script, 3)) {
        let _outcome = write_value(script, variable_id, TriggerValue::Player(i32::from(owner)));
    }
    EffectOutcome::Applied
}

fn scalar_entity(
    effect: &Effect,
    script: &TriggerScript,
    signature_id: u16,
    kind: EntityListKind,
) -> Option<EntityId> {
    entities_at(effect, script, signature_id, kind)?
        .into_iter()
        .next()
}
