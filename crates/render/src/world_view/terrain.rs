//! GPU resources of the tessellated terrain heightfield.
//!
//! The submodules make the textures, buffers and pipelines of the terrain. `TerrainBuild`
//! collects them, together with the resources that share the terrain textures: the compositor,
//! the shadows, the foliage and the roads.

mod buffers;
mod pipelines;
mod textures;

use num_traits::ToPrimitive;
use pipeline::source::{AssetSource, StdFileProvider};

use super::foliage::FoliageResources;
use super::roads::RoadResources;
use super::shadow::ShadowResources;
use crate::terrain::{CompositorResources, TerrainScene};
use crate::{RenderPhase, WorldRenderer, wgpu};

/// The inputs and the results of one terrain GPU resource build.
pub(super) struct TerrainBuild<'a> {
    /// Decoded terrain data.
    pub(super) scene: &'a TerrainScene,
    /// Linear color format of the scene pipelines.
    pub(super) scene_format: wgpu::TextureFormat,
    /// Initial terrain debug mode.
    pub(super) debug_mode: u32,
    /// Initial normal map strength.
    pub(super) bump_power: f32,
    /// Asset source for the road textures.
    pub(super) asset_source: Option<&'a mut AssetSource<StdFileProvider>>,
    pub(super) compositor: Option<CompositorResources>,
    pub(super) compositor_bind_group: Option<wgpu::BindGroup>,
    pub(super) chunk_centers: Vec<[f32; 3]>,
    pub(super) gpu: Option<TerrainResources>,
    pub(super) shadow_resources: Option<ShadowResources>,
    pub(super) foliage_resources: Option<FoliageResources>,
    pub(super) road_resources: Option<RoadResources>,
}

const TERRAIN_CELLS_PER_CHUNK: u32 = 64;

/// Decoded dimensions of the XTT chunk grid covering a terrain.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct TerrainChunkGrid {
    width: u32,
    height: u32,
}

impl TerrainChunkGrid {
    /// Derives the square chunk grid from the packed XTD terrain dimension.
    pub(crate) fn from_terrain_dimension(terrain_dimension: u32) -> Option<Self> {
        if terrain_dimension == 0 || !terrain_dimension.is_multiple_of(TERRAIN_CELLS_PER_CHUNK) {
            return None;
        }
        let chunks_per_axis = terrain_dimension / TERRAIN_CELLS_PER_CHUNK;
        Some(Self {
            width: chunks_per_axis,
            height: chunks_per_axis,
        })
    }

    pub(crate) fn width(self) -> u32 {
        self.width
    }

    pub(crate) fn height(self) -> u32 {
        self.height
    }

    pub(crate) fn total_chunks_u32(self) -> u32 {
        self.width
            .checked_mul(self.height)
            .expect("terrain chunk count must fit u32")
    }

    pub(crate) fn total_chunks(self) -> usize {
        usize::try_from(self.total_chunks_u32()).expect("terrain chunk count must fit usize")
    }

    pub(crate) fn dimensions_f32(self) -> [f32; 2] {
        [
            self.width.to_f32().expect("chunk grid width must fit f32"),
            self.height
                .to_f32()
                .expect("chunk grid height must fit f32"),
        ]
    }

    /// Checks the XTT linker coordinates against the chunk grid.
    ///
    /// XTT `grid_x` advances along world X and XTT `grid_z` advances along world Z.
    pub(crate) fn world_chunk_coords(self, grid_x: i32, grid_z: i32) -> Option<(u32, u32)> {
        let world_x = u32::try_from(grid_x).ok()?;
        let world_z = u32::try_from(grid_z).ok()?;
        (world_x < self.width && world_z < self.height).then_some((world_x, world_z))
    }

    /// Converts XTT coordinates to the unique-atlas slot sampled by terrain.
    ///
    /// The atlas is row-major with rows along world Z, so the slot is
    /// `grid_z * width + grid_x`.
    pub(crate) fn chunk_index(self, grid_x: i32, grid_z: i32) -> Option<usize> {
        let (world_x, world_z) = self.world_chunk_coords(grid_x, grid_z)?;
        let index = world_z.checked_mul(self.width)?.checked_add(world_x)?;
        usize::try_from(index).ok()
    }
}

