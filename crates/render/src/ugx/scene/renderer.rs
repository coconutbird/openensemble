//! GPU presentation for unit visuals bound to simulation entities.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;
use sim::{EntityId, TargetingSelection, TeamId, World as SimWorld};

use super::{
    UnitScene,
    combat_animations::combat_animation_position,
    lights::{AttachedLightRenderer, LightPlacementState},
    particles::{AttachedParticleRenderer, ParticlePlacementState},
    simulation_entity_animation, simulation_entity_transform, simulation_entity_visible_to_team,
    simulation_entity_visual_opacity,
};
mod animation_effects;
mod impacts;

use crate::environment::EnvironmentMap;
use crate::lighting::{LocalLightSet, LocalLightView};
use crate::particle::{ParticleScene, ParticleSceneTextures};
use crate::terrain::{LightingParams, TerrainPatchWorldBindings};
use crate::ugx::renderer::{SelectionOverlay, SharedResources, VisualState};
use crate::ugx::unit::{UnitAnimationEvent, UnitAnimationFrame};
use crate::ugx::{RendererResources, UnitRenderer, WorldBindings};
use crate::{RenderPhase, WorldRenderer};
use animation_clock::PresentationAnimationClock;
use animation_effects::AnimationEffects;
use impacts::ImpactRuntime;

mod animation_clock;

pub use animation_effects::{
    AnimationCameraShake, AnimationTerrainAlpha, AnimationTerrainAlphaShape,
};

struct RenderedPlacement {
    entity_id: EntityId,
    proto_name: String,
    visual_variation_index: Option<usize>,
    animation_type: Option<String>,
    animation_asset: Option<String>,
    animation_uses_simulation_clock: bool,
    animation_revision: u32,
    combat_animation_duration: Option<f32>,
    movement_track_animation: Option<String>,
    visual_mesh_revision: u32,
    visual_opacity: f32,
    transform: Mat4,
    secondary_transform: Option<Mat4>,
    visible: bool,
    visual_bounds_min: Vec3,
    visual_bounds_max: Vec3,
    selection: SelectionOverlay,
    animation_clock: PresentationAnimationClock,
    renderer: UnitRenderer,
}

#[derive(Clone, Copy)]
struct FrameUpdate<'frame> {
    viewer_team: Option<TeamId>,
    view_projection: Mat4,
    lighting: &'frame LightingParams,
    time_seconds: f32,
    particle_frame: Option<(&'frame wgpu::Device, &'frame ParticleScene)>,
}

/// GPU resources for every successfully decoded unit placement.
///
/// Simulation-bound placements fetch position and facing from [`SimWorld`]
/// each frame. This type owns only GPU/UI state; it never advances gameplay.
pub struct UnitSceneRenderer {
    placements: Vec<RenderedPlacement>,
    shared: Arc<SharedResources>,
    lights: AttachedLightRenderer,
    particles: Option<AttachedParticleRenderer>,
    impacts: ImpactRuntime,
    animation_effects: AnimationEffects,
}

