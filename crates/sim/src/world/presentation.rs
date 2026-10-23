//! Checksummed trigger-authored state consumed by renderer and UI clients.

use std::collections::BTreeMap;

use glam::Vec3;

use super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

mod callouts;
mod camera_shakes;
mod fades;
mod flares;
mod hints;
mod messages;
mod objective_pointers;
mod rumbles;
mod scene;
mod user_messages;

pub use callouts::{HintCallout, HintCalloutAnchor};
pub use camera_shakes::CameraShake;
pub use fades::{ScreenFadeOverlay, ScreenFadeSequence};
pub use flares::MinimapFlare;
pub use hints::HintMessage;
pub use messages::TriggerMessage;
pub use objective_pointers::ObjectivePointer;
pub use rumbles::{RumbleMotor, RumbleRequest};
pub use scene::{FollowCamera, LightsetAnimation, WorldSoundRequest};
pub(crate) use user_messages::UserMessageUpdate;
pub use user_messages::{USER_MESSAGE_COUNT, UserMessage, UserMessageJustify};

const HUD_ITEM_COUNT: usize = 11;
const GLOBAL_TOGGLE_COUNT: usize = 6;

#[derive(Debug, Clone, Copy)]
enum GlobalPresentationToggle {
    Chats,
    RenderTerrainSkirt,
    ScreenBlur,
    MinimapSkirtMirroring,
    /// `EnableMusicManager` (1006): the music manager plays music.
    MusicManager,
    /// `ChatForceSubtitles` (1049): chats show subtitles also when the options hide them.
    ChatForceSubtitles,
}

const DEFAULT_GLOBAL_TOGGLES: [bool; GLOBAL_TOGGLE_COUNT] = [true, true, false, true, true, false];

/// One retail `BUser::cHUDItem*` slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HudItem {
    /// The minimap.
    Minimap = 0,
    /// The resource display.
    Resources = 1,
    /// The game time display.
    Time = 2,
    /// The power status display.
    PowerStatus = 3,
    /// The unit display.
    Units = 4,
    /// The directional pad help.
    DpadHelp = 5,
    /// The button help.
    ButtonHelp = 6,
    /// The targeting reticle.
    Reticle = 7,
    /// The score display.
    Score = 8,
    /// The unit statistics display.
    UnitStats = 9,
    /// The extra information panel of the circle menu.
    CircleMenuExtraInfo = 10,
}

impl HudItem {
    /// Resolve the exact trigger-database HUD item names.
    #[must_use]
    pub fn from_trigger_name(name: &str) -> Option<Self> {
        HUD_ITEM_NAMES
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name.trim()))
            .and_then(|index| u8::try_from(index).ok())
            .and_then(Self::from_u8)
    }

    /// Return the retail trigger-database name.
    #[must_use]
    pub const fn trigger_name(self) -> &'static str {
        HUD_ITEM_NAMES[self as usize]
    }

    const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Minimap),
            1 => Some(Self::Resources),
            2 => Some(Self::Time),
            3 => Some(Self::PowerStatus),
            4 => Some(Self::Units),
            5 => Some(Self::DpadHelp),
            6 => Some(Self::ButtonHelp),
            7 => Some(Self::Reticle),
            8 => Some(Self::Score),
            9 => Some(Self::UnitStats),
            10 => Some(Self::CircleMenuExtraInfo),
            _ => None,
        }
    }
}

const HUD_ITEM_NAMES: [&str; HUD_ITEM_COUNT] = [
    "Minimap",
    "Resources",
    "Time",
    "PowerStatus",
    "Units",
    "DpadHelp",
    "ButtonHelp",
    "Reticle",
    "Score",
    "UnitStats",
    "CircleMenuExtraInfo",
];

const DEFAULT_HUD_ITEMS: [bool; HUD_ITEM_COUNT] = [
    true, true, false, true, true, false, false, true, false, true, true,
];

/// One retail camera directive (`SetCamera` v2, v3 or v4) waiting for a renderer/UI adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraDirective {
    /// The nonzero identity of this directive, which increases with each new directive.
    pub revision: u32,
    /// Terrain-planted hover point, when authored.
    pub location: Option<Vec3>,
    /// Horizontal direction used by retail's relative-yaw operation.
    pub direction: Option<Vec3>,
    /// Optional hover-height offset introduced by version 4.
    pub hover_height_offset: Option<f32>,
    /// Version 3 and 4 slot 8: the location is absolute. Retail then clears the camera lock
    /// flags of the user before it moves the camera.
    pub absolute_position: bool,
    /// Version 3 and 4 slot 9: the camera takes the direction as its own direction. A
    /// direction with zero x and z keeps the current direction and only changes the pitch.
    pub absolute_direction: bool,
}

