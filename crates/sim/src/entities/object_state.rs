//! Runtime state shared by retail `BObject`-derived entities.
//!
//! Units, class-0 objects, and projectiles are all `BObject` derivatives in
//! retail. Keeping their visual requests and fog-memory policy here lets the
//! simulation remain authoritative while renderers consume a read-only view.

use crate::EntityId;
use crate::sync::SyncChecksum;
use num_traits::ToPrimitive;

/// Trigger-authored animation selected and timed by the authoritative sim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedAnimation {
    revision: u32,
    animation_type: String,
    asset_path: Option<String>,
    started_at_ms: u32,
    duration_ms: u32,
}

impl ScriptedAnimation {
    /// Monotonic revision used by renderers to detect retriggers.
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.revision
    }

    /// Retail visual animation type, such as `Death` or `Research`.
    #[must_use]
    pub fn animation_type(&self) -> &str {
        &self.animation_type
    }

    /// Canonical UAX selected by scenario-layered gameplay data, when known.
    #[must_use]
    pub fn asset_path(&self) -> Option<&str> {
        self.asset_path.as_deref()
    }

    /// Authoritative game time at which playback began.
    #[must_use]
    pub const fn started_at_ms(&self) -> u32 {
        self.started_at_ms
    }

    /// Locked animation duration in milliseconds.
    #[must_use]
    pub const fn duration_ms(&self) -> u32 {
        self.duration_ms
    }

    /// Playback position derived entirely from authoritative sim time.
    #[must_use]
    pub fn normalized_position(&self, now_ms: u32) -> f32 {
        if self.duration_ms == 0 {
            return 1.0;
        }
        now_ms
            .wrapping_sub(self.started_at_ms)
            .min(self.duration_ms)
            .to_f32()
            .unwrap_or(f32::MAX)
            / self.duration_ms.to_f32().unwrap_or(f32::MAX)
    }
}

/// Retail targeting-selection texture state installed by `FlashEntity`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetingSelection {
    color: [u8; 4],
    started_at_ms: u32,
    expires_at_ms: Option<u32>,
    scroll_speed: f32,
    intensity: f32,
}

impl TargetingSelection {
    /// RGBA override tint used by the additive selection texture.
    #[must_use]
    pub const fn color(self) -> [u8; 4] {
        self.color
    }

    /// Authoritative game time at which this texture request began.
    #[must_use]
    pub const fn started_at_ms(self) -> u32 {
        self.started_at_ms
    }

    /// Absolute expiration time, or `None` for retail's indefinite timeout.
    #[must_use]
    pub const fn expires_at_ms(self) -> Option<u32> {
        self.expires_at_ms
    }

    /// World-height texture scroll speed in UV units per second.
    #[must_use]
    pub const fn scroll_speed(self) -> f32 {
        self.scroll_speed
    }

    /// Additive texture intensity authored by the trigger.
    #[must_use]
    pub const fn intensity(self) -> f32 {
        self.intensity
    }
}

/// Fog-memory behavior retained by a retail `BObject`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DopplePolicy {
    gray_map_dopples: bool,
    dopples: bool,
    reset_revision: u32,
    visibility_update_pending: bool,
}

impl DopplePolicy {
    /// Whether this object leaves a gray-map fog-memory representation.
    #[must_use]
    pub const fn gray_map_dopples(self) -> bool {
        self.gray_map_dopples
    }

    /// Whether this object leaves an ordinary fog-memory representation.
    #[must_use]
    pub const fn dopples(self) -> bool {
        self.dopples
    }

    /// Monotonic invalidation revision for previously created dopples.
    #[must_use]
    pub const fn reset_revision(self) -> u32 {
        self.reset_revision
    }

    /// Whether retail's forced visibility reconciliation is still pending.
    #[must_use]
    pub const fn visibility_update_pending(self) -> bool {
        self.visibility_update_pending
    }
}

/// Authoritative state common to the simulation's `BObject` derivatives.
#[derive(Debug, Clone)]
pub struct ObjectState {
    override_tint: [u8; 4],
    targeting_selection: Option<TargetingSelection>,
    scripted_animation: Option<ScriptedAnimation>,
    scripted_animation_revision: u32,
    attachments: Vec<EntityId>,
    attached_to: Option<EntityId>,
    gray_map_dopples: bool,
    dopples: bool,
    dopple_reset_revision: u32,
    force_visibility_update_next_frame: bool,
}