impl UnitSceneRenderer {
    /// Upload every placed unit and its recursive component graph.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            scene,
            WorldBindings::default(),
        )
    }

    /// Upload all placements with a scenario-global environment fallback.
    #[must_use]
    pub fn new_with_environment(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
        environment: Option<&EnvironmentMap>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            scene,
            WorldBindings {
                environment,
                ..WorldBindings::default()
            },
        )
    }

    /// Upload all placements with environment and directional-shadow inputs.
    #[must_use]
    pub fn new_with_environment_and_shadow(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
        environment: Option<&EnvironmentMap>,
        shadow_view: Option<&wgpu::TextureView>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            scene,
            WorldBindings {
                environment,
                directional_shadow: shadow_view,
                ..WorldBindings::default()
            },
        )
    }

    /// Upload all placements with every scenario-global rendering input.
    #[must_use]
    pub fn new_with_world(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        scene: &UnitScene,
        world: WorldBindings<'_>,
    ) -> Self {
        let resources = RendererResources::new_with_world(device, queue, surface_format, world);
        Self::new_with_resources(device, queue, scene, &resources)
    }

    /// Upload all placements using an existing scenario-global pipeline set.
    #[must_use]
    pub fn new_with_resources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &UnitScene,
        resources: &RendererResources,
    ) -> Self {
        let placements = scene
            .placements
            .iter()
            .map(|placement| RenderedPlacement {
                entity_id: placement.entity_id(),
                proto_name: placement.proto_name().to_owned(),
                visual_variation_index: placement.visual_variation_index(),
                animation_type: placement.animation_type().map(str::to_owned),
                animation_asset: placement.animation_asset().map(str::to_owned),
                animation_uses_simulation_clock: placement.animation_uses_simulation_clock(),
                animation_revision: placement.animation_revision(),
                combat_animation_duration: placement.combat_animation_duration,
                movement_track_animation: placement
                    .movement_track_animation_type()
                    .map(str::to_owned),
                visual_mesh_revision: placement.visual_mesh_mask().revision(),
                visual_opacity: placement.visual_opacity(),
                transform: placement.transform,
                secondary_transform: placement.secondary_transform,
                visible: true,
                visual_bounds_min: Vec3::from_array(placement.unit.bounds_min()),
                visual_bounds_max: Vec3::from_array(placement.unit.bounds_max()),
                selection: SelectionOverlay::default(),
                animation_clock: PresentationAnimationClock::default(),
                renderer: configured_unit_renderer(device, queue, placement, &resources.shared),
            })
            .collect::<Vec<_>>();
        let mut lights = AttachedLightRenderer::new(&scene.light_assets);
        for placement in &placements {
            lights.sync_placement(light_placement_state(placement));
        }
        lights.end_sync();
        Self {
            placements,
            shared: Arc::clone(&resources.shared),
            lights,
            particles: None,
            impacts: ImpactRuntime::new(scene),
            animation_effects: AnimationEffects::default(),
        }
    }

    /// Drains newly crossed animation-tag camera shakes.
    pub fn take_animation_camera_shakes(&mut self) -> Vec<AnimationCameraShake> {
        self.animation_effects.take_camera_shakes()
    }

    /// Drains newly crossed dynamic terrain-alpha animation tags.
    pub fn take_animation_terrain_alpha(&mut self) -> Vec<AnimationTerrainAlpha> {
        self.animation_effects.take_terrain_alpha()
    }

    /// Enables authored PFX attachments against the active scene depth target.
    pub fn enable_attached_particles(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        scene_textures: ParticleSceneTextures<'_>,
        scene: &UnitScene,
        world: &SimWorld,
    ) {
        let mut particles = AttachedParticleRenderer::new(color_format, scene_textures);
        particles.begin_sync(&scene.particle_assets, &scene.terrain_effect_assets);
        for placement in &self.placements {
            particles.sync_placement(device, queue, particle_placement_state(placement, world));
        }
        particles.end_sync();
        self.particles = Some(particles);
    }

    /// Rebinds PFX depth/light-volume inputs after render-target recreation.
    pub fn set_particle_scene_textures(&mut self, scene_textures: ParticleSceneTextures<'_>) {
        if let Some(particles) = &mut self.particles {
            particles.set_scene_textures(scene_textures);
        }
    }

    /// Enables timed TFX impact decals against the active terrain bindings.
    pub fn enable_impact_decals(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        world: TerrainPatchWorldBindings<'_>,
        scene: &UnitScene,
    ) {
        self.impacts
            .enable_decals(device, queue, color_format, depth_format, world, scene);
    }

    /// Synchronize uploaded instances with a refreshed simulation scene.
    ///
    /// Existing entity/prototype pairs retain their GPU resources. New entities
    /// receive an instance renderer and removed entities are dropped.
    pub fn sync_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &UnitScene,
        world: &SimWorld,
    ) {
        let mut previous = self
            .placements
            .drain(..)
            .map(|placement| (placement.entity_id, placement))
            .collect::<HashMap<_, _>>();
        self.placements = scene
            .placements
            .iter()
            .map(|placement| {
                let mut previous_placement = previous.remove(&placement.entity_id());
                let animation_clock = previous_placement
                    .as_ref()
                    .filter(|rendered| same_visual(rendered, placement))
                    .map_or_else(PresentationAnimationClock::default, |rendered| {
                        rendered.animation_clock
                    });
                if previous_placement
                    .as_ref()
                    .is_some_and(|rendered| renderer_is_reusable(rendered, placement))
                    && let Some(mut rendered) = previous_placement.take()
                {
                    rendered.transform = placement.transform;
                    rendered.secondary_transform = placement.secondary_transform;
                    rendered.visual_opacity = placement.visual_opacity();
                    rendered.animation_type = placement.animation_type().map(str::to_owned);
                    rendered.animation_asset = placement.animation_asset().map(str::to_owned);
                    rendered.animation_uses_simulation_clock =
                        placement.animation_uses_simulation_clock();
                    rendered.combat_animation_duration = placement.combat_animation_duration;
                    rendered.movement_track_animation =
                        placement.movement_track_animation_type().map(str::to_owned);
                    rendered.visible = true;
                    return rendered;
                }
                let mut renderer = configured_unit_renderer(device, queue, placement, &self.shared);
                if let Some(previous) = previous_placement
                    .as_ref()
                    .filter(|rendered| same_visual(rendered, placement))
                {
                    renderer.inherit_ik_from(&previous.renderer);
                }
                RenderedPlacement {
                    entity_id: placement.entity_id(),
                    proto_name: placement.proto_name().to_owned(),
                    visual_variation_index: placement.visual_variation_index(),
                    animation_type: placement.animation_type().map(str::to_owned),
                    animation_asset: placement.animation_asset().map(str::to_owned),
                    animation_uses_simulation_clock: placement.animation_uses_simulation_clock(),
                    animation_revision: placement.animation_revision(),
                    combat_animation_duration: placement.combat_animation_duration,
                    movement_track_animation: placement
                        .movement_track_animation_type()
                        .map(str::to_owned),
                    visual_mesh_revision: placement.visual_mesh_mask().revision(),
                    visual_opacity: placement.visual_opacity(),
                    transform: placement.transform,
                    secondary_transform: placement.secondary_transform,
                    visible: true,
                    visual_bounds_min: Vec3::from_array(placement.unit.bounds_min()),
                    visual_bounds_max: Vec3::from_array(placement.unit.bounds_max()),
                    selection: SelectionOverlay::default(),
                    animation_clock,
                    renderer,
                }
            })
            .collect();
        self.sync_attached_particles(device, queue, scene, world);
        self.sync_attached_lights(scene);
        self.impacts.sync_scene(scene);
    }

    /// Refresh live transforms/poses and evaluate render-only `.lgt` attachments.
    ///
    /// This is a presentation prepass: it reads authoritative sim state but
    /// never advances or mutates gameplay. Run it before building the shared
    /// lighting constants so terrain and units observe the same light payload.
    pub fn update_attached_lights_at_time(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        time_seconds: f32,
        view: LocalLightView,
        intensity_scale: f32,
        output: &mut LocalLightSet,
    ) {
        self.impacts.advance_animation_anchors(time_seconds);
        self.impacts.collect(world, self.particles.is_some());
        let animation_events =
            update_placement_states(&mut self.placements, queue, world, None, time_seconds);
        self.collect_animation_events(world, animation_events);
        self.impacts
            .update_animation_light_anchors(&self.placements, &mut self.lights);
        self.impacts.spawn_lights(&mut self.lights);
        self.lights.begin_frame();
        for placement in &self.placements {
            self.lights
                .update_placement(light_placement_state(placement));
        }
        self.impacts
            .update_visual_light_attachments(&mut self.lights);
        self.lights
            .finish_frame(time_seconds, view, intensity_scale, output);
    }

    /// Present the latest authoritative simulation transforms.
    pub fn update_from_world(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        view_projection: Mat4,
        lighting: &LightingParams,
    ) {
        self.update_from_world_at_time(queue, world, view_projection, lighting, 0.0);
    }

    /// Present the latest sim state as visible to one team.
    pub fn update_from_world_for_team(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        team_id: TeamId,
        view_projection: Mat4,
        lighting: &LightingParams,
    ) {
        self.update_from_world_for_team_at_time(
            queue,
            world,
            team_id,
            view_projection,
            lighting,
            0.0,
        );
    }

    /// Present sim state with animated material UVs at the supplied render time.
    pub fn update_from_world_at_time(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        view_projection: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
    ) {
        self.update_from_world_projection(
            queue,
            world,
            FrameUpdate {
                viewer_team: None,
                view_projection,
                lighting,
                time_seconds,
                particle_frame: None,
            },
        );
    }

    /// Present animated sim state as visible to one team.
    pub fn update_from_world_for_team_at_time(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        team_id: TeamId,
        view_projection: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
    ) {
        self.update_from_world_projection(
            queue,
            world,
            FrameUpdate {
                viewer_team: Some(team_id),
                view_projection,
                lighting,
                time_seconds,
                particle_frame: None,
            },
        );
    }

    /// Presents animated sim state and advances renderer-only attached PFX.
    pub fn update_from_world_with_particles_at_time(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &SimWorld,
        lighting: &LightingParams,
        time_seconds: f32,
        particle_scene: &ParticleScene,
    ) {
        self.update_from_world_projection(
            queue,
            world,
            FrameUpdate {
                viewer_team: None,
                view_projection: particle_scene.view_projection,
                lighting,
                time_seconds,
                particle_frame: Some((device, particle_scene)),
            },
        );
    }

    fn update_from_world_projection(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        update: FrameUpdate<'_>,
    ) {
        self.impacts.advance_animation_anchors(update.time_seconds);
        self.impacts.collect(world, self.particles.is_some());
        let animation_events = update_placement_states(
            &mut self.placements,
            queue,
            world,
            update.viewer_team,
            update.time_seconds,
        );
        self.collect_animation_events(world, animation_events);
        self.impacts
            .update_animation_particle_anchors(&self.placements, self.particles.as_mut());
        if let Some((device, _)) = update.particle_frame {
            if let Some(particles) = self.particles.as_mut() {
                particles.begin_frame();
            }
            self.impacts
                .spawn_particles(self.particles.as_mut(), device, queue);
            self.impacts.update_visuals(
                device,
                queue,
                &self.shared,
                update.view_projection,
                update.lighting,
                update.time_seconds,
            );
        }
        let (placements, particles, impacts) =
            (&mut self.placements, &mut self.particles, &mut self.impacts);
        for placement in placements.iter() {
            if let (Some(particles), Some((device, _))) =
                (particles.as_mut(), update.particle_frame)
            {
                particles.update_placement(
                    device,
                    queue,
                    particle_placement_state(placement, world),
                );
            }
        }
        if let (Some(particles), Some((device, _))) = (particles.as_mut(), update.particle_frame) {
            impacts.update_visual_particle_attachments(particles, device, queue, world);
        }
        update_renderers(
            placements,
            queue,
            update.view_projection,
            update.lighting,
            update.time_seconds,
        );
        if let Some((device, particle_scene)) = update.particle_frame {
            impacts.update_decals(
                device,
                queue,
                update.time_seconds,
                particle_scene.view_projection,
                update.lighting,
            );
        }
        if let (Some(particles), Some((device, particle_scene))) =
            (particles.as_mut(), update.particle_frame)
        {
            particles.finish_frame(
                device,
                queue,
                update.time_seconds,
                particle_scene,
                [
                    update.lighting.dir_light_color[0],
                    update.lighting.dir_light_color[1],
                    update.lighting.dir_light_color[2],
                    1.0,
                ],
            );
        }
    }

    /// Draw all visible placements and their recursive component graphs.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.render_phase(RenderPhase::World, pass);
    }

    /// Draw authored screen-space distortion for all visible placements.
    pub fn render_distortion<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.render_phase(RenderPhase::Distortion, pass);
    }

    /// Draw every visible shadow-enabled component into one cascade.
    pub fn render_shadow<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>, cascade: usize) {
        self.render_phase(RenderPhase::Shadow { cascade }, pass);
    }

    /// Return the number of uploaded visual placements.
    #[must_use]
    pub fn placement_count(&self) -> usize {
        self.placements.len()
    }

    /// Return the number of placements bound to sim entity IDs.
    #[must_use]
    pub fn bound_entity_count(&self) -> usize {
        self.placements.len()
    }

    /// Return the number of live root PFX attachment runtimes.
    #[must_use]
    pub fn live_particle_effect_count(&self) -> usize {
        self.particles
            .as_ref()
            .map_or(0, AttachedParticleRenderer::live_effect_count)
    }

    /// Return diagnostics from the live impact router.
    #[must_use]
    pub fn terrain_impact_issues(&self) -> &[String] {
        self.impacts.issues()
    }

    /// Return the number of live fading TFX impact decals.
    #[must_use]
    pub fn live_impact_decal_count(&self) -> usize {
        self.impacts.live_decal_count()
    }

    /// Return the number of live TFX temporary visual graphs.
    #[must_use]
    pub fn live_impact_visual_count(&self) -> usize {
        self.impacts.live_visual_count()
    }

    /// Return the number of live root `.lgt` attachment runtimes.
    #[must_use]
    pub fn live_light_effect_count(&self) -> usize {
        self.lights.live_effect_count()
    }

    /// Return how many evaluated lights exceeded the shared shader payload.
    #[must_use]
    pub const fn omitted_local_light_count(&self) -> usize {
        self.lights.omitted_light_count()
    }

    /// Return the non-shadowed authored lights routed into the shared 3D field.
    #[must_use]
    pub fn buffered_local_lights(&self) -> &[crate::lighting::LocalLight] {
        self.lights.buffered_lights()
    }

    /// Return selected direct lights that request renderer-owned local shadows.
    #[must_use]
    pub fn local_shadow_requests(&self) -> &[crate::local_shadow::LocalShadowRequest] {
        self.lights.shadow_requests()
    }

    /// Return how many buffered lights exceeded the retail scene-manager limit.
    #[must_use]
    pub const fn omitted_buffered_light_count(&self) -> usize {
        self.lights.omitted_buffered_light_count()
    }

    fn sync_attached_particles(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &UnitScene,
        world: &SimWorld,
    ) {
        let Some(particles) = &mut self.particles else {
            return;
        };
        particles.begin_sync(&scene.particle_assets, &scene.terrain_effect_assets);
        for placement in &self.placements {
            particles.sync_placement(device, queue, particle_placement_state(placement, world));
        }
        particles.end_sync();
    }

    fn sync_attached_lights(&mut self, scene: &UnitScene) {
        self.lights.begin_sync(&scene.light_assets);
        for placement in &self.placements {
            self.lights.sync_placement(light_placement_state(placement));
        }
        self.lights.end_sync();
    }

    fn collect_animation_events(
        &mut self,
        world: &SimWorld,
        events: impl IntoIterator<Item = UnitAnimationEvent>,
    ) {
        let effects = self.animation_effects.route(events);
        self.impacts
            .collect_animation_events(world, effects, self.particles.is_some());
    }
}

