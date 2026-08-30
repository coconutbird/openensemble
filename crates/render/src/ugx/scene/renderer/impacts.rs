//! Renderer-local dispatch state for synchronized impact requests.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use glam::{Mat4, Vec3};
use sim::World as SimWorld;

use super::super::{
    UnitScene,
    impact_decals::ImpactDecalRenderer,
    impact_visuals::ImpactVisualAssets,
    lights::{AttachedLightRenderer, LightPlacementState},
    particles::{AttachedParticleRenderer, ParticlePlacementState},
    terrain_effects::TerrainEffectAssets,
};
use super::RenderedPlacement;
use crate::terrain::{LightingParams, TerrainPatchWorldBindings};
use crate::terrain_effect::{
    ResolvedTerrainEffect, TerrainEffectAction, TerrainEffectRouteKind, TerrainImpactRouter,
};
use crate::ugx::UnitRenderer;
use crate::ugx::renderer::{SharedResources, VisualState};
use crate::ugx::unit::{UnitAnimationAnchor, UnitAnimationEvent, UnitAnimationEventKind};
use crate::{RenderPhase, WorldRenderer, wgpu};

const MAX_PENDING_IMPACT_ROUTES: usize = 512;
const TEMP_VISUAL_OWNER_BIT: u64 = 1_u64 << 63;

struct TimedVisual {
    owner_id: u64,
    transform: Mat4,
    remaining_seconds: f32,
    renderer: UnitRenderer,
}

struct PendingAnimationParticle {
    presentation_id: u64,
    path: String,
    transform: Mat4,
    lifespan_seconds: f32,
    tracks_anchor: bool,
}

struct PendingAnimationLight {
    presentation_id: u64,
    path: String,
    transform: Mat4,
    lifespan_seconds: f32,
    tracks_anchor: bool,
}

struct TimedAnimationAnchor {
    source_owner_id: u64,
    anchor: UnitAnimationAnchor,
    remaining_seconds: f32,
}

pub(super) struct ImpactRuntime {
    router: Option<TerrainImpactRouter>,
    visual_assets: ImpactVisualAssets,
    pending_particles: VecDeque<ResolvedTerrainEffect>,
    pending_lights: VecDeque<ResolvedTerrainEffect>,
    pending_visuals: VecDeque<ResolvedTerrainEffect>,
    visuals: BTreeMap<u64, TimedVisual>,
    expired_visual_owners: Vec<u64>,
    last_visual_time_seconds: Option<f32>,
    decals: Option<ImpactDecalRenderer>,
    pending_decals: VecDeque<ResolvedTerrainEffect>,
    animation_effect_assets: TerrainEffectAssets,
    animation_event_sequence: u64,
    pending_animation_particles: VecDeque<PendingAnimationParticle>,
    pending_animation_lights: VecDeque<PendingAnimationLight>,
    animation_anchors: BTreeMap<u64, TimedAnimationAnchor>,
    last_animation_anchor_time_seconds: Option<f32>,
}

impl ImpactRuntime {
    pub(super) fn new(scene: &UnitScene) -> Self {
        Self {
            router: scene
                .terrain_impact_assets
                .clone()
                .map(TerrainImpactRouter::new),
            visual_assets: scene.impact_visual_assets.clone(),
            pending_particles: VecDeque::new(),
            pending_lights: VecDeque::new(),
            pending_visuals: VecDeque::new(),
            visuals: BTreeMap::new(),
            expired_visual_owners: Vec::new(),
            last_visual_time_seconds: None,
            decals: None,
            pending_decals: VecDeque::new(),
            animation_effect_assets: scene.terrain_effect_assets.clone(),
            animation_event_sequence: 0,
            pending_animation_particles: VecDeque::new(),
            pending_animation_lights: VecDeque::new(),
            animation_anchors: BTreeMap::new(),
            last_animation_anchor_time_seconds: None,
        }
    }

    pub(super) fn sync_scene(&mut self, scene: &UnitScene) {
        if self.router.is_none() {
            self.router = scene
                .terrain_impact_assets
                .clone()
                .map(TerrainImpactRouter::new);
        }
        self.visual_assets = scene.impact_visual_assets.clone();
        self.animation_effect_assets = scene.terrain_effect_assets.clone();
    }

    pub(super) fn enable_decals(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        world: TerrainPatchWorldBindings<'_>,
        scene: &UnitScene,
    ) {
        self.decals = Some(ImpactDecalRenderer::new(
            device,
            queue,
            color_format,
            depth_format,
            world,
            &scene.impact_decal_assets,
        ));
        self.pending_decals.clear();
    }