impl Default for ObjectState {
    fn default() -> Self {
        Self {
            override_tint: [0, 0, 0, u8::MAX],
            targeting_selection: None,
            scripted_animation: None,
            scripted_animation_revision: 0,
            attachments: Vec::new(),
            attached_to: None,
            gray_map_dopples: false,
            dopples: false,
            dopple_reset_revision: 0,
            force_visibility_update_next_frame: false,
        }
    }
}

impl ObjectState {
    /// Return the active targeting-selection request, if any.
    #[must_use]
    pub const fn targeting_selection(&self) -> Option<TargetingSelection> {
        self.targeting_selection
    }

    /// Return the current scripted animation, including its locked final pose.
    #[must_use]
    pub const fn scripted_animation(&self) -> Option<&ScriptedAnimation> {
        self.scripted_animation.as_ref()
    }

    /// Child entities attached to this retail object, in creation order.
    #[must_use]
    pub fn attachments(&self) -> &[EntityId] {
        &self.attachments
    }

    /// Parent object that owns this attachment, when present.
    #[must_use]
    pub const fn attached_to(&self) -> Option<EntityId> {
        self.attached_to
    }

    /// Return the current fog-memory policy and invalidation state.
    #[must_use]
    pub const fn dopple_policy(&self) -> DopplePolicy {
        DopplePolicy {
            gray_map_dopples: self.gray_map_dopples,
            dopples: self.dopples,
            reset_revision: self.dopple_reset_revision,
            visibility_update_pending: self.force_visibility_update_next_frame,
        }
    }

    pub(crate) fn flash(
        &mut self,
        now_ms: u32,
        interval_ms: u32,
        duration_ms: u32,
        color: [u8; 4],
        intensity: f32,
    ) {
        self.override_tint = color;
        let expires_at_ms = (duration_ms > 0).then(|| now_ms.wrapping_add(duration_ms));
        let scroll_speed = if interval_ms > 0 {
            -2.0 / (interval_ms.to_f32().unwrap_or(f32::MAX) * 0.001)
        } else {
            -2.0
        };
        let incoming = TargetingSelection {
            color,
            started_at_ms: now_ms,
            expires_at_ms,
            scroll_speed,
            intensity,
        };

        if self
            .targeting_selection
            .is_none_or(|existing| timeout_is_no_later(existing.expires_at_ms, expires_at_ms))
        {
            self.targeting_selection = Some(incoming);
        } else if let Some(existing) = &mut self.targeting_selection {
            // Retail changes mOverrideTint before its additive-timeout check.
            existing.color = self.override_tint;
        }
    }

    pub(crate) fn reset_dopples(&mut self, gray_map_dopples: bool, dopples: bool) {
        self.gray_map_dopples = gray_map_dopples;
        self.dopples = dopples;
        self.dopple_reset_revision = self.dopple_reset_revision.wrapping_add(1);
        self.force_visibility_update_next_frame = true;
    }

    pub(crate) fn play_scripted_animation(
        &mut self,
        now_ms: u32,
        animation_type: String,
        asset_path: Option<String>,
        duration_ms: u32,
    ) {
        self.scripted_animation_revision = self.scripted_animation_revision.wrapping_add(1);
        self.scripted_animation = Some(ScriptedAnimation {
            revision: self.scripted_animation_revision,
            animation_type,
            asset_path,
            started_at_ms: now_ms,
            duration_ms,
        });
    }

    pub(crate) fn add_attachment(&mut self, entity_id: EntityId) {
        self.attachments.push(entity_id);
    }

    pub(crate) fn remove_attachment(&mut self, entity_id: EntityId) {
        self.attachments.retain(|candidate| *candidate != entity_id);
    }

    pub(crate) fn take_attachments(&mut self) -> Vec<EntityId> {
        std::mem::take(&mut self.attachments)
    }

    pub(crate) fn set_attached_to(&mut self, parent_id: Option<EntityId>) {
        self.attached_to = parent_id;
    }

