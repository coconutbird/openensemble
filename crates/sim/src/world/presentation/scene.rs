//! Trigger camera follow, scene light animation and positional sounds for renderer and audio
//! adapters.

use std::collections::VecDeque;

use glam::Vec3;

use super::super::World;
use super::hash_optional_vec3;
use crate::EntityId;
use crate::sync::SyncChecksum;

/// Positional sounds that the world keeps for adapters that read them late.
const MAX_RETAINED_WORLD_SOUNDS: usize = 64;

/// The camera follow that `SetFollowCam` (521) set for every user.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowCamera {
    /// The nonzero identity of this directive, which increases with each new directive.
    pub revision: u32,
    /// The entity that the camera follows.
    pub entity: EntityId,
    /// The direction that the camera turns to, if authored. Retail turns the camera about
    /// the entity by the horizontal angle between the camera and this direction.
    pub direction: Option<Vec3>,
    /// If `true`, the users go to the follow mode (user mode 12). Otherwise they go to the
    /// normal mode (user mode 0).
    pub follow: bool,
}

/// The scene light animation that `LightsetAnimate` (533) started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightsetAnimation {
    /// The scenario `Lightsets` ID of the target lightset.
    pub lightset_id: i32,
    /// Game time in milliseconds when the animation started from the current light.
    pub start_ms: u32,
    /// Game time in milliseconds when the target light is reached.
    pub end_ms: u32,
}

/// A sound cue that plays at the position of an entity (`PlayWorldSoundOnEntity`, 515).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSoundRequest {
    sequence: u64,
    sound_cue: String,
    entity: EntityId,
}

impl WorldSoundRequest {
    /// The sequence number of the request. It increases with each request.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// The name of the sound cue.
    #[must_use]
    pub fn sound_cue(&self) -> &str {
        &self.sound_cue
    }

    /// The entity that the sound follows.
    #[must_use]
    pub const fn entity(&self) -> EntityId {
        self.entity
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct SceneCueState {
    next_follow_revision: u32,
    follow_camera: Option<FollowCamera>,
    lightset_ids: Vec<i32>,
    lightset_animation: Option<LightsetAnimation>,
    latest_world_sound_sequence: u64,
    world_sounds: VecDeque<WorldSoundRequest>,
}

impl World {
    /// Return the last camera follow directive, if a trigger set one.
    #[must_use]
    pub const fn follow_camera(&self) -> Option<FollowCamera> {
        self.presentation_control.scene.follow_camera
    }

    /// Return the last scene light animation, if a trigger started one.
    #[must_use]
    pub const fn lightset_animation(&self) -> Option<LightsetAnimation> {
        self.presentation_control.scene.lightset_animation
    }

    /// The sequence number of the latest positional sound request, or 0 if there is none.
    #[must_use]
    pub const fn latest_world_sound_sequence(&self) -> u64 {
        self.presentation_control.scene.latest_world_sound_sequence
    }

    /// Iterate the retained positional sound requests that are newer than an adapter cursor.
    pub fn world_sound_requests_after(
        &self,
        sequence: u64,
    ) -> impl Iterator<Item = &WorldSoundRequest> {
        self.presentation_control
            .scene
            .world_sounds
            .iter()
            .filter(move |request| {
                request.sequence != sequence
                    && request.sequence.wrapping_sub(sequence) < (1_u64 << 63)
            })
    }

    /// Set the IDs of the scenario `Lightsets` entries in document order
    /// (`BScenario::loadOtherLightset`).
    pub fn configure_lightsets(&mut self, lightset_ids: Vec<i32>) {
        self.presentation_control.scene.lightset_ids = lightset_ids;
    }

    pub(crate) fn set_follow_camera(
        &mut self,
        entity: EntityId,
        direction: Option<Vec3>,
        follow: bool,
    ) {
        let scene = &mut self.presentation_control.scene;
        scene.next_follow_revision = scene.next_follow_revision.wrapping_add(1).max(1);
        scene.follow_camera = Some(FollowCamera {
            revision: scene.next_follow_revision,
            entity,
            direction,
            follow,
        });
    }

    /// `BTriggerEffect_teLightsetAnimate` (`0x14046A880`): an unknown lightset ID changes
    /// nothing. Returns whether the animation started.
    pub(crate) fn animate_lightset(&mut self, lightset_id: i32, duration_ms: u32) -> bool {
        let start_ms = self.game_time_ms;
        let scene = &mut self.presentation_control.scene;
        if !scene.lightset_ids.contains(&lightset_id) {
            return false;
        }
        scene.lightset_animation = Some(LightsetAnimation {
            lightset_id,
            start_ms,
            end_ms: start_ms.wrapping_add(duration_ms),
        });
        true
    }

    pub(crate) fn play_world_sound(&mut self, sound_cue: String, entity: EntityId) {
        let scene = &mut self.presentation_control.scene;
        scene.latest_world_sound_sequence =
            scene.latest_world_sound_sequence.wrapping_add(1).max(1);
        if scene.world_sounds.len() == MAX_RETAINED_WORLD_SOUNDS {
            scene.world_sounds.pop_front();
        }
        scene.world_sounds.push_back(WorldSoundRequest {
            sequence: scene.latest_world_sound_sequence,
            sound_cue,
            entity,
        });
    }
}

impl SceneCueState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.next_follow_revision);
        if let Some(follow) = self.follow_camera {
            checksum.hash_u32(1);
            checksum.hash_u32(follow.revision);
            checksum.hash_u32(follow.entity.as_u32());
            hash_optional_vec3(checksum, follow.direction);
            checksum.hash_u32(u32::from(follow.follow));
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::try_from(self.lightset_ids.len()).unwrap_or(u32::MAX));
        for lightset_id in &self.lightset_ids {
            checksum.hash_i32(*lightset_id);
        }
        if let Some(animation) = self.lightset_animation {
            checksum.hash_u32(1);
            checksum.hash_i32(animation.lightset_id);
            checksum.hash_u32(animation.start_ms);
            checksum.hash_u32(animation.end_ms);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_bytes(&self.latest_world_sound_sequence.to_le_bytes());
        checksum.hash_u32(u32::try_from(self.world_sounds.len()).unwrap_or(u32::MAX));
        for request in &self.world_sounds {
            checksum.hash_bytes(&request.sequence.to_le_bytes());
            checksum.hash_string(&request.sound_cue);
            checksum.hash_u32(request.entity.as_u32());
        }
    }
}
