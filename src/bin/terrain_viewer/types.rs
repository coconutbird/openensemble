//! Data types for the terrain viewer.
//!
//! This module contains GPU-specific types that remain in the viewer,
//! and re-exports terrain types from the `render` crate.

use render::wgpu;

// Re-export terrain types from render crate
pub use render::terrain::{AlbedoData, FoliageQNChunk, FoliageSet, RawXtdData};

pub const TERRAIN_CHUNKS_PER_AXIS: usize = 16;

/// Converts the XTT linker's texture-grid axes into terrain world axes.
///
/// XTT `grid_x` advances along world Z, while XTT `grid_z` advances along
/// world X. This is the same diagonal transpose used when placing linker data
/// in the unique terrain atlas.
pub fn terrain_world_chunk_coords(grid_x: i32, grid_z: i32) -> Option<(u32, u32)> {
    let world_x = u32::try_from(grid_z).ok()?;
    let world_z = u32::try_from(grid_x).ok()?;
    let chunk_limit = u32::try_from(TERRAIN_CHUNKS_PER_AXIS).ok()?;
    (world_x < chunk_limit && world_z < chunk_limit).then_some((world_x, world_z))
}

/// Converts XTT grid coordinates to the unique-atlas slot sampled by terrain.
///
/// The PC terrain shaders transpose XTT's grid axes: XTT `grid_z` advances
/// across the unique atlas, while XTT `grid_x` advances down it.
pub fn terrain_chunk_index(grid_x: i32, grid_z: i32) -> Option<usize> {
    let (world_x, world_z) = terrain_world_chunk_coords(grid_x, grid_z)?;
    let world_x = usize::try_from(world_x).ok()?;
    let world_z = usize::try_from(world_z).ok()?;
    world_z
        .checked_mul(TERRAIN_CHUNKS_PER_AXIS)?
        .checked_add(world_x)
}

/// GPU resources for terrain rendering.
pub struct GpuResources {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_bind_group: wgpu::BindGroup,
    pub texture_bind_group: wgpu::BindGroup,
    /// Accepted-axis packed position texture shared with terrain-conform UGX.
    pub position_texture_view: wgpu::TextureView,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub params_buffer: wgpu::Buffer,
    pub lighting_buffer: Option<wgpu::Buffer>,
    /// Shared oracle-packed local-light storage used by every world renderer.
    pub local_lights: render::lighting::LocalLightBuffer,
    pub terrain_size: [f32; 2],
    pub tile_scale: f32,
    /// Number of patch instances to draw (64x64 = 4096).
    pub num_patch_instances: u32,
}

#[cfg(test)]
mod tests {
    use super::{terrain_chunk_index, terrain_world_chunk_coords};

    #[test]
    fn xtt_chunk_axes_are_transposed_into_world_axes() {
        assert_eq!(terrain_world_chunk_coords(2, 3), Some((3, 2)));
        assert_eq!(terrain_world_chunk_coords(-1, 0), None);
        assert_eq!(terrain_world_chunk_coords(0, 16), None);
    }

    #[test]
    fn terrain_chunk_indices_follow_transposed_shader_axes() {
        assert_eq!(terrain_chunk_index(0, 0), Some(0));
        assert_eq!(terrain_chunk_index(0, 1), Some(1));
        assert_eq!(terrain_chunk_index(1, 0), Some(16));
        assert_eq!(terrain_chunk_index(15, 15), Some(255));
        assert_eq!(terrain_chunk_index(16, 0), None);
        assert_eq!(terrain_chunk_index(0, -1), None);
    }
}
