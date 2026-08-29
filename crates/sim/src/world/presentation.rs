//! Checksummed trigger-authored state consumed by renderer and UI clients.

use std::collections::BTreeMap;

use glam::Vec3;

use super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

mod callouts;
mod fades;

pub use callouts::{HintCallout, HintCalloutAnchor};
pub use fades::{ScreenFadeOverlay, ScreenFadeSequence};

const HUD_ITEM_COUNT: usize = 11;

/// One retail `BUser::cHUDItem*` slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HudItem {
    Minimap = 0,
    Resources = 1,
    Time = 2,
    PowerStatus = 3,
    Units = 4,
    DpadHelp = 5,
    ButtonHelp = 6,
    Reticle = 7,
    Score = 8,
    UnitStats = 9,
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

/// One version-4 retail camera directive waiting for a renderer/UI adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraDirective {
    pub revision: u32,
    /// Terrain-planted hover point, when authored.
    pub location: Option<Vec3>,
    /// Horizontal direction used by retail's relative-yaw operation.
    pub direction: Option<Vec3>,
    /// Optional hover-height offset introduced by version 4.
    pub hover_height_offset: Option<f32>,
}

/// Trigger-authored permissions for renderer-owned camera input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraControlPermissions {
    pub scroll: bool,
    pub yaw: bool,
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
    pub camera_controls: CameraControlPermissions,
    pub camera_directive: Option<CameraDirective>,
    pub ignore_dpad: bool,
    pub power_menu_enabled: bool,
    pub user_lock_owner_script: Option<u32>,
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
        }
    }
}

#[derive(Debug, PartialEq)]
pub(super) struct PresentationControlState {
    callouts: callouts::HintCalloutState,
    screen_fade: fades::ScreenFadeState,
    hud_items: [bool; HUD_ITEM_COUNT],
    render_terrain_skirt: bool,
    screen_blur_enabled: bool,
    minimap_rotation_degrees: f32,
    minimap_skirt_mirroring: bool,
    circle_menu_reset_revision: u32,
    next_camera_revision: u32,
    players: BTreeMap<PlayerId, PlayerPresentationState>,
}

impl Default for PresentationControlState {
    fn default() -> Self {
        Self {
            callouts: callouts::HintCalloutState::default(),
            screen_fade: fades::ScreenFadeState::default(),
            hud_items: DEFAULT_HUD_ITEMS,
            render_terrain_skirt: true,
            screen_blur_enabled: false,
            minimap_rotation_degrees: 0.0,
            minimap_skirt_mirroring: true,
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

    /// Whether the renderer should draw the authored terrain skirt.
    #[must_use]
    pub const fn render_terrain_skirt_enabled(&self) -> bool {
        self.presentation_control.render_terrain_skirt
    }

    /// Whether the primary view requests the retail screen-blur presentation.
    #[must_use]
    pub const fn screen_blur_enabled(&self) -> bool {
        self.presentation_control.screen_blur_enabled
    }

    /// Rotation offset in degrees for the renderer-owned minimap.
    #[must_use]
    pub const fn minimap_rotation_degrees(&self) -> f32 {
        self.presentation_control.minimap_rotation_degrees
    }

    /// Whether the renderer-owned minimap mirrors the terrain skirt.
    #[must_use]
    pub const fn minimap_skirt_mirroring(&self) -> bool {
        self.presentation_control.minimap_skirt_mirroring
    }

    /// Monotonic signal instructing UI adapters to close their circle menu.
    #[must_use]
    pub const fn circle_menu_reset_revision(&self) -> u32 {
        self.presentation_control.circle_menu_reset_revision
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

    pub(crate) fn set_render_terrain_skirt_enabled(&mut self, enabled: bool) {
        self.presentation_control.render_terrain_skirt = enabled;
    }

    pub(crate) fn set_screen_blur_enabled(&mut self, enabled: bool) {
        self.presentation_control.screen_blur_enabled = enabled;
    }

    pub(crate) fn set_minimap_rotation_degrees(&mut self, degrees: f32) {
        self.presentation_control.minimap_rotation_degrees = degrees;
    }

    pub(crate) fn set_minimap_skirt_mirroring(&mut self, enabled: bool) {
        self.presentation_control.minimap_skirt_mirroring = enabled;
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

    pub(crate) fn set_player_camera_v4(
        &mut self,
        player_id: PlayerId,
        controls: [bool; 3],
        location: Option<Vec3>,
        direction: Option<Vec3>,
        hover_height_offset: Option<f32>,
    ) {
        if player_id == 0 || self.get_player(player_id).is_none() {
            return;
        }
        let directive =
            if location.is_some() || direction.is_some() || hover_height_offset.is_some() {
                Some(CameraDirective {
                    revision: self.presentation_control.allocate_camera_revision(),
                    location,
                    direction,
                    hover_height_offset,
                })
            } else {
                None
            };
        self.update_player_presentation(player_id, |state| {
            state.camera_controls = CameraControlPermissions {
                scroll: controls[0],
                yaw: controls[1],
                zoom: controls[2],
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
    fn allocate_camera_revision(&mut self) -> u32 {
        self.next_camera_revision = self.next_camera_revision.wrapping_add(1).max(1);
        self.next_camera_revision
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        self.callouts.hash_state(checksum);
        self.hash_screen_fade(checksum);
        for enabled in self.hud_items {
            checksum.hash_u32(u32::from(enabled));
        }
        checksum.hash_u32(u32::from(self.render_terrain_skirt));
        checksum.hash_u32(u32::from(self.screen_blur_enabled));
        checksum.hash_f32(self.minimap_rotation_degrees);
        checksum.hash_u32(u32::from(self.minimap_skirt_mirroring));
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
    }
}

impl CameraDirective {
    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.revision);
        hash_optional_vec3(checksum, self.location);
        hash_optional_vec3(checksum, self.direction);
        hash_optional_f32(checksum, self.hover_height_offset);
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