fn light_placement_state(placement: &RenderedPlacement) -> LightPlacementState<'_> {
    LightPlacementState {
        owner_id: u64::from(placement.entity_id.as_u32()),
        animation_type: placement
            .renderer
            .active_action_animation_type()
            .or(placement.animation_type.as_deref()),
        movement_animation_type: placement
            .renderer
            .active_movement_animation_type()
            .or(placement.movement_track_animation.as_deref()),
        animation_revision: placement.animation_revision,
        transform: placement.transform,
        visible: placement.visible,
        renderer: &placement.renderer,
    }
}

fn particle_placement_state<'state>(
    placement: &'state RenderedPlacement,
    world: &'state SimWorld,
) -> ParticlePlacementState<'state> {
    ParticlePlacementState {
        owner_id: u64::from(placement.entity_id.as_u32()),
        animation_type: placement
            .renderer
            .active_action_animation_type()
            .or(placement.animation_type.as_deref()),
        movement_animation_type: placement
            .renderer
            .active_movement_animation_type()
            .or(placement.movement_track_animation.as_deref()),
        animation_revision: placement.animation_revision,
        transform: placement.transform,
        secondary_transform: placement.secondary_transform.unwrap_or(placement.transform),
        visible: placement.visible,
        emitter_opacity: placement.visual_opacity,
        renderer: &placement.renderer,
        world,
    }
}