    pub(super) fn collect(&mut self, world: &SimWorld, particles_enabled: bool) {
        let Some(router) = &mut self.router else {
            return;
        };
        let new_impacts = router.route_new(world);
        self.enqueue_routes(new_impacts, particles_enabled);
    }

    pub(super) fn collect_animation_events(
        &mut self,
        world: &SimWorld,
        events: impl IntoIterator<Item = UnitAnimationEvent>,
        particles_enabled: bool,
    ) {
        let mut routes = Vec::new();
        for event in events {
            let UnitAnimationEvent {
                kind,
                transform,
                source_owner_id,
                anchor,
            } = event;
            self.animation_event_sequence = self.animation_event_sequence.wrapping_add(1);
            let sequence = self.animation_event_sequence;
            let presentation_id = animation_event_id(sequence);
            match kind {
                UnitAnimationEventKind::Particle {
                    path,
                    lifespan_seconds,
                } => {
                    let tracks_anchor = self.insert_animation_anchor(
                        presentation_id,
                        source_owner_id,
                        anchor,
                        lifespan_seconds,
                    );
                    push_bounded(
                        &mut self.pending_animation_particles,
                        PendingAnimationParticle {
                            presentation_id,
                            path,
                            transform,
                            lifespan_seconds,
                            tracks_anchor,
                        },
                    );
                }
                UnitAnimationEventKind::Light {
                    path,
                    lifespan_seconds,
                } => {
                    let tracks_anchor = self.insert_animation_anchor(
                        presentation_id,
                        source_owner_id,
                        anchor,
                        lifespan_seconds,
                    );
                    push_bounded(
                        &mut self.pending_animation_lights,
                        PendingAnimationLight {
                            presentation_id,
                            path,
                            transform,
                            lifespan_seconds,
                            tracks_anchor,
                        },
                    );
                }
                UnitAnimationEventKind::TerrainEffect(path) => {
                    let position = transform.transform_point3(Vec3::ZERO);
                    let forward = transform.transform_vector3(Vec3::Z).normalize_or_zero();
                    if !position.is_finite() || !forward.is_finite() || forward == Vec3::ZERO {
                        continue;
                    }
                    let roll = animation_event_roll(sequence, &path);
                    let Some(item) = self.animation_effect_assets.select(
                        &path,
                        world.terrain_surface_type(position),
                        roll,
                    ) else {
                        continue;
                    };
                    routes.push(ResolvedTerrainEffect {
                        sequence,
                        kind: TerrainEffectRouteKind::AnimationTag,
                        terrain_effect_path: crate::terrain_effect::canonical_terrain_effect_path(
                            &path,
                        ),
                        item,
                        position,
                        forward,
                        player_id: sim::PlayerId::MAX,
                        lifespan_seconds: 0.0,
                    });
                }
                UnitAnimationEventKind::CameraShake { .. }
                | UnitAnimationEventKind::TerrainAlpha { .. } => {}
            }
        }
        self.enqueue_routes(routes, particles_enabled);
    }

    fn insert_animation_anchor(
        &mut self,
        presentation_id: u64,
        source_owner_id: u64,
        anchor: Option<UnitAnimationAnchor>,
        lifespan_seconds: f32,
    ) -> bool {
        let Some(anchor) = anchor else {
            return false;
        };
        self.animation_anchors.insert(
            presentation_id,
            TimedAnimationAnchor {
                source_owner_id,
                anchor,
                remaining_seconds: lifespan_seconds,
            },
        );
        true
    }

    pub(super) fn advance_animation_anchors(&mut self, time_seconds: f32) {
        let delta_seconds = frame_delta(self.last_animation_anchor_time_seconds, time_seconds);
        self.last_animation_anchor_time_seconds = time_seconds.is_finite().then_some(time_seconds);
        for binding in self.animation_anchors.values_mut() {
            binding.remaining_seconds = (binding.remaining_seconds - delta_seconds).max(0.0);
        }
        self.animation_anchors
            .retain(|_, binding| binding.remaining_seconds > 0.0);
        self.pending_animation_particles.retain(|event| {
            !event.tracks_anchor || self.animation_anchors.contains_key(&event.presentation_id)
        });
        self.pending_animation_lights.retain(|event| {
            !event.tracks_anchor || self.animation_anchors.contains_key(&event.presentation_id)
        });
    }

