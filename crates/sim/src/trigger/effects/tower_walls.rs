//! Retail tower-wall trigger setup.

use super::{EffectOutcome, value_at};
use crate::gameplay::GameplayCatalog;
use crate::trigger::{Effect, TriggerScript};
use crate::{EntityId, World};

pub(super) fn set_destination(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
) -> EffectOutcome {
    let (Some(source), Some(target), Some(gameplay)) = (
        entity_at(effect, script, 1),
        entity_at(effect, script, 2),
        gameplay,
    ) else {
        return EffectOutcome::Skipped;
    };
    let Some(source_proto) = source_leader_proto(world, source) else {
        return EffectOutcome::Skipped;
    };
    if !has_tower_wall_action(gameplay, &source_proto) {
        return EffectOutcome::Skipped;
    }
    if world.set_tower_wall_destination(source, target) {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

fn entity_at(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<EntityId> {
    value_at(effect, script, slot)?.as_entity()
}

fn source_leader_proto(world: &World, source: EntityId) -> Option<String> {
    let squad = world.get_squad(source)?;
    let unit = world.get_unit(*squad.unit_ids.first()?)?;
    Some(unit.proto_object_name.clone())
}

fn has_tower_wall_action(gameplay: &GameplayCatalog, proto_object_name: &str) -> bool {
    gameplay.object(proto_object_name).is_some_and(|object| {
        object.tactics().actions.iter().any(|action| {
            action
                .action_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("TowerWall"))
        })
    })
}

#[cfg(test)]
#[path = "tower_walls/tests.rs"]
mod tests;
