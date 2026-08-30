//! Bounded presentation-event queues emitted by visual animation tracks.

use std::collections::VecDeque;

use glam::{Mat4, Vec3};

use crate::ugx::unit::{UnitAnimationEvent, UnitAnimationEventKind, UnitTerrainAlphaShape};

const MAX_PENDING_EFFECTS: usize = 64;

/// Renderer-owned camera shake emitted by a crossed visual animation tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationCameraShake {
    source_owner_id: u64,
    position: Vec3,
    strength: f32,
    duration_seconds: f32,
    check_selected: bool,
}

impl AnimationCameraShake {
    /// Renderer owner identity used by UI selection adapters.
    #[must_use]
    pub const fn source_owner_id(self) -> u64 {
        self.source_owner_id
    }

    /// World-space source position used for retail on-screen filtering.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Authored camera displacement strength.
    #[must_use]
    pub const fn strength(self) -> f32 {
        self.strength
    }

    /// Authored hold duration before the retail default trail-off.
    #[must_use]
    pub const fn duration_seconds(self) -> f32 {
        self.duration_seconds
    }

    /// Whether retail requires the source unit to be selected.
    #[must_use]
    pub const fn check_selected(self) -> bool {
        self.check_selected
    }
}

/// Authored region written into the dynamic terrain-visibility mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationTerrainAlphaShape {
    /// Object-oriented rectangle with world-space half extents.
    Rectangle {
        /// Half extent along the object's local X axis.
        half_extent_x: f32,
        /// Half extent along the object's local Z axis.
        half_extent_z: f32,
    },
    /// World-space circle.
    Circle {
        /// Circle radius in world units.
        radius: f32,
    },
}

/// Renderer-owned dynamic terrain-alpha update from a crossed animation tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationTerrainAlpha {
    source_owner_id: u64,
    transform: Mat4,
    shape: AnimationTerrainAlphaShape,
    enabled: bool,
}

impl AnimationTerrainAlpha {
    /// Renderer owner identity for visibility adapters and diagnostics.
    #[must_use]
    pub const fn source_owner_id(self) -> u64 {
        self.source_owner_id
    }

    /// Source world transform; its translation is the region center.
    #[must_use]
    pub const fn transform(self) -> Mat4 {
        self.transform
    }

    /// Authored region geometry.
    #[must_use]
    pub const fn shape(self) -> AnimationTerrainAlphaShape {
        self.shape
    }

    /// Whether the region should reveal (`true`) or cut (`false`) terrain.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }
}

#[derive(Default)]
pub(super) struct AnimationEffects {
    camera_shakes: VecDeque<AnimationCameraShake>,
    terrain_alpha: VecDeque<AnimationTerrainAlpha>,
}

impl AnimationEffects {
    pub(super) fn route(
        &mut self,
        events: impl IntoIterator<Item = UnitAnimationEvent>,
    ) -> Vec<UnitAnimationEvent> {
        let mut remaining = Vec::new();
        for event in events {
            match event {
                UnitAnimationEvent {
                    kind:
                        UnitAnimationEventKind::CameraShake {
                            strength,
                            lifespan_seconds,
                            check_selected,
                        },
                    transform,
                    source_owner_id,
                    ..
                } => push_bounded(
                    &mut self.camera_shakes,
                    AnimationCameraShake {
                        source_owner_id,
                        position: transform.transform_point3(Vec3::ZERO),
                        strength,
                        duration_seconds: lifespan_seconds,
                        check_selected,
                    },
                ),
                UnitAnimationEvent {
                    kind: UnitAnimationEventKind::TerrainAlpha { shape, enabled },
                    transform,
                    source_owner_id,
                    ..
                } => {
                    let shape = match shape {
                        UnitTerrainAlphaShape::Rectangle { half_extents } => {
                            AnimationTerrainAlphaShape::Rectangle {
                                half_extent_x: half_extents.x,
                                half_extent_z: half_extents.y,
                            }
                        }
                        UnitTerrainAlphaShape::Circle { radius } => {
                            AnimationTerrainAlphaShape::Circle { radius }
                        }
                    };
                    push_bounded(
                        &mut self.terrain_alpha,
                        AnimationTerrainAlpha {
                            source_owner_id,
                            transform,
                            shape,
                            enabled,
                        },
                    );
                }
                event => remaining.push(event),
            }
        }
        remaining
    }

    pub(super) fn take_camera_shakes(&mut self) -> Vec<AnimationCameraShake> {
        self.camera_shakes.drain(..).collect()
    }

    pub(super) fn take_terrain_alpha(&mut self) -> Vec<AnimationTerrainAlpha> {
        self.terrain_alpha.drain(..).collect()
    }
}

fn push_bounded<T>(queue: &mut VecDeque<T>, item: T) {
    if queue.len() >= MAX_PENDING_EFFECTS {
        queue.pop_front();
    }
    queue.push_back(item);
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{MAX_PENDING_EFFECTS, push_bounded};

    #[test]
    fn presentation_queue_retains_the_newest_bounded_events() {
        let mut queue = VecDeque::new();
        for value in 0..MAX_PENDING_EFFECTS + 3 {
            push_bounded(&mut queue, value);
        }
        assert_eq!(queue.len(), MAX_PENDING_EFFECTS);
        assert_eq!(queue.front(), Some(&3));
    }
}