/// GPU resources for terrain rendering.
pub(super) struct TerrainResources {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_bind_group: wgpu::BindGroup,
    pub texture_bind_group: wgpu::BindGroup,
    /// Packed position texture in native X-major order, shared with terrain-conform UGX.
    pub position_texture_view: wgpu::TextureView,
    /// Dynamic packed terrain visibility mask shared with conforming visuals.
    pub terrain_alpha_view: wgpu::TextureView,
    /// CPU-backed GPU mask updated by crossed `TerrainAlpha` animation tags.
    pub(crate) dynamic_terrain_alpha: super::dynamic_alpha::DynamicTerrainAlphaTexture,
    /// Fog-of-war visibility texture shared with lit decals.
    pub blackmap_view: wgpu::TextureView,
    /// Unexplored-color texture shared with lit decals.
    pub unexplored_view: wgpu::TextureView,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub params_buffer: wgpu::Buffer,
    pub lighting_buffer: Option<wgpu::Buffer>,
    /// Shared oracle-packed local-light storage used by every world renderer.
    pub local_lights: crate::lighting::LocalLightBuffer,
    /// Renderer-owned local spot/omni shadow atlas.
    pub local_shadows: crate::local_shadow::LocalShadowMap,
    /// Shared retail-style 3D field for non-shadowed buffered local lights.
    pub light_volume: crate::light_volume::LocalLightVolume,
    pub terrain_size: [f32; 2],
    pub(crate) chunk_grid: TerrainChunkGrid,
    pub tile_scale: f32,
    /// Number of patch instances to draw (64x64 = 4096).
    pub num_patch_instances: u32,
}

impl WorldRenderer for TerrainResources {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        if phase != RenderPhase::World {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, &self.texture_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.index_buffer.slice(..));
        pass.draw(0..self.index_count, 0..self.num_patch_instances);
    }
}

#[cfg(test)]
mod tests {
    use super::TerrainChunkGrid;

    #[test]
    fn xtt_chunk_axes_are_world_axes() {
        let grid = TerrainChunkGrid::from_terrain_dimension(1024).expect("valid grid");
        assert_eq!(grid.world_chunk_coords(2, 3), Some((2, 3)));
        assert_eq!(grid.world_chunk_coords(-1, 0), None);
        assert_eq!(grid.world_chunk_coords(16, 0), None);
    }

    #[test]
    fn terrain_chunk_indices_are_rows_along_world_z() {
        let grid = TerrainChunkGrid::from_terrain_dimension(1024).expect("valid grid");
        assert_eq!(grid.chunk_index(0, 0), Some(0));
        assert_eq!(grid.chunk_index(1, 0), Some(1));
        assert_eq!(grid.chunk_index(0, 1), Some(16));
        assert_eq!(grid.chunk_index(15, 15), Some(255));
        assert_eq!(grid.chunk_index(16, 0), None);
        assert_eq!(grid.chunk_index(0, -1), None);
    }

    #[test]
    fn tundra_uses_its_decoded_fourteen_chunk_row_stride() {
        let grid = TerrainChunkGrid::from_terrain_dimension(896).expect("valid grid");
        assert_eq!(grid.width(), 14);
        assert_eq!(grid.height(), 14);
        assert_eq!(grid.total_chunks(), 196);
        assert_eq!(grid.chunk_index(0, 1), Some(14));
        assert_eq!(grid.chunk_index(13, 13), Some(195));
        assert_eq!(grid.chunk_index(14, 0), None);
    }

    #[test]
    fn terrain_dimension_must_describe_whole_chunks() {
        assert_eq!(TerrainChunkGrid::from_terrain_dimension(0), None);
        assert_eq!(TerrainChunkGrid::from_terrain_dimension(895), None);
    }
}