/// The inputs of one `SetCamera` directive for one player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CameraDirectiveInput {
    pub(crate) controls: [bool; 3],
    pub(crate) location: Option<Vec3>,
    pub(crate) direction: Option<Vec3>,
    pub(crate) hover_height_offset: Option<f32>,
    pub(crate) absolute_position: bool,
    pub(crate) absolute_direction: bool,
}

/// Trigger-authored permissions for renderer-owned camera input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraControlPermissions {
    /// If `true`, the player can scroll the camera.
    pub scroll: bool,
    /// If `true`, the player can turn the camera.
    pub yaw: bool,
    /// If `true`, the player can zoom the camera.
    pub zoom: bool,
}

impl Default for CameraControlPermissions {
    fn default() -> Self {
        Self {
            scroll: true,
            yaw: true,
            zoom: true,
        }
    }
}

/// Durable presentation controls for one player/user slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerPresentationState {
    /// The camera input that the player can use.
    pub camera_controls: CameraControlPermissions,
    /// The last camera directive from a trigger, if any.
    pub camera_directive: Option<CameraDirective>,
    /// If `true`, the game ignores directional pad input from the player.
    pub ignore_dpad: bool,
    /// If `true`, the player can open the power menu.
    pub power_menu_enabled: bool,
    /// The ID of the trigger script that locks the user input, if any.
    pub user_lock_owner_script: Option<u32>,
    /// Bit `n` is set while `FlashUIElement` (923) flashes `FlashableUIItem` `n` for the
    /// player.
    pub flashing_ui_items: u32,
    /// Increases each time `UITogglePowerOverlay` (726) cancels the power targeting of the
    /// player. The UI closes its power targeting when the value changes.
    pub power_targeting_cancel_revision: u32,
}

impl PlayerPresentationState {
    /// Whether this user's game input is currently owned by a trigger script.
    #[must_use]
    pub const fn user_locked(self) -> bool {
        self.user_lock_owner_script.is_some()
    }
}

impl Default for PlayerPresentationState {
    fn default() -> Self {
        Self {
            camera_controls: CameraControlPermissions::default(),
            camera_directive: None,
            ignore_dpad: false,
            power_menu_enabled: true,
            user_lock_owner_script: None,
            flashing_ui_items: 0,
            power_targeting_cancel_revision: 0,
        }
    }
}

#[derive(Debug, PartialEq)]
pub(super) struct PresentationControlState {
    callouts: callouts::HintCalloutState,
    camera_shakes: camera_shakes::CameraShakeState,
    flares: flares::FlareState,
    hints: hints::HintMessageState,
    messages: messages::TriggerMessageState,
    objective_pointers: objective_pointers::ObjectivePointerState,
    rumbles: rumbles::RumbleState,
    scene: scene::SceneCueState,
    screen_fade: fades::ScreenFadeState,
    user_messages: user_messages::UserMessageState,
    global_toggles: [bool; GLOBAL_TOGGLE_COUNT],
    hud_items: [bool; HUD_ITEM_COUNT],
    minimap_rotation_degrees: f32,
    circle_menu_reset_revision: u32,
    next_camera_revision: u32,
    players: BTreeMap<PlayerId, PlayerPresentationState>,
}

impl Default for PresentationControlState {
    fn default() -> Self {
        Self {
            callouts: callouts::HintCalloutState::default(),
            camera_shakes: camera_shakes::CameraShakeState::default(),
            flares: flares::FlareState::default(),
            hints: hints::HintMessageState::default(),
            messages: messages::TriggerMessageState::default(),
            objective_pointers: objective_pointers::ObjectivePointerState::default(),
            rumbles: rumbles::RumbleState::default(),
            scene: scene::SceneCueState::default(),
            screen_fade: fades::ScreenFadeState::default(),
            user_messages: user_messages::UserMessageState::default(),
            global_toggles: DEFAULT_GLOBAL_TOGGLES,
            hud_items: DEFAULT_HUD_ITEMS,
            minimap_rotation_degrees: 0.0,
            circle_menu_reset_revision: 0,
            next_camera_revision: 0,
            players: BTreeMap::new(),
        }
    }
}

impl World {
    /// Return the authoritative visibility of one HUD component.
    #[must_use]
    pub fn hud_item_enabled(&self, item: HudItem) -> bool {
        self.presentation_control.hud_items[item as usize]
    }

    /// Whether renderer/UI clients should present authored chat messages.
    #[must_use]
    pub const fn chats_enabled(&self) -> bool {
        self.presentation_control
            .toggle(GlobalPresentationToggle::Chats)
    }

    /// Whether the renderer should draw the authored terrain skirt.
    #[must_use]
    pub const fn render_terrain_skirt_enabled(&self) -> bool {
        self.presentation_control
            .toggle(GlobalPresentationToggle::RenderTerrainSkirt)
    }

