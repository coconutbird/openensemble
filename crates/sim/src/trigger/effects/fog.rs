//! Retail trigger effects 385 (`EnableFogOfWar`), 713 (`ResetBlackMap`) and 863
//! (`ClearBlackMap`).
//!
//! The cases of `BTriggerEffect_fire` (`0x14044A0A0`) do not test the effect version: 385
//! calls `BWorld_setFogOfWar(enable, !enable, false)` (`0x1405D2A40`), 713 calls
//! `BTriggerEffect_teResetBlackMap` (`0x1404630E0`) and 863 calls `BWorld_clearBlackMap`
//! (`0x1405D2BE0`).

use super::EffectOutcome;
use super::support::bool_at;
use crate::trigger::{Effect, TriggerScript};
use crate::world::World;

pub(super) fn set_enabled(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(enabled) = bool_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    world.set_fog_of_war_enabled(enabled);
    EffectOutcome::Applied
}

pub(super) fn black_map(effect: &Effect, world: &mut World) -> EffectOutcome {
    match effect.effect_type {
        crate::trigger::EffectType::ClearBlackMap => world.clear_black_map(),
        crate::trigger::EffectType::ResetBlackMap => world.reset_black_map(),
        _ => return EffectOutcome::Unsupported(effect.raw_type),
    }
    EffectOutcome::Applied
}

#[cfg(test)]
mod tests;
