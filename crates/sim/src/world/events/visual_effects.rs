//! Bounded renderer-only requests derived from authoritative sim events.

use std::collections::VecDeque;

use crate::EntityId;
use crate::gameplay::ImpactEffectProfile;
use crate::player::PlayerId;
use crate::world::World;
use glam::Vec3;

const MAX_RETAINED_IMPACT_EFFECTS: usize = 256;

/// Surface identity resolved when a projectile lands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImpactSurface {
    /// Numeric XSD tile type, resolved through `terrainTileTypes.xml` by render.
    Terrain(u8),
    /// Surface name authored directly on the struck proto object.
    Object(String),
}

/// One transient projectile- or action-impact presentation request.
#[derive(Clone, Debug, PartialEq)]
pub struct ImpactEffectRequest {
    sequence: u64,
    occurred_at_ms: u32,
    projectile_id: EntityId,
    primary_target_id: Option<EntityId>,
    player_id: PlayerId,
    effect: ImpactEffectProfile,
    position: Vec3,
    forward: Vec3,
    surface: Option<ImpactSurface>,
    emit_surface_effect: bool,
}

pub(in crate::world) struct ImpactEffectRequestData {
    pub occurred_at_ms: u32,
    pub projectile_id: EntityId,
    pub primary_target_id: Option<EntityId>,
    pub player_id: PlayerId,
    pub effect: ImpactEffectProfile,
    pub position: Vec3,
    pub forward: Vec3,
    pub surface: Option<ImpactSurface>,
    pub emit_surface_effect: bool,
}

impl ImpactEffectRequest {
    pub(in crate::world) fn new(data: ImpactEffectRequestData) -> Self {
        Self {
            sequence: 0,
            occurred_at_ms: data.occurred_at_ms,
            projectile_id: data.projectile_id,
            primary_target_id: data.primary_target_id,
            player_id: data.player_id,
            effect: data.effect,
            position: data.position,
            forward: data.forward,
            surface: data.surface,
            emit_surface_effect: data.emit_surface_effect,
        }
    }

    /// Monotonic renderer cursor assigned when the request is queued.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Authoritative game time at which the impact occurred.
    #[must_use]
    pub const fn occurred_at_ms(&self) -> u32 {
        self.occurred_at_ms
    }

    /// Projectile or action-source entity that produced the request, even if now removed.
    #[must_use]
    pub const fn projectile_id(&self) -> EntityId {
        self.projectile_id
    }

    /// Concrete struck unit, when the impact had one.
    #[must_use]
    pub const fn primary_target_id(&self) -> Option<EntityId> {
        self.primary_target_id
    }

    /// Player that owns the effect instance.
    #[must_use]
    pub const fn player_id(&self) -> PlayerId {
        self.player_id
    }

    /// Synchronized tactic identity resolved later through `impacteffects.xml`.
    #[must_use]
    pub const fn effect(&self) -> &ImpactEffectProfile {
        &self.effect
    }

    /// World-space impact point.
    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    /// Retail TFX forward axis, including wall/base-shield overrides.
    #[must_use]
    pub const fn forward(&self) -> Vec3 {
        self.forward
    }

    /// Struck surface, or `None` for retail's invalid/default route.
    #[must_use]
    pub const fn surface(&self) -> Option<&ImpactSurface> {
        self.surface.as_ref()
    }

    /// Whether retail also instantiates the global terrain-tile TFX.
    #[must_use]
    pub const fn emits_surface_effect(&self) -> bool {
        self.emit_surface_effect
    }
}

#[derive(Debug, Default)]
pub(super) struct VisualEffectJournal {
    latest_sequence: u64,
    impacts: VecDeque<ImpactEffectRequest>,
}

impl VisualEffectJournal {
    pub(super) fn push(&mut self, mut request: ImpactEffectRequest) {
        self.latest_sequence = self.latest_sequence.wrapping_add(1).max(1);
        request.sequence = self.latest_sequence;
        if self.impacts.len() == MAX_RETAINED_IMPACT_EFFECTS {
            self.impacts.pop_front();
        }
        self.impacts.push_back(request);
    }
}

impl World {
    pub(in crate::world) fn queue_impact_effect(&mut self, request: ImpactEffectRequest) {
        self.presentation.visual_effects.push(request);
    }

    /// Latest transient-impact sequence available to renderer clients.
    #[must_use]
    pub const fn latest_impact_effect_sequence(&self) -> u64 {
        self.presentation.visual_effects.latest_sequence
    }

    /// Iterate retained requests newer than a renderer's local cursor.
    pub fn impact_effect_requests_after(
        &self,
        sequence: u64,
    ) -> impl Iterator<Item = &ImpactEffectRequest> {
        self.presentation
            .visual_effects
            .impacts
            .iter()
            .filter(move |request| sequence_is_after(request.sequence, sequence))
    }
}

const fn sequence_is_after(candidate: u64, cursor: u64) -> bool {
    candidate != cursor && candidate.wrapping_sub(cursor) < (1_u64 << 63)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameplay::ImpactEffectSize;

    fn request(projectile_index: u32) -> ImpactEffectRequest {
        ImpactEffectRequest::new(ImpactEffectRequestData {
            occurred_at_ms: 10,
            projectile_id: EntityId::new(crate::EntityClass::Projectile, projectile_index),
            primary_target_id: None,
            player_id: 1,
            effect: ImpactEffectProfile {
                name: "Impact".to_owned(),
                size: ImpactEffectSize::Large,
                do_shockwave_action: false,
            },
            position: Vec3::ZERO,
            forward: Vec3::Z,
            surface: Some(ImpactSurface::Terrain(2)),
            emit_surface_effect: true,
        })
    }

    #[test]
    fn journal_is_bounded_and_cursor_driven_without_sim_acknowledgement() {
        let mut world = World::new();
        for index in 0..300 {
            world.queue_impact_effect(request(index));
        }

        assert_eq!(world.latest_impact_effect_sequence(), 300);
        let retained = world.impact_effect_requests_after(0).collect::<Vec<_>>();
        assert_eq!(retained.len(), MAX_RETAINED_IMPACT_EFFECTS);
        assert_eq!(retained[0].sequence(), 45);
        assert_eq!(
            world
                .impact_effect_requests_after(299)
                .map(ImpactEffectRequest::sequence)
                .collect::<Vec<_>>(),
            [300]
        );
    }
}
