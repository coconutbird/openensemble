//! Retail hint, flare, UI flash, user message and global UI toggle effects.

use super::super::support::variable_is_used;
use super::super::{EffectOutcome, value_at};
use super::{
    bool_value, color_value, float_value, int_value, optional_value, player_value, time_value,
    unique_add,
};
use crate::player::PlayerId;
use crate::trigger::{Effect, TriggerScript, TriggerValue};
use crate::world::{UserMessageUpdate, World};
use num_traits::ToPrimitive;

/// The `FlashableUIItem` values that `BTriggerEffect_teFlashUIElement` handles. Retail does
/// nothing for the D-pad items 12 to 16.
const FLASHABLE_ITEMS_WITH_HANDLER: [i32; 16] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 17, 18, 19, 20];

/// `BTriggerEffect_teHintMessageShow` (`0x140472EA0`).
///
/// Slot 1 is the player list and slot 2 the string ID; retail reads both without a null
/// check. A used slot 3 is the time; retail divides the milliseconds by 1000 as integers.
pub(super) fn hint_message_show(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let (Some(TriggerValue::PlayerList(recipients)), Some(string_id)) =
        (value_at(effect, script, 1), int_value(effect, script, 2))
    else {
        return EffectOutcome::Skipped;
    };
    let duration_s = if variable_is_used(effect, script, 3) {
        let Some(duration_ms) = time_value(effect, script, 3) else {
            return EffectOutcome::Skipped;
        };
        whole_seconds(duration_ms)
    } else {
        0.0
    };
    let recipients = recipients.clone();
    world.show_hint_message(string_id, &recipients, duration_s);
    EffectOutcome::Presentation
}

/// `BTriggerEffect_teHintMessageDestroy` (`0x140473960`).
///
/// v1 removes the current hint. v2 removes it if slot 1 is not used or names its string ID.
pub(super) fn hint_message_destroy(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let string_id = match effect.version {
        2 if variable_is_used(effect, script, 1) => {
            let Some(string_id) = int_value(effect, script, 1) else {
                return EffectOutcome::Skipped;
            };
            Some(string_id)
        }
        1 | 2 => None,
        _ => return EffectOutcome::Unsupported(effect.raw_type),
    };
    world.destroy_hint_message(string_id);
    EffectOutcome::Presentation
}

/// `BTriggerEffect_teFlareMinimapNormal` (`0x140459290`): slot 1 is the source player and
/// slot 2 the location.
pub(super) fn flare_minimap_normal(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let (Some(TriggerValue::Player(from_player)), Some(location)) = (
        value_at(effect, script, 1),
        value_at(effect, script, 2).and_then(TriggerValue::as_location),
    ) else {
        return EffectOutcome::Skipped;
    };
    world.flare_minimap_allies(
        glam::Vec3::new(location.x, location.y, location.z),
        *from_player,
    );
    EffectOutcome::Presentation
}

/// `BTriggerEffect_teFlareMinimapSpoof` (`0x1404589B0`).
///
/// Slot 1 is the location and slot 2 the player that the flare names. The targets are the
/// player in slot 3, then the list in slot 4, each once.
pub(super) fn flare_minimap_spoof(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let (Some(location), Some(TriggerValue::Player(from_player))) = (
        value_at(effect, script, 1).and_then(TriggerValue::as_location),
        value_at(effect, script, 2),
    ) else {
        return EffectOutcome::Skipped;
    };
    let mut targets: Vec<i32> = Vec::new();
    if let Some(value) = optional_value(effect, script, 3) {
        let TriggerValue::Player(player) = value else {
            return EffectOutcome::Skipped;
        };
        targets.push(*player);
    }
    if let Some(value) = optional_value(effect, script, 4) {
        let TriggerValue::PlayerList(players) = value else {
            return EffectOutcome::Skipped;
        };
        for player in players {
            unique_add(&mut targets, *player);
        }
    }
    let location = glam::Vec3::new(location.x, location.y, location.z);
    for player in targets {
        if let Ok(player_id) = PlayerId::try_from(player) {
            world.add_minimap_flare(player_id, location, *from_player);
        }
    }
    EffectOutcome::Presentation
}

