//! GPU presentation for unit visuals bound to simulation entities.

use std::collections::HashMap;
use std::sync::Arc;

use glam::Mat4;
use sim::{EntityId, World as SimWorld};

use super::{UnitScene, simulation_entity_transform};
use crate::environment::EnvironmentMap;
use crate::terrain::LightingParams;
use crate::ugx::renderer::SharedResources;
use crate::ugx::{RendererResources, UnitRenderer, WorldBindings};
use crate::{RenderPhase, WorldRenderer};

struct RenderedPlacement {
    entity_id: EntityId,
    proto_name: String,
    transform: Mat4,
    visible: bool,
    renderer: UnitRenderer,
}

/// GPU resources for every successfully decoded unit placement.
///
/// Simulation-bound placements fetch position and facing from [`SimWorld`]
/// each frame. This type owns only GPU/UI state; it never advances gameplay.
pub struct UnitSceneRenderer {
    placements: Vec<RenderedPlacement>,
    shared: Arc<SharedResources>,
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
                transform: placement.transform,
                visible: true,
                renderer: UnitRenderer::new_with_shared(
                    device,
                    queue,
                    &placement.unit,
                    placement.transform,
                    &resources.shared,
                ),
            })
            .collect();
        Self {
            placements,
            shared: Arc::clone(&resources.shared),
        }
    }

    /// Synchronize uploaded instances with a refreshed simulation scene.
    ///
    /// Existing entity/prototype pairs retain their GPU resources. New entities
    /// receive an instance renderer and removed entities are dropped.
    pub fn sync_scene(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &UnitScene) {
        let mut previous = self
            .placements
            .drain(..)
            .map(|placement| (placement.entity_id, placement))
            .collect::<HashMap<_, _>>();
        self.placements = scene
            .placements
            .iter()
            .map(|placement| {
                if let Some(mut rendered) = previous.remove(&placement.entity_id())
                    && rendered
                        .proto_name
                        .eq_ignore_ascii_case(placement.proto_name())
                {
                    rendered.transform = placement.transform;
                    rendered.visible = true;
                    return rendered;
                }
                RenderedPlacement {
                    entity_id: placement.entity_id(),
                    proto_name: placement.proto_name().to_owned(),
                    transform: placement.transform,
                    visible: true,
                    renderer: UnitRenderer::new_with_shared(
                        device,
                        queue,
                        &placement.unit,
                        placement.transform,
                        &self.shared,
                    ),
                }
            })
            .collect();
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

    /// Present sim state with animated material UVs at the supplied render time.
    pub fn update_from_world_at_time(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        view_projection: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
    ) {
        for placement in &mut self.placements {
            let Some(transform) = simulation_entity_transform(world, placement.entity_id) else {
                placement.visible = false;
                continue;
            };
            placement.transform = transform;
            placement.visible = true;
        }
        update_renderers(
            &mut self.placements,
            queue,
            view_projection,
            lighting,
            time_seconds,
        );
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
}

fn update_renderers(
    placements: &mut [RenderedPlacement],
    queue: &wgpu::Queue,
    view_projection: Mat4,
    lighting: &LightingParams,
    time_seconds: f32,
) {
    for placement in placements.iter_mut().filter(|placement| placement.visible) {
        placement.renderer.update_frame_at_time(
            queue,
            view_projection,
            placement.transform,
            lighting,
            time_seconds,
        );
    }
}

impl WorldRenderer for UnitSceneRenderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        if phase == RenderPhase::Sky {
            return;
        }
        for placement in self.placements.iter().filter(|placement| placement.visible) {
            placement.renderer.render_phase(phase, pass);
        }
    }
}
