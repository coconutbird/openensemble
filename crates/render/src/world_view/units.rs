//! Creation of the sky and unit renderers of the `WorldView`.

use super::WorldViewUnits;
use super::shadow::ShadowResources;
use super::terrain::TerrainResources;
use crate::environment::EnvironmentMap;
use crate::particle::ParticleSceneTextures;
use crate::terrain::{
    NORMALIZED_TERRAIN_Y_OFFSET, RawXtdData, TerrainHeightfield, TerrainPatchWorldBindings,
};
use crate::ugx::{RendererResources, Unit, UnitRenderer, UnitSceneRenderer, WorldBindings};
use crate::wgpu;

/// The inputs of the sky and unit renderers.
pub(super) struct UnitRendererInputs<'a> {
    pub(super) terrain: &'a TerrainResources,
    pub(super) shadow: &'a ShadowResources,
    pub(super) raw_data: &'a RawXtdData,
    pub(super) environment: Option<&'a EnvironmentMap>,
    pub(super) sky: Option<&'a Unit>,
    pub(super) units: Option<WorldViewUnits<'a>>,
    pub(super) scene_format: wgpu::TextureFormat,
}

/// The sky renderer and the unit scene renderer.
pub(super) struct UnitRenderers {
    pub(super) sky: Option<UnitRenderer>,
    pub(super) units: Option<UnitSceneRenderer>,
}

fn terrain_heightfield<'a>(
    terrain: &'a TerrainResources,
    raw_data: &RawXtdData,
) -> TerrainHeightfield<'a> {
    TerrainHeightfield {
        view: &terrain.position_texture_view,
        dimension: raw_data.num_verts_per_axis,
        tile_scale: raw_data.tile_scale,
        world_min_xz: [raw_data.world_min[2], raw_data.world_min[0]],
        y_range: raw_data.range[1],
        y_mid: raw_data.mid[1],
        normalized_y_bias: NORMALIZED_TERRAIN_Y_OFFSET,
    }
}

/// Returns the scenario resources that terrain patches bind: the heightfield for the terrain
/// conformance, the terrain alpha, the shadows, the fog of war and the local lights.
pub(super) fn patch_world_bindings<'a>(
    terrain: &'a TerrainResources,
    shadow: &'a ShadowResources,
    raw_data: &RawXtdData,
) -> TerrainPatchWorldBindings<'a> {
    TerrainPatchWorldBindings {
        heightfield: Some(terrain_heightfield(terrain, raw_data)),
        terrain_alpha: Some(&terrain.terrain_alpha_view),
        directional_shadow: Some(&shadow.shadow_view),
        blackmap: Some(&terrain.blackmap_view),
        unexplored: Some(&terrain.unexplored_view),
        local_lights: Some(&terrain.local_lights),
        local_shadow: Some(terrain.local_shadows.view()),
        light_volume_color: Some(terrain.light_volume.color_view()),
        light_volume_vector: Some(terrain.light_volume.vector_view()),
    }
}

/// Creates the shared UGX resources, then the sky renderer and the unit scene renderer.
pub(super) fn create_unit_renderers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    inputs: &UnitRendererInputs<'_>,
) -> UnitRenderers {
    let terrain = inputs.terrain;
    let world = WorldBindings {
        environment: inputs.environment,
        directional_shadow: Some(&inputs.shadow.shadow_view),
        terrain_heightfield: Some(terrain_heightfield(terrain, inputs.raw_data)),
        local_lights: Some(&terrain.local_lights),
        local_shadows: Some(&terrain.local_shadows),
        light_volume_color: Some(terrain.light_volume.color_view()),
        light_volume_vector: Some(terrain.light_volume.vector_view()),
    };
    let ugx_resources =
        RendererResources::new_with_world(device, queue, inputs.scene_format, world);
    let sky = inputs.sky.map(|sky| {
        UnitRenderer::new_with_resources(device, queue, sky, glam::Mat4::IDENTITY, &ugx_resources)
    });
    let units = inputs
        .units
        .map(|units| create_unit_scene_renderer(device, queue, inputs, units, &ugx_resources));
    UnitRenderers { sky, units }
}

fn create_unit_scene_renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    inputs: &UnitRendererInputs<'_>,
    units: WorldViewUnits<'_>,
    ugx_resources: &RendererResources,
) -> UnitSceneRenderer {
    let terrain = inputs.terrain;
    let mut renderer =
        UnitSceneRenderer::new_with_resources(device, queue, units.scene, ugx_resources);
    renderer.enable_attached_particles(
        device,
        queue,
        inputs.scene_format,
        ParticleSceneTextures {
            depth: &terrain.depth_view,
            light_volume: Some(terrain.light_volume.color_view()),
        },
        units.scene,
        units.world,
    );
    renderer.enable_impact_decals(
        device,
        queue,
        inputs.scene_format,
        Some(wgpu::TextureFormat::Depth32Float),
        patch_world_bindings(terrain, inputs.shadow, inputs.raw_data),
        units.scene,
    );
    log::info!(
        "Uploaded {} simulation-backed UGX placements with {} live PFX and {} live LGT graphs",
        renderer.placement_count(),
        renderer.live_particle_effect_count(),
        renderer.live_light_effect_count(),
    );
    renderer
}