fn same_visual(rendered: &RenderedPlacement, placement: &super::UnitPlacement) -> bool {
    rendered
        .proto_name
        .eq_ignore_ascii_case(placement.proto_name())
        && rendered.visual_variation_index == placement.visual_variation_index()
}

fn renderer_is_reusable(rendered: &RenderedPlacement, placement: &super::UnitPlacement) -> bool {
    same_visual(rendered, placement)
        && rendered.animation_type.as_deref() == placement.animation_type()
        && rendered.animation_asset.as_deref() == placement.animation_asset()
        && rendered.animation_uses_simulation_clock == placement.animation_uses_simulation_clock()
        && rendered.movement_track_animation.as_deref() == placement.movement_track_animation_type()
        && rendered.animation_revision == placement.animation_revision()
        && rendered.visual_mesh_revision == placement.visual_mesh_mask().revision()
}

fn configured_unit_renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    placement: &super::UnitPlacement,
    shared: &Arc<SharedResources>,
) -> UnitRenderer {
    let mut renderer = UnitRenderer::new_with_shared_mesh_mask(
        device,
        queue,
        &placement.unit,
        placement.transform,
        shared,
        Some(placement.visual_mesh_mask()),
    );
    renderer.configure_ik(placement.ik_profile().clone());
    renderer
}