    pub(crate) fn update(&mut self, now_ms: u32) {
        if self
            .targeting_selection
            .and_then(TargetingSelection::expires_at_ms)
            .is_some_and(|expires_at_ms| expires_at_ms < now_ms)
        {
            self.targeting_selection = None;
        }
        self.force_visibility_update_next_frame = false;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        for channel in self.override_tint {
            checksum.hash_u32(u32::from(channel));
        }
        if let Some(selection) = self.targeting_selection {
            checksum.hash_u32(1);
            checksum.hash_u32(selection.started_at_ms);
            if let Some(expires_at_ms) = selection.expires_at_ms {
                checksum.hash_u32(1);
                checksum.hash_u32(expires_at_ms);
            } else {
                checksum.hash_u32(0);
            }
            checksum.hash_f32(selection.scroll_speed);
            checksum.hash_f32(selection.intensity);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(self.scripted_animation_revision);
        if let Some(animation) = &self.scripted_animation {
            checksum.hash_u32(1);
            checksum.hash_u32(u32::try_from(animation.animation_type.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(animation.animation_type.as_bytes());
            if let Some(asset_path) = &animation.asset_path {
                checksum.hash_u32(1);
                checksum.hash_u32(u32::try_from(asset_path.len()).unwrap_or(u32::MAX));
                checksum.hash_bytes(asset_path.as_bytes());
            } else {
                checksum.hash_u32(0);
            }
            checksum.hash_u32(animation.started_at_ms);
            checksum.hash_u32(animation.duration_ms);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::try_from(self.attachments.len()).unwrap_or(u32::MAX));
        for attachment in &self.attachments {
            checksum.hash_u32(attachment.as_u32());
        }
        if let Some(parent) = self.attached_to {
            checksum.hash_u32(1);
            checksum.hash_u32(parent.as_u32());
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::from(self.gray_map_dopples));
        checksum.hash_u32(u32::from(self.dopples));
        checksum.hash_u32(self.dopple_reset_revision);
        checksum.hash_u32(u32::from(self.force_visibility_update_next_frame));
    }
}

fn timeout_is_no_later(existing: Option<u32>, incoming: Option<u32>) -> bool {
    match (existing, incoming) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(existing), Some(incoming)) => existing <= incoming,
    }
}

#[cfg(test)]
mod tests {
    use super::ObjectState;

    #[test]
    fn indefinite_retrigger_does_not_replace_a_timed_additive_texture() {
        let mut state = ObjectState::default();
        state.flash(10, 500, 1_000, [255, 255, 0, 255], 20.0);
        state.flash(20, 250, 0, [255, 0, 0, 255], 80.0);

        let selection = state.targeting_selection().unwrap();
        assert_eq!(selection.started_at_ms(), 10);
        assert_eq!(selection.expires_at_ms(), Some(1_010));
        assert_eq!(selection.color(), [255, 0, 0, 255]);
        assert_eq!(selection.intensity().to_bits(), 20.0_f32.to_bits());
    }

    #[test]
    fn timeout_removal_uses_retails_strict_comparison() {
        let mut state = ObjectState::default();
        state.flash(0, 500, 1_000, [255; 4], 20.0);
        state.update(1_000);
        assert!(state.targeting_selection().is_some());
        state.update(1_001);
        assert!(state.targeting_selection().is_none());
    }

    #[test]
    fn scripted_animation_retriggers_and_locks_at_its_final_position() {
        let mut state = ObjectState::default();
        state.play_scripted_animation(
            100,
            "Death".to_owned(),
            Some("art\\bridge_death.uax".to_owned()),
            1_000,
        );
        let first = state.scripted_animation().unwrap();
        assert_eq!(first.revision(), 1);
        assert_eq!(first.normalized_position(600).to_bits(), 0.5_f32.to_bits());
        assert_eq!(
            first.normalized_position(1_101).to_bits(),
            1.0_f32.to_bits()
        );

        state.play_scripted_animation(1_200, "Research".to_owned(), None, 500);
        let second = state.scripted_animation().unwrap();
        assert_eq!(second.revision(), 2);
        assert_eq!(second.animation_type(), "Research");
        assert_eq!(
            second.normalized_position(1_200).to_bits(),
            0.0_f32.to_bits()
        );
    }
}
