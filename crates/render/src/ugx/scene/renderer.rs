//! GPU presentation for unit visuals bound to simulation entities.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;
use sim::{EntityId, TargetingSelection, TeamId, World as SimWorld};

use super::{
    UnitScene, simulation_entity_animation, simulation_entity_transform,
    simulation_entity_visible_to_team,
};
use crate::environment::EnvironmentMap;
use crate::terrain::LightingParams;
use crate::ugx::renderer::{SelectionOverlay, SharedResources};
use crate::ugx::{RendererResources, UnitRenderer, WorldBindings};
use crate::{RenderPhase, WorldRenderer};

struct RenderedPlacement {
    entity_id: EntityId,
    proto_name: String,
    animation_revision: u32,
    transform: Mat4,
    visible: bool,
    visual_bounds_min: Vec3,
    visual_bounds_max: Vec3,
    selection: SelectionOverlay,
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
                animation_revision: placement.animation_revision(),
                transform: placement.transform,
                visible: true,
                visual_bounds_min: Vec3::from_array(placement.unit.bounds_min()),
                visual_bounds_max: Vec3::from_array(placement.unit.bounds_max()),
                selection: SelectionOverlay::default(),
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
                    && rendered.animation_revision == placement.animation_revision()
                {
                    rendered.transform = placement.transform;
                    rendered.visible = true;
                    return rendered;
                }
                RenderedPlacement {
                    entity_id: placement.entity_id(),
                    proto_name: placement.proto_name().to_owned(),
                    animation_revision: placement.animation_revision(),
                    transform: placement.transform,
                    visible: true,
                    visual_bounds_min: Vec3::from_array(placement.unit.bounds_min()),
                    visual_bounds_max: Vec3::from_array(placement.unit.bounds_max()),
                    selection: SelectionOverlay::default(),
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
            None,
            view_projection,
            lighting,
            time_seconds,
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
            Some(team_id),
            view_projection,
            lighting,
            time_seconds,
        );
    }

    fn update_from_world_projection(
        &mut self,
        queue: &wgpu::Queue,
        world: &SimWorld,
        viewer_team: Option<TeamId>,
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
            let animation_position = simulation_entity_animation(world, placement.entity_id)
                .map(|animation| animation.normalized_position(world.game_time()));
            placement
                .renderer
                .update_scripted_animation(queue, animation_position);
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
        placement.renderer.update_frame_with_selection_at_time(
            queue,
            view_projection,
            placement.transform,
            lighting,
            time_seconds,
            placement.selection,
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
        for placement in self.placements.iter().filter(|placement| placement.visible) {
            placement.renderer.render_phase(phase, pass);
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
