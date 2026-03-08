//! Buffer creation for terrain rendering.
//!
//! Creates GPU storage and uniform buffers for chunk layer data and texture scales.

use render::wgpu;

use crate::viewer::TerrainViewer;

impl TerrainViewer {
    pub(crate) fn create_chunk_layers_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;

        // 256 chunks * 8 layers = 2048 u32s
        let mut layer_data = vec![0u32; 256 * 8];

        for chunk in &self.chunk_splat_data {
            let chunk_idx = (chunk.grid_z * 16 + chunk.grid_x) as usize;
            let base = chunk_idx * 8;

            for (i, &layer_id) in chunk.layer_texture_ids.iter().enumerate().take(8) {
                layer_data[base + i] = layer_id as u32;
            }
        }

        log::info!(
            "Creating chunk layers buffer: {} chunks",
            self.chunk_splat_data.len()
        );
        // Log first few chunks for debugging
        log::info!("=== First 5 chunk layer IDs in buffer ===");
        for chunk in self.chunk_splat_data.iter().take(5) {
            let chunk_idx = (chunk.grid_z * 16 + chunk.grid_x) as usize;
            log::info!(
                "  Chunk ({}, {}) idx={}: layers={:?}",
                chunk.grid_x,
                chunk.grid_z,
                chunk_idx,
                &chunk.layer_texture_ids
            );
        }

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Chunk Layers Buffer"),
            contents: bytemuck::cast_slice(&layer_data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    /// Creates the texture scales storage buffer (per-texture u_scale/v_scale values).
    /// The game uses these to control how many times each texture tiles across the terrain.
    pub(crate) fn create_texture_scales_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;

        // Each texture has a vec2<f32> with (u_scale, v_scale)
        // Must cover ALL active textures since splat_layer_ids can reference any index.
        // The original game stores these in g_LayerData[i].yz per-layer, but we store
        // them per-texture and look up by texture index in the shader.
        let num_textures = self.terrain_textures.len().max(1);
        let mut scale_data = vec![1.0f32; num_textures * 2]; // Default scale of 1.0

        for (i, tex) in self.terrain_textures.iter().enumerate() {
            // Game stores scale as i32, but shader needs f32
            // Scale values are typically 1, 2, 4, etc.
            scale_data[i * 2] = tex.u_scale as f32;
            scale_data[i * 2 + 1] = tex.v_scale as f32;
            log::info!(
                "Texture[{}] {} scale: ({}, {})",
                i,
                tex.name,
                tex.u_scale,
                tex.v_scale
            );
        }

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Texture Scales Buffer"),
            contents: bytemuck::cast_slice(&scale_data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }
}