    pub(super) fn update_animation_particle_anchors(
        &mut self,
        placements: &[RenderedPlacement],
        mut particles: Option<&mut AttachedParticleRenderer>,
    ) {
        let mut missing = Vec::new();
        for (&presentation_id, binding) in &self.animation_anchors {
            let Some(transform) = animation_anchor_transform(placements, binding) else {
                missing.push(presentation_id);
                continue;
            };
            if let Some(particles) = particles.as_deref_mut() {
                particles.update_one_shot_transform(presentation_id, transform);
            }
        }
        for presentation_id in missing {
            self.animation_anchors.remove(&presentation_id);
        }
    }

    pub(super) fn update_animation_light_anchors(
        &mut self,
        placements: &[RenderedPlacement],
        lights: &mut AttachedLightRenderer,
    ) {
        let mut missing = Vec::new();
        for (&presentation_id, binding) in &self.animation_anchors {
            let Some(transform) = animation_anchor_transform(placements, binding) else {
                missing.push(presentation_id);
                continue;
            };
            lights.update_timed_transform(presentation_id, transform);
        }
        for presentation_id in missing {
            self.animation_anchors.remove(&presentation_id);
        }
    }

    fn enqueue_routes(
        &mut self,
        routes: impl IntoIterator<Item = ResolvedTerrainEffect>,
        particles_enabled: bool,
    ) {
        let routes = routes.into_iter().collect::<Vec<_>>();
        if particles_enabled {
            extend_bounded(&mut self.pending_particles, routes.iter().cloned());
        }
        extend_bounded(&mut self.pending_visuals, routes.iter().cloned());
        if self.decals.is_some() {
            extend_bounded(&mut self.pending_decals, routes.iter().cloned());
        }
        extend_bounded(&mut self.pending_lights, routes);
    }

    pub(super) fn spawn_particles(
        &mut self,
        particles: Option<&mut AttachedParticleRenderer>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let Some(particles) = particles else {
            self.pending_particles.clear();
            self.pending_animation_particles.clear();
            return;
        };
        while let Some(event) = self.pending_animation_particles.pop_front() {
            if event.tracks_anchor && !self.animation_anchors.contains_key(&event.presentation_id) {
                continue;
            }
            particles.spawn_one_shot(
                device,
                queue,
                event.presentation_id,
                &event.path,
                event.transform,
                event.lifespan_seconds,
            );
        }
        while let Some(route) = self.pending_particles.pop_front() {
            let Some(transform) = impact_transform(route.position, route.forward) else {
                continue;
            };
            if let Some((action_index, path)) = last_particle_action(&route) {
                particles.spawn_one_shot(
                    device,
                    queue,
                    presentation_id(&route, action_index),
                    path,
                    transform,
                    route.lifespan_seconds,
                );
            }
            if let Some((action_index, name)) = last_visual_action(&route)
                && let Some(path) = self
                    .visual_assets
                    .get(name)
                    .and_then(|asset| asset.particle_path.as_deref())
            {
                particles.spawn_one_shot(
                    device,
                    queue,
                    presentation_id(&route, action_index),
                    path,
                    transform,
                    route.lifespan_seconds,
                );
            }
        }
    }

    pub(super) fn update_visuals(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shared: &Arc<SharedResources>,
        view_projection: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
    ) {
        let delta_seconds = frame_delta(self.last_visual_time_seconds, time_seconds);
        self.last_visual_time_seconds = time_seconds.is_finite().then_some(time_seconds);
        for visual in self.visuals.values_mut() {
            visual.remaining_seconds = (visual.remaining_seconds - delta_seconds).max(0.0);
        }
        self.visuals.retain(|_, visual| {
            let keep = visual.remaining_seconds > 0.0;
            if !keep {
                self.expired_visual_owners.push(visual.owner_id);
            }
            keep
        });

        while let Some(route) = self.pending_visuals.pop_front() {
            if !route.lifespan_seconds.is_finite() || route.lifespan_seconds <= 0.0 {
                continue;
            }
            let Some(transform) = impact_transform(route.position, route.forward) else {
                continue;
            };
            let Some((action_index, name)) = last_visual_action(&route) else {
                continue;
            };
            let Some(unit) = self
                .visual_assets
                .get(name)
                .and_then(|asset| asset.unit.as_ref())
            else {
                continue;
            };
            let owner_id = temporary_visual_owner_id(presentation_id(&route, action_index));
            self.visuals.insert(
                owner_id,
                TimedVisual {
                    owner_id,
                    transform,
                    remaining_seconds: route.lifespan_seconds,
                    renderer: UnitRenderer::new_with_shared(device, queue, unit, transform, shared),
                },
            );
        }

        for visual in self.visuals.values_mut() {
            visual.renderer.update_frame_with_visual_state_at_time(
                queue,
                view_projection,
                visual.transform,
                lighting,
                time_seconds,
                VisualState::default(),
            );
        }
    }

