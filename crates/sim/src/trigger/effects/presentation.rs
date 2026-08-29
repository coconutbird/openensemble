//! Retail trigger effects that author renderer and per-user UI state.

use super::{EffectOutcome, value_at};
use crate::player::PlayerId;
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::world::{HudItem, ScreenFadeSequence, World};
use glam::Vec3;

mod callouts;

pub(super) fn execute(
    effect: &Effect,
    script: &mut TriggerScript,
    world: &mut World,
) -> Option<EffectOutcome> {
    let outcome = match effect.effect_type {
        EffectType::HintCalloutCreate => callouts::create(effect, script, world),
        EffectType::HintCalloutDestroy => callouts::destroy(effect, script, world),
        EffectType::HudToggle => hud_toggle(effect, script, world),
        EffectType::SetRenderTerrainSkirt => set_terrain_skirt(effect, script, world),
        EffectType::SetCamera => set_camera(effect, script, world),
        EffectType::FadeToColor => fade_to_color(effect, script, world),
        EffectType::FadeTransition => fade_transition(effect, script, world),
        EffectType::IgnoreDpad => set_ignore_dpad(effect, script, world),
        EffectType::EnableScreenBlur => set_screen_blur(effect, script, world),
        EffectType::PowerMenuEnable => set_power_menu(effect, script, world),
        EffectType::SetMinimapNorthPointerRotation => set_minimap_rotation(effect, script, world),
        EffectType::HideCircleMenu => {
            world.reset_circle_menu();
            EffectOutcome::Presentation
        }
        EffectType::SetMinimapSkirtMirroring => set_minimap_skirt_mirroring(effect, script, world),
        EffectType::LockPlayerUser => set_user_lock(effect, script, world),
        _ => return None,
    };
    Some(outcome)
}

