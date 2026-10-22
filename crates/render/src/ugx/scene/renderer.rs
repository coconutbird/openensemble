//! GPU presentation for unit visuals bound to simulation entities.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use sim::{EntityId, TeamId, World as SimWorld};

use super::{
    UnitScene,
    lights::{AttachedLightRenderer, LightPlacementState},
    movement_animations::MovementAnimation,
    particles::{AttachedParticleRenderer, ParticlePlacementState},
    simulation_entity_transform,
};
mod animation_effects;
mod impacts;
mod placement_state;

use crate::environment::EnvironmentMap;
use crate::lighting::{LocalLightSet, LocalLightView};
use crate::particle::{ParticleScene, ParticleSceneTextures};
use crate::terrain::{LightingParams, TerrainPatchWorldBindings};
use crate::ugx::renderer::{SelectionOverlay, SharedResources};
use crate::ugx::unit::UnitAnimationEvent;
use crate::ugx::{RendererResources, UnitRenderer, WorldBindings};
use crate::{RenderPhase, WorldRenderer};
use animation_clock::PresentationAnimationClock;
use animation_effects::AnimationEffects;
use impacts::ImpactRuntime;
use placement_state::{update_placement_states, update_renderers};
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
    movement_animation: Option<MovementAnimation>,
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
    movement_animation_clock: PresentationAnimationClock,
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
                movement_animation: placement.movement_animation,
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
                movement_animation_clock: PresentationAnimationClock::default(),
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
                let fresh_clock = PresentationAnimationClock::default();
                let (animation_clock, mut movement_animation_clock, previous_movement, complete) =
                    previous_placement
                        .as_ref()
                        .filter(|rendered| same_visual(rendered, placement))
                        .map_or((fresh_clock, fresh_clock, None, true), |rendered| {
                            (
                                rendered.animation_clock,
                                rendered.movement_animation_clock,
                                rendered.movement_animation,
                                rendered.renderer.ik_idle_transition_complete(),
                            )
                        });
                movement_animation_clock.rebase_movement(
                    previous_movement,
                    placement.movement_animation,
                    complete,
                );
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
                    rendered.movement_animation = placement.movement_animation;
                    rendered.movement_track_animation =
                        placement.movement_track_animation_type().map(str::to_owned);
                    rendered.movement_animation_clock = movement_animation_clock;
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
                    movement_animation: placement.movement_animation,
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
                    movement_animation_clock,
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
        self.refresh_placements(queue, world, None, time_seconds);
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
    ///
    /// Call [`Self::update_attached_lights_at_time`] first in the same frame with the same world
    /// and time. That call samples the animations. This call presents its result.
    pub fn present_with_particles_at_time(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        world: &SimWorld,
        lighting: &LightingParams,
        time_seconds: f32,
        particle_scene: &ParticleScene,
    ) {
        self.present_frame(
            queue,
            world,
            &FrameUpdate {
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
        self.refresh_placements(queue, world, update.viewer_team, update.time_seconds);
        self.present_frame(queue, world, &update);
    }

    /// Moves the placements to the sim state and samples their animations at `time_seconds`.
    fn refresh_placements(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        viewer_team: Option<TeamId>,
        time_seconds: f32,
    ) {
        self.impacts.advance_animation_anchors(time_seconds);
        self.impacts.collect(world, self.particles.is_some());
        let animation_events = update_placement_states(
            &mut self.placements,
            queue,
            world,
            viewer_team,
            time_seconds,
        );
        self.collect_animation_events(world, animation_events);
    }

    fn present_frame(&mut self, queue: &wgpu::Queue, world: &SimWorld, update: &FrameUpdate<'_>) {
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