fn update_placement_states(
    placements: &mut [RenderedPlacement],
    queue: &wgpu::Queue,
    world: &SimWorld,
    viewer_team: Option<TeamId>,
    time_seconds: f32,
) -> Vec<UnitAnimationEvent> {
    let mut animation_events = Vec::new();
    for placement in placements {
        let Some(transform) = simulation_entity_transform(world, placement.entity_id) else {
            placement.visible = false;
            continue;
        };
        placement.transform = transform;
        placement.secondary_transform =
            simulation_entity_secondary_transform(world, placement.entity_id);
        placement.visual_opacity =
            simulation_entity_visual_opacity(world, placement.entity_id).unwrap_or(1.0);
        placement.selection = world
            .entity_targeting_selection(placement.entity_id)
            .map_or_else(SelectionOverlay::default, |selection| {
                project_selection(
                    world,
                    selection,
                    transform,
                    placement.visual_bounds_min,
                    placement.visual_bounds_max,
                )
            });
        placement.visible = viewer_team.is_none_or(|team_id| {
            simulation_entity_visible_to_team(world, team_id, placement.entity_id)
        });
        let animation_position = placement.combat_animation_duration.map_or_else(
            || {
                simulation_entity_animation(world, placement.entity_id)
                    .map(|animation| animation.normalized_position(world.game_time()))
            },
            |duration| combat_animation_position(world, placement.entity_id, duration),
        );
        let presentation_phase = placement.animation_clock.sample(
            time_seconds,
            placement.renderer.presentation_animation_duration(),
        );
        let movement_animation = placement
            .movement_track_animation
            .as_deref()
            .or(placement.animation_type.as_deref());
        let events = placement.renderer.update_animation(
            queue,
            UnitAnimationFrame {
                simulation_position: animation_position,
                presentation_phase,
                unit_transform: placement.transform,
                source_owner_id: u64::from(placement.entity_id.as_u32()),
                world,
                entity_id: placement.entity_id,
                movement_animation,
            },
        );
        if placement.visible {
            animation_events.extend(events);
        }
    }
    animation_events
}

