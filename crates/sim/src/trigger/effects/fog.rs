//! Retail trigger effect 385 (`EnableFogOfWar`).

use super::EffectOutcome;
use super::support::bool_at;
use crate::trigger::{Effect, TriggerScript};
use crate::world::World;

pub(super) fn set_enabled(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if effect.version != 1 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(enabled) = bool_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    world.set_fog_of_war_enabled(enabled);
    EffectOutcome::Applied
}

pub(super) fn black_map(effect: &Effect, world: &mut World) -> EffectOutcome {
    if effect.version != 1 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    match effect.effect_type {
        crate::trigger::EffectType::ClearBlackMap => world.clear_black_map(),
        crate::trigger::EffectType::ResetBlackMap => world.reset_black_map(),
        _ => return EffectOutcome::Unsupported(effect.raw_type),
    }
    EffectOutcome::Applied
}

#[cfg(test)]
mod tests;