fn fade_to_color(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let (Some(color), Some(duration_ms)) = (
        color_value(effect, script, 1),
        time_value(effect, script, 2),
    ) else {
        return EffectOutcome::Skipped;
    };
    let Ok(fade_in) = optional_bool(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    world.start_screen_fade(
        color,
        ScreenFadeSequence::ToColor {
            duration_ms,
            fade_in: fade_in.unwrap_or(false),
        },
    );
    EffectOutcome::Presentation
}

fn fade_transition(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let (Some(fade_down_ms), Some(hold_ms), Some(fade_up_ms), Some(color)) = (
        time_value(effect, script, 1),
        time_value(effect, script, 2),
        time_value(effect, script, 3),
        color_value(effect, script, 5),
    ) else {
        return EffectOutcome::Skipped;
    };
    let Ok(reverse) = optional_bool(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    world.start_screen_fade(
        color,
        ScreenFadeSequence::Transition {
            fade_down_ms,
            hold_ms,
            fade_up_ms,
            reverse: reverse.unwrap_or(false),
        },
    );
    EffectOutcome::Presentation
}

fn hud_toggle(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(TriggerValue::String(name)) = value_at(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let (Some(item), Some(enabled)) = (
        HudItem::from_trigger_name(name),
        bool_value(effect, script, 2),
    ) else {
        return EffectOutcome::Skipped;
    };
    world.set_hud_item_enabled(item, enabled);
    EffectOutcome::Presentation
}

fn set_terrain_skirt(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(enabled) = bool_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    world.set_render_terrain_skirt_enabled(enabled);
    EffectOutcome::Presentation
}

fn set_camera(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    if effect.version != 4 {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let (Some(scroll), Some(yaw), Some(zoom)) = (
        bool_value(effect, script, 1),
        bool_value(effect, script, 2),
        bool_value(effect, script, 3),
    ) else {
        return EffectOutcome::Skipped;
    };
    let Ok(mut location) = optional_vector(effect, script, 4) else {
        return EffectOutcome::Skipped;
    };
    let Ok(mut direction) = optional_vector(effect, script, 5) else {
        return EffectOutcome::Skipped;
    };
    let Ok(players) = player_targets(effect, script, 6, 7) else {
        return EffectOutcome::Skipped;
    };
    let Ok(absolute_position) = optional_bool(effect, script, 8) else {
        return EffectOutcome::Skipped;
    };
    let Ok(absolute_direction) = optional_bool(effect, script, 9) else {
        return EffectOutcome::Skipped;
    };
    if absolute_position == Some(true) || absolute_direction == Some(true) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Ok(height_offset) = optional_float(effect, script, 10) else {
        return EffectOutcome::Skipped;
    };

    if let Some(value) = &mut location
        && let Some(height) = world.terrain_height(*value, true)
    {
        value.y = height;
    }
    if let Some(value) = &mut direction {
        value.y = 0.0;
    }
    for player_id in players {
        world.set_player_camera_v4(
            player_id,
            [scroll, yaw, zoom],
            location,
            direction,
            height_offset,
        );
    }
    EffectOutcome::Presentation
}

fn set_ignore_dpad(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Ok(players) = player_targets(effect, script, 1, 2) else {
        return EffectOutcome::Skipped;
    };
    let Some(ignore) = bool_value(effect, script, 3) else {
        return EffectOutcome::Skipped;
    };
    for player_id in players {
        world.set_player_ignore_dpad(player_id, ignore);
    }
    EffectOutcome::Presentation
}

fn set_screen_blur(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(enabled) = bool_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    world.set_screen_blur_enabled(enabled);
    EffectOutcome::Presentation
}

fn set_power_menu(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(enabled) = bool_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Ok(players) = player_targets(effect, script, 2, 3) else {
        return EffectOutcome::Skipped;
    };
    for player_id in players {
        world.set_player_power_menu_enabled(player_id, enabled);
    }
    EffectOutcome::Presentation
}

fn set_minimap_rotation(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(degrees) = float_value(effect, script, 1).filter(|value| value.is_finite()) else {
        return EffectOutcome::Skipped;
    };
    world.set_minimap_rotation_degrees(degrees);
    EffectOutcome::Presentation
}

fn set_minimap_skirt_mirroring(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
) -> EffectOutcome {
    let Some(enabled) = bool_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    world.set_minimap_skirt_mirroring(enabled);
    EffectOutcome::Presentation
}

fn set_user_lock(effect: &Effect, script: &TriggerScript, world: &mut World) -> EffectOutcome {
    let Some(player_id) = player_value(effect, script, 1) else {
        return EffectOutcome::Skipped;
    };
    let Some(lock) = bool_value(effect, script, 2) else {
        return EffectOutcome::Skipped;
    };
    world.set_player_user_lock(player_id, script.id, lock);
    EffectOutcome::Presentation
}

fn player_targets(
    effect: &Effect,
    script: &TriggerScript,
    scalar_slot: u16,
    list_slot: u16,
) -> Result<Vec<PlayerId>, ()> {
    let mut players = Vec::new();
    if let Some(value) = optional_value(effect, script, list_slot) {
        let TriggerValue::PlayerList(values) = value else {
            return Err(());
        };
        players.extend(values.iter().filter_map(|value| u8::try_from(*value).ok()));
    }
    if let Some(value) = optional_value(effect, script, scalar_slot) {
        let TriggerValue::Player(value) = value else {
            return Err(());
        };
        if let Ok(player_id) = u8::try_from(*value) {
            unique_add(&mut players, player_id);
        }
    }
    Ok(players)
}

fn optional_vector(effect: &Effect, script: &TriggerScript, slot: u16) -> Result<Option<Vec3>, ()> {
    let Some(value) = optional_value(effect, script, slot) else {
        return Ok(None);
    };
    let Some(value) = value.as_location() else {
        return Err(());
    };
    let vector = Vec3::new(value.x, value.y, value.z);
    vector.is_finite().then_some(Some(vector)).ok_or(())
}

fn optional_bool(effect: &Effect, script: &TriggerScript, slot: u16) -> Result<Option<bool>, ()> {
    optional_value(effect, script, slot)
        .map_or(Ok(None), |value| value.as_bool().ok_or(()).map(Some))
}

fn optional_float(effect: &Effect, script: &TriggerScript, slot: u16) -> Result<Option<f32>, ()> {
    let Some(value) = optional_value(effect, script, slot) else {
        return Ok(None);
    };
    value
        .as_float()
        .filter(|value| value.is_finite())
        .map(Some)
        .ok_or(())
}

fn optional_value<'a>(
    effect: &Effect,
    script: &'a TriggerScript,
    slot: u16,
) -> Option<&'a TriggerValue> {
    value_at(effect, script, slot)
}

fn bool_value(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<bool> {
    value_at(effect, script, slot).and_then(TriggerValue::as_bool)
}

fn float_value(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<f32> {
    value_at(effect, script, slot).and_then(TriggerValue::as_float)
}

fn time_value(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<u32> {
    match value_at(effect, script, slot)? {
        TriggerValue::Time(value) => Some(*value),
        TriggerValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn color_value(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<[u8; 3]> {
    let TriggerValue::Color(color) = value_at(effect, script, slot)? else {
        return None;
    };
    Some([color.r, color.g, color.b])
}

fn player_value(effect: &Effect, script: &TriggerScript, slot: u16) -> Option<PlayerId> {
    let TriggerValue::Player(value) = value_at(effect, script, slot)? else {
        return None;
    };
    u8::try_from(*value).ok()
}

fn unique_add<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
#[path = "presentation/tests.rs"]
mod tests;