/// Project an optional second world matrix for a sim-owned beam visual.
#[must_use]
pub fn simulation_entity_secondary_transform(
    world: &SimWorld,
    entity_id: EntityId,
) -> Option<Mat4> {
    let position = world
        .get_object(entity_id)?
        .visual_secondary_position()
        .filter(|position| position.is_finite())?;
    let mut transform = simulation_entity_transform(world, entity_id)?;
    transform.w_axis = position.extend(1.0);
    Some(transform)
}

fn update_renderers(
    placements: &mut [RenderedPlacement],
    queue: &wgpu::Queue,
    view_projection: Mat4,
    lighting: &LightingParams,
    time_seconds: f32,
) {
    for placement in placements.iter_mut().filter(|placement| placement.visible) {
        placement.renderer.update_frame_with_visual_state_at_time(
            queue,
            view_projection,
            placement.transform,
            lighting,
            time_seconds,
            VisualState::new(placement.selection, placement.visual_opacity),
        );
    }
}

fn project_selection(
    world: &SimWorld,
    selection: TargetingSelection,
    transform: Mat4,
    bounds_min: Vec3,
    bounds_max: Vec3,
) -> SelectionOverlay {
    let (minimum_y, maximum_y) = transformed_y_bounds(transform, bounds_min, bounds_max);
    let extent = maximum_y - minimum_y;
    let scale = if extent.abs() > f32::EPSILON {
        -extent.recip()
    } else {
        1.0
    };
    let base_offset = 2.0 - minimum_y * scale;
    let elapsed_seconds = world
        .game_time()
        .wrapping_sub(selection.started_at_ms())
        .to_f32()
        .unwrap_or(f32::MAX)
        * 0.001;
    let raw_scroll = elapsed_seconds * selection.scroll_speed();
    let scroll = if raw_scroll < -4.0 {
        -((-raw_scroll) % 4.0)
    } else {
        raw_scroll
    };
    let [r, g, b, a] = selection.color();
    SelectionOverlay::new(
        [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
        ],
        scale,
        base_offset + scroll,
        selection.intensity(),
    )
}