/// `BTriggerEffect_teFlashUIElement` (`0x14047AFF0`): only v2 runs. Slot 1 is the item,
/// slot 2 starts or stops the flash and slot 3 is the player.
pub(super) fn flash_ui_element(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if effect.version != 2 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let (Some(item), Some(flash), Some(player_id)) = (
        int_value(effect, script, 1),
        bool_value(effect, script, 2),
        player_value(effect, script, 3),
    ) else {
        return EffectOutcome::Skipped;
    };
    if FLASHABLE_ITEMS_WITH_HANDLER.contains(&item) {
        world.set_player_ui_item_flashing(player_id, item.cast_unsigned(), flash);
    }
    EffectOutcome::Presentation
}

/// `UserMessage` (164) v3 (`BTriggerEffect_teUserMessageV3` `0x140458EC0`).
///
/// Slot 1 is the player list and slot 11 a player. Slot 2 is the text ("NULL" if not used),
/// slot 3 the index, slots 4 and 5 the position, slot 6 the justify, slot 7 the point size,
/// slot 8 the alpha and slot 9 the color. A slot that is not used gives -1, which keeps the
/// old value. Slot 10 enables the message.
pub(super) fn user_message(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    if effect.version != 3 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let (Some(index), Some(enabled)) =
        (int_value(effect, script, 3), bool_value(effect, script, 10))
    else {
        return EffectOutcome::Skipped;
    };
    let mut players: Vec<i32> = Vec::new();
    if let Some(value) = optional_value(effect, script, 1) {
        let TriggerValue::PlayerList(values) = value else {
            return EffectOutcome::Skipped;
        };
        for player in values {
            unique_add(&mut players, *player);
        }
    }
    if let Some(value) = optional_value(effect, script, 11) {
        let TriggerValue::Player(player) = value else {
            return EffectOutcome::Skipped;
        };
        unique_add(&mut players, *player);
    }
    let text = match optional_value(effect, script, 2) {
        None => "NULL".to_owned(),
        Some(TriggerValue::String(text)) => text.clone(),
        Some(_) => return EffectOutcome::Skipped,
    };
    let optional_float = |slot| {
        if variable_is_used(effect, script, slot) {
            float_value(effect, script, slot).ok_or(())
        } else {
            Ok(-1.0)
        }
    };
    let (Ok(x), Ok(y), Ok(point), Ok(alpha)) = (
        optional_float(4),
        optional_float(5),
        optional_float(7),
        optional_float(8),
    ) else {
        return EffectOutcome::Skipped;
    };
    let justify = if variable_is_used(effect, script, 6) {
        let Some(justify) = int_value(effect, script, 6) else {
            return EffectOutcome::Skipped;
        };
        justify
    } else {
        -1
    };
    let color = if variable_is_used(effect, script, 9) {
        let Some(color) = color_value(effect, script, 9) else {
            return EffectOutcome::Skipped;
        };
        Some(color)
    } else {
        None
    };
    let update = UserMessageUpdate {
        players,
        text,
        position: [x, y],
        justify,
        point,
        alpha,
        color,
        enabled,
    };
    if world.set_user_message(index, update) {
        EffectOutcome::Presentation
    } else {
        EffectOutcome::Skipped
    }
}

/// `UITogglePowerOverlay` (726, inline case `0x14044D1EF`). Only the off case runs: slot 2
/// false cancels the power targeting of the user of the player in slot 3.
pub(super) fn ui_toggle_power_overlay(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(on) = bool_value(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    if on {
        return EffectOutcome::Presentation;
    }
    let Some(player_id) = player_value(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    world.cancel_player_power_targeting(player_id);
    EffectOutcome::Presentation
}

/// `EnableMusicManager` (1006, `0x14047C470`) and `ChatForceSubtitles` (1049,
/// `0x1404803F0`): slot 1 sets one flag.
pub(super) fn global_flag(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    set: fn(&mut World, bool),
) -> EffectOutcome {
    let Some(enabled) = bool_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    set(world, enabled);
    EffectOutcome::Presentation
}

/// Integer seconds from milliseconds, as retail `ms / 1000` before the float conversion.
fn whole_seconds(duration_ms: u32) -> f32 {
    (duration_ms / 1000).to_f32().unwrap_or(f32::MAX)
}

#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;