    pub(super) fn update_visual_particle_attachments(
        &self,
        particles: &mut AttachedParticleRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &SimWorld,
    ) {
        for visual in self.visuals.values() {
            particles.update_placement(
                device,
                queue,
                ParticlePlacementState {
                    owner_id: visual.owner_id,
                    animation_type: None,
                    movement_animation_type: None,
                    animation_revision: 0,
                    transform: visual.transform,
                    secondary_transform: visual.transform,
                    visible: true,
                    emitter_opacity: 1.0,
                    renderer: &visual.renderer,
                    world,
                },
            );
        }
    }

    pub(super) fn update_visual_light_attachments(&mut self, lights: &mut AttachedLightRenderer) {
        for owner_id in self.expired_visual_owners.drain(..) {
            lights.remove_owner(owner_id);
        }
        for visual in self.visuals.values() {
            lights.sync_placement(LightPlacementState {
                owner_id: visual.owner_id,
                animation_type: None,
                movement_animation_type: None,
                animation_revision: 0,
                transform: visual.transform,
                visible: true,
                renderer: &visual.renderer,
            });
        }
    }

    pub(super) fn spawn_lights(&mut self, lights: &mut AttachedLightRenderer) {
        while let Some(event) = self.pending_animation_lights.pop_front() {
            if event.tracks_anchor && !self.animation_anchors.contains_key(&event.presentation_id) {
                continue;
            }
            lights.spawn_timed(
                event.presentation_id,
                &event.path,
                event.transform,
                event.lifespan_seconds,
            );
        }
        while let Some(route) = self.pending_lights.pop_front() {
            if !route.position.is_finite() {
                continue;
            }
            let Some((action_index, light)) = last_light_action(&route) else {
                continue;
            };
            lights.spawn_timed(
                presentation_id(&route, action_index),
                &light.path,
                Mat4::from_translation(route.position),
                light.lifespan_seconds,
            );
        }
    }

    pub(super) fn update_decals(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        time_seconds: f32,
        view_projection: Mat4,
        lighting: &LightingParams,
    ) {
        let Some(decals) = &mut self.decals else {
            self.pending_decals.clear();
            return;
        };
        while let Some(route) = self.pending_decals.pop_front() {
            let Some((action_index, decal)) = last_decal_action(&route) else {
                continue;
            };
            decals.spawn(
                presentation_id(&route, action_index),
                decal,
                route.position,
                route.forward,
            );
        }
        decals.update_frame(device, queue, time_seconds, view_projection, lighting);
    }

    pub(super) fn render_phase<'pass>(
        &'pass self,
        phase: RenderPhase,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        for visual in self.visuals.values() {
            visual.renderer.render_phase(phase, pass);
        }
        if let Some(decals) = &self.decals {
            decals.render_phase(phase, pass);
        }
    }

    pub(super) fn issues(&self) -> &[String] {
        self.router
            .as_ref()
            .map_or(&[], TerrainImpactRouter::issues)
    }

    pub(super) fn live_decal_count(&self) -> usize {
        self.decals
            .as_ref()
            .map_or(0, ImpactDecalRenderer::live_count)
    }

    pub(super) fn live_visual_count(&self) -> usize {
        self.visuals.len()
    }
}

fn animation_anchor_transform(
    placements: &[RenderedPlacement],
    binding: &TimedAnimationAnchor,
) -> Option<Mat4> {
    placements
        .iter()
        .find(|placement| u64::from(placement.entity_id.as_u32()) == binding.source_owner_id)?
        .renderer
        .animation_anchor_world_transform(&binding.anchor)
}

fn extend_bounded(
    pending: &mut VecDeque<ResolvedTerrainEffect>,
    routes: impl IntoIterator<Item = ResolvedTerrainEffect>,
) {
    for route in routes {
        if pending.len() == MAX_PENDING_IMPACT_ROUTES {
            pending.pop_front();
        }
        pending.push_back(route);
    }
}

fn push_bounded<T>(pending: &mut VecDeque<T>, value: T) {
    if pending.len() == MAX_PENDING_IMPACT_ROUTES {
        pending.pop_front();
    }
    pending.push_back(value);
}