fn transformed_y_bounds(transform: Mat4, minimum: Vec3, maximum: Vec3) -> (f32, f32) {
    let mut minimum_y = f32::INFINITY;
    let mut maximum_y = f32::NEG_INFINITY;
    for x in [minimum.x, maximum.x] {
        for y in [minimum.y, maximum.y] {
            for z in [minimum.z, maximum.z] {
                let world_y = transform.transform_point3(Vec3::new(x, y, z)).y;
                minimum_y = minimum_y.min(world_y);
                maximum_y = maximum_y.max(world_y);
            }
        }
    }
    if minimum_y.is_finite() && maximum_y.is_finite() {
        (minimum_y, maximum_y)
    } else {
        (0.0, 0.0)
    }
}

impl WorldRenderer for UnitSceneRenderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        if phase == RenderPhase::Sky {
            return;
        }
        self.impacts.render_phase(phase, pass);
        for placement in self.placements.iter().filter(|placement| placement.visible) {
            placement.renderer.render_phase(phase, pass);
        }
        if let Some(particles) = &self.particles {
            particles.render_phase(phase, pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mat4, Vec3, project_selection};

    #[test]
    fn selection_projection_uses_sim_time_and_decoded_world_bounds() {
        let mut world = sim::World::new();
        let unit_id = world.create_unit(1);
        assert!(world.flash_entity(unit_id, 500, 3_000, [255, 255, 0, 255], 80.0));
        world.game_time_ms = 500;
        let selection = world.entity_targeting_selection(unit_id).unwrap();
        let overlay = project_selection(
            &world,
            selection,
            Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0)),
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );

        assert_eq!(
            overlay.color().map(f32::to_bits),
            [1.0, 1.0, 0.0, 1.0].map(f32::to_bits)
        );
        let params = overlay.params();
        assert_eq!(params[0].to_bits(), (-0.5_f32).to_bits());
        assert_eq!(params[1].to_bits(), 4.5_f32.to_bits());
        assert_eq!(params[2].to_bits(), 80.0_f32.to_bits());
        assert_eq!(params[3].to_bits(), 1.0_f32.to_bits());
    }
}