    /// Whether the primary view requests the retail screen-blur presentation.
    #[must_use]
    pub const fn screen_blur_enabled(&self) -> bool {
        self.presentation_control
            .toggle(GlobalPresentationToggle::ScreenBlur)
    }

    /// Rotation offset in degrees for the renderer-owned minimap.
    #[must_use]
    pub const fn minimap_rotation_degrees(&self) -> f32 {
        self.presentation_control.minimap_rotation_degrees
    }

    /// Whether the renderer-owned minimap mirrors the terrain skirt.
    #[must_use]
    pub const fn minimap_skirt_mirroring(&self) -> bool {
        self.presentation_control
            .toggle(GlobalPresentationToggle::MinimapSkirtMirroring)
    }

    /// Monotonic signal instructing UI adapters to close their circle menu.
    #[must_use]
    pub const fn circle_menu_reset_revision(&self) -> u32 {
        self.presentation_control.circle_menu_reset_revision
    }

    /// Whether the music manager plays music (`EnableMusicManager`, 1006).
    #[must_use]
    pub const fn music_manager_enabled(&self) -> bool {
        self.presentation_control
            .toggle(GlobalPresentationToggle::MusicManager)
    }

    /// Whether chats show subtitles also when the options hide them (`ChatForceSubtitles`,
    /// 1049).
    #[must_use]
    pub const fn chat_subtitles_forced(&self) -> bool {
        self.presentation_control
            .toggle(GlobalPresentationToggle::ChatForceSubtitles)
    }

    /// Snapshot the authoritative renderer/UI controls for one player.
    #[must_use]
    pub fn player_presentation_state(&self, player_id: PlayerId) -> PlayerPresentationState {
        self.presentation_control
            .players
            .get(&player_id)
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn set_hud_item_enabled(&mut self, item: HudItem, enabled: bool) {
        self.presentation_control.hud_items[item as usize] = enabled;
    }

    pub(crate) fn set_chats_enabled(&mut self, enabled: bool) {
        self.presentation_control
            .set_toggle(GlobalPresentationToggle::Chats, enabled);
    }

    pub(crate) fn set_render_terrain_skirt_enabled(&mut self, enabled: bool) {
        self.presentation_control
            .set_toggle(GlobalPresentationToggle::RenderTerrainSkirt, enabled);
    }

    pub(crate) fn set_screen_blur_enabled(&mut self, enabled: bool) {
        self.presentation_control
            .set_toggle(GlobalPresentationToggle::ScreenBlur, enabled);
    }

    pub(crate) fn set_minimap_rotation_degrees(&mut self, degrees: f32) {
        self.presentation_control.minimap_rotation_degrees = degrees;
    }

    pub(crate) fn set_minimap_skirt_mirroring(&mut self, enabled: bool) {
        self.presentation_control
            .set_toggle(GlobalPresentationToggle::MinimapSkirtMirroring, enabled);
    }

    pub(crate) fn set_music_manager_enabled(&mut self, enabled: bool) {
        self.presentation_control
            .set_toggle(GlobalPresentationToggle::MusicManager, enabled);
    }

    pub(crate) fn set_chat_subtitles_forced(&mut self, forced: bool) {
        self.presentation_control
            .set_toggle(GlobalPresentationToggle::ChatForceSubtitles, forced);
    }

    /// `FlashUIElement` (923) v2: start or stop the flash of one `FlashableUIItem` for a
    /// player.
    pub(crate) fn set_player_ui_item_flashing(
        &mut self,
        player_id: PlayerId,
        item: u32,
        flash: bool,
    ) {
        let bit = 1_u32.checked_shl(item).unwrap_or(0);
        self.update_player_presentation(player_id, |state| {
            if flash {
                state.flashing_ui_items |= bit;
            } else {
                state.flashing_ui_items &= !bit;
            }
        });
    }

    /// `UITogglePowerOverlay` (726) off: cancel the power targeting of the user of a player.
    pub(crate) fn cancel_player_power_targeting(&mut self, player_id: PlayerId) {
        self.update_player_presentation(player_id, |state| {
            state.power_targeting_cancel_revision =
                state.power_targeting_cancel_revision.wrapping_add(1).max(1);
        });
    }

    pub(crate) fn reset_circle_menu(&mut self) {
        let revision = self
            .presentation_control
            .circle_menu_reset_revision
            .wrapping_add(1);
        self.presentation_control.circle_menu_reset_revision = revision.max(1);
    }

    pub(crate) fn set_player_ignore_dpad(&mut self, player_id: PlayerId, ignore: bool) {
        self.update_player_presentation(player_id, |state| state.ignore_dpad = ignore);
    }

    pub(crate) fn set_player_power_menu_enabled(&mut self, player_id: PlayerId, enabled: bool) {
        self.update_player_presentation(player_id, |state| state.power_menu_enabled = enabled);
    }

    pub(crate) fn set_player_user_lock(&mut self, player_id: PlayerId, script_id: u32, lock: bool) {
        self.update_player_presentation(player_id, |state| {
            if lock {
                if state.user_lock_owner_script.is_none() {
                    state.user_lock_owner_script = Some(script_id);
                }
            } else if state.user_lock_owner_script == Some(script_id) {
                state.user_lock_owner_script = None;
            }
        });
    }

    pub(crate) fn set_player_camera(&mut self, player_id: PlayerId, input: CameraDirectiveInput) {
        if player_id == 0 || self.get_player(player_id).is_none() {
            return;
        }
        let directive = if input.location.is_some()
            || input.direction.is_some()
            || input.hover_height_offset.is_some()
        {
            Some(CameraDirective {
                revision: self.presentation_control.allocate_camera_revision(),
                location: input.location,
                direction: input.direction,
                hover_height_offset: input.hover_height_offset,
                absolute_position: input.absolute_position,
                absolute_direction: input.absolute_direction,
            })
        } else {
            None
        };
        self.update_player_presentation(player_id, |state| {
            state.camera_controls = CameraControlPermissions {
                scroll: input.controls[0],
                yaw: input.controls[1],
                zoom: input.controls[2],
            };
            if directive.is_some() {
                state.camera_directive = directive;
            }
        });
    }

    fn update_player_presentation(
        &mut self,
        player_id: PlayerId,
        update: impl FnOnce(&mut PlayerPresentationState),
    ) {
        if player_id == 0 || self.get_player(player_id).is_none() {
            return;
        }
        let mut state = self
            .presentation_control
            .players
            .remove(&player_id)
            .unwrap_or_default();
        update(&mut state);
        if state != PlayerPresentationState::default() {
            self.presentation_control.players.insert(player_id, state);
        }
    }
}

impl PresentationControlState {
    const fn toggle(&self, toggle: GlobalPresentationToggle) -> bool {
        self.global_toggles[toggle as usize]
    }