fn last_particle_action(route: &ResolvedTerrainEffect) -> Option<(usize, &str)> {
    route
        .item
        .actions
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, action)| {
            if let TerrainEffectAction::Particle(path) = action {
                Some((index, path.as_str()))
            } else {
                None
            }
        })
}

fn last_light_action(
    route: &ResolvedTerrainEffect,
) -> Option<(usize, &crate::terrain_effect::TerrainLight)> {
    route
        .item
        .actions
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, action)| {
            if let TerrainEffectAction::Light(light) = action {
                Some((index, light))
            } else {
                None
            }
        })
}

fn last_decal_action(
    route: &ResolvedTerrainEffect,
) -> Option<(usize, &crate::terrain_effect::TerrainImpactDecal)> {
    route
        .item
        .actions
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, action)| {
            if let TerrainEffectAction::ImpactDecal(decal) = action {
                Some((index, decal))
            } else {
                None
            }
        })
}

fn last_visual_action(route: &ResolvedTerrainEffect) -> Option<(usize, &str)> {
    route
        .item
        .actions
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, action)| {
            if let TerrainEffectAction::Visual(name) = action {
                Some((index, name.as_str()))
            } else {
                None
            }
        })
}

fn presentation_id(route: &ResolvedTerrainEffect, action_index: usize) -> u64 {
    let route_salt = match route.kind {
        TerrainEffectRouteKind::Impact => 0x243f_6a88_85a3_08d3,
        TerrainEffectRouteKind::Surface => 0x1319_8a2e_0370_7344,
        TerrainEffectRouteKind::AnimationTag => 0xa409_3822_299f_31d0,
    };
    route.sequence
        ^ route_salt
        ^ u64::try_from(action_index)
            .unwrap_or(u64::MAX)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

fn animation_event_roll(sequence: u64, path: &str) -> u32 {
    let mut value = sequence ^ 0xa409_3822_299f_31d0;
    for byte in path.bytes() {
        value ^= u64::from(byte.to_ascii_lowercase());
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mixed = value ^ (value >> 32);
    let [a, b, c, d, _, _, _, _] = mixed.to_le_bytes();
    u32::from_le_bytes([a, b, c, d])
}

const fn animation_event_id(sequence: u64) -> u64 {
    sequence ^ 0xa409_3822_299f_31d0
}

const fn temporary_visual_owner_id(presentation_id: u64) -> u64 {
    TEMP_VISUAL_OWNER_BIT | (presentation_id & !TEMP_VISUAL_OWNER_BIT)
}

fn frame_delta(last_time_seconds: Option<f32>, time_seconds: f32) -> f32 {
    last_time_seconds.map_or(0.0, |last| {
        if time_seconds.is_finite() && last.is_finite() {
            (time_seconds - last).max(0.0)
        } else {
            0.0
        }
    })
}

fn impact_transform(position: Vec3, authored_forward: Vec3) -> Option<Mat4> {
    if !position.is_finite() || !authored_forward.is_finite() {
        return None;
    }
    let forward = authored_forward.try_normalize()?;
    let reference_up = if forward.dot(Vec3::Y).abs() > 0.999 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let right = reference_up.cross(forward).try_normalize()?;
    let up = forward.cross(right).try_normalize()?;
    Some(Mat4::from_cols(
        right.extend(0.0),
        up.extend(0.0),
        forward.extend(0.0),
        position.extend(1.0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn impact_transform_places_local_forward_on_the_recovered_world_axis() {
        let position = Vec3::new(4.0, 5.0, 6.0);
        let transform = impact_transform(position, Vec3::X).expect("valid impact basis");

        assert!(
            transform
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(position, 1.0e-6)
        );
        assert!(
            transform
                .transform_vector3(Vec3::Z)
                .abs_diff_eq(Vec3::X, 1.0e-6)
        );
        assert!(transform.determinant() > 0.0);
    }

    #[test]
    fn temporary_visual_owner_ids_do_not_overlap_sim_entity_ids() {
        assert_eq!(temporary_visual_owner_id(7), TEMP_VISUAL_OWNER_BIT | 7);
        assert_ne!(temporary_visual_owner_id(7), 7);
    }

    #[test]
    fn visual_lifetime_delta_never_rewinds() {
        assert_near(frame_delta(None, 3.0), 0.0);
        assert_near(frame_delta(Some(3.0), 3.25), 0.25);
        assert_near(frame_delta(Some(3.0), 2.0), 0.0);
        assert_near(frame_delta(Some(3.0), f32::NAN), 0.0);
    }
}
