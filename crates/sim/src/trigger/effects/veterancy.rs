//! Retail squad-veterancy trigger effects.

use super::EffectOutcome;
use super::support::{EntityListKind, entities_at, float_at, variable_is_used};
use crate::gameplay::GameplayCatalog;
use crate::trigger::{Effect, TriggerScript};
use crate::world::World;

#[cfg(test)]
mod tests;

pub(super) fn add_experience(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
) -> EffectOutcome {
    let Some(gameplay) = gameplay else {
        return EffectOutcome::Skipped;
    };
    let Some(experience) = float_at(effect, script, 3).filter(|value| value.is_finite()) else {
        return EffectOutcome::Skipped;
    };
    if variable_is_used(effect, script, 1) {
        if let Some(squad_id) = entities_at(effect, script, 1, EntityListKind::Squad)
            .and_then(|squads| squads.into_iter().next())
        {
            let _added = world.add_squad_experience(squad_id, experience, gameplay);
        }
        return EffectOutcome::Applied;
    }
    if variable_is_used(effect, script, 2) {
        for squad_id in entities_at(effect, script, 2, EntityListKind::Squad).unwrap_or_default() {
            let _added = world.add_squad_experience(squad_id, experience, gameplay);
        }
        return EffectOutcome::Applied;
    }
    EffectOutcome::Skipped
}