    fn set_toggle(&mut self, toggle: GlobalPresentationToggle, enabled: bool) {
        self.global_toggles[toggle as usize] = enabled;
    }

    fn allocate_camera_revision(&mut self) -> u32 {
        self.next_camera_revision = self.next_camera_revision.wrapping_add(1).max(1);
        self.next_camera_revision
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        self.callouts.hash_state(checksum);
        self.camera_shakes.hash_state(checksum);
        self.flares.hash_state(checksum);
        self.hints.hash_state(checksum);
        self.messages.hash_state(checksum);
        self.objective_pointers.hash_state(checksum);
        self.rumbles.hash_state(checksum);
        self.scene.hash_state(checksum);
        self.user_messages.hash_state(checksum);
        self.hash_screen_fade(checksum);
        for enabled in self.global_toggles {
            checksum.hash_u32(u32::from(enabled));
        }
        for enabled in self.hud_items {
            checksum.hash_u32(u32::from(enabled));
        }
        checksum.hash_f32(self.minimap_rotation_degrees);
        checksum.hash_u32(self.circle_menu_reset_revision);
        checksum.hash_u32(self.next_camera_revision);
        checksum.hash_u32(u32::try_from(self.players.len()).unwrap_or(u32::MAX));
        for (player_id, state) in &self.players {
            checksum.hash_u32(u32::from(*player_id));
            state.hash_state(checksum);
        }
    }
}

impl PlayerPresentationState {
    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.camera_controls.scroll));
        checksum.hash_u32(u32::from(self.camera_controls.yaw));
        checksum.hash_u32(u32::from(self.camera_controls.zoom));
        if let Some(directive) = self.camera_directive {
            checksum.hash_u32(1);
            directive.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::from(self.ignore_dpad));
        checksum.hash_u32(u32::from(self.power_menu_enabled));
        checksum.hash_u32(self.user_lock_owner_script.unwrap_or(u32::MAX));
        checksum.hash_u32(self.flashing_ui_items);
        checksum.hash_u32(self.power_targeting_cancel_revision);
    }
}

impl CameraDirective {
    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.revision);
        hash_optional_vec3(checksum, self.location);
        hash_optional_vec3(checksum, self.direction);
        hash_optional_f32(checksum, self.hover_height_offset);
        checksum.hash_u32(u32::from(self.absolute_position));
        checksum.hash_u32(u32::from(self.absolute_direction));
    }
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_vec3(value.x, value.y, value.z);
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_optional_f32(checksum: &mut SyncChecksum, value: Option<f32>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_f32(value);
    } else {
        checksum.hash_u32(0);
    }
}

#[cfg(test)]
#[path = "presentation/tests.rs"]
mod tests;
