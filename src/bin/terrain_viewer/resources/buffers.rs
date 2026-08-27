//! Buffer creation for terrain rendering.
//!
//! Creates GPU storage and uniform buffers for chunk layer data and texture scales.

use num_traits::ToPrimitive;
use render::wgpu;

use crate::types::terrain_chunk_index;
use crate::viewer::TerrainViewer;

impl TerrainViewer {
    pub(crate) fn create_chunk_layers_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        let scene = self.scene.as_ref().expect("scene must be loaded");
        let chunk_splat_data = &scene.chunk_splat_data;

        // 256 chunks * 8 layers = 2048 u32s
        let mut layer_data = vec![0u32; 256 * 8];

        for chunk in chunk_splat_data {
            // XTT axes are transposed into the unique atlas by the PC shaders.
            let Some(chunk_idx) = terrain_chunk_index(chunk.grid_x, chunk.grid_z) else {
                continue;
            };
            let base = chunk_idx * 8;

            for (i, &layer_id) in chunk.layer_texture_ids.iter().enumerate().take(8) {
                layer_data[base + i] = layer_id.cast_unsigned();
            }
        }

        log::info!(
            "Creating chunk layers buffer: {} chunks",
            chunk_splat_data.len()
        );

        // Log first few chunks for debugging
        log::info!("=== First 5 chunk layer IDs in buffer ===");
        for chunk in chunk_splat_data.iter().take(5) {
            let Some(chunk_idx) = terrain_chunk_index(chunk.grid_x, chunk.grid_z) else {
                continue;
            };
            log::info!(
                "  Chunk ({}, {}) idx={}: layers={:?}",
                chunk.grid_x,
                chunk.grid_z,
                chunk_idx,
                chunk.layer_texture_ids
            );
        }

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Chunk Layers Buffer"),
            contents: bytemuck::cast_slice(&layer_data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    /// Creates the texture scales storage buffer (per-texture `u_scale/v_scale` values).
    /// The game uses these to control how many times each texture tiles across the terrain.
    pub(crate) fn create_texture_scales_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;

        // Each texture has a vec2<f32> with (u_scale, v_scale)
        // Must cover ALL active textures since splat_layer_ids can reference any index.
        // The original game stores these in g_LayerData[i].yz per-layer, but we store
        // them per-texture and look up by texture index in the shader.
        let scene = self.scene.as_ref().expect("scene must be loaded");
        let terrain_textures = &scene.terrain_textures;
        let num_textures = terrain_textures.len().max(1);
        let mut scale_data = vec![1.0f32; num_textures * 2]; // Default scale of 1.0

        for (i, tex) in terrain_textures.iter().enumerate() {
            // Game stores scale as i32, but shader needs f32
            // Scale values are typically 1, 2, 4, etc.
            scale_data[i * 2] = tex.u_scale.to_f32().unwrap_or_default();
            scale_data[i * 2 + 1] = tex.v_scale.to_f32().unwrap_or_default();
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

    /// Creates the chunk decal layers buffer.
    /// 256 chunks × 8 decal layer slots = 2048 u32s.
    /// Each entry is the decal instance index for that chunk's decal layer.
    pub(crate) fn create_chunk_decal_layers_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        let scene = self.scene.as_ref().expect("scene must be loaded");

        // 256 chunks * 8 decal layers = 2048 u32s
        let mut layer_data = vec![0u32; 256 * 8];

        for chunk in &scene.chunk_decal_data {
            let Some(chunk_idx) = terrain_chunk_index(chunk.grid_x, chunk.grid_z) else {
                continue;
            };
            if chunk_idx >= 256 {
                continue;
            }
            let base = chunk_idx * 8;
            for (i, &decal_id) in chunk.decal_layer_ids.iter().enumerate().take(8) {
                layer_data[base + i] = decal_id.cast_unsigned();
            }
        }

        log::info!(
            "Creating chunk decal layers buffer: {} chunks with decals",
            scene.chunk_decal_data.len()
        );

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Chunk Decal Layers Buffer"),
            contents: bytemuck::cast_slice(&layer_data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    /// Creates the decal instances buffer.
    /// Each instance is a `vec4<f32>`: rotation, center U/V, and bitcast decal index.
    pub(crate) fn create_decal_instances_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        let scene = self.scene.as_ref().expect("scene must be loaded");

        // At least 1 entry to avoid zero-sized buffer
        let num = scene.decal_instances.len().max(1);
        let mut data = vec![[0.0f32; 4]; num];

        for (i, inst) in scene.decal_instances.iter().enumerate() {
            data[i] = [
                inst.rotation,
                inst.tile_center_x,
                inst.tile_center_y,
                f32::from_bits(inst.decal_index.cast_unsigned()),
            ];
        }

        log::info!(
            "Creating decal instances buffer: {} instances",
            scene.decal_instances.len()
        );

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Decal Instances Buffer"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }

    /// Creates the decal UV scales buffer.
    /// Each entry is a `vec2<f32>`: (`u_scale`, `v_scale`) per decal instance.
    pub(crate) fn create_decal_uv_scales_buffer(&self, device: &wgpu::Device) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        let scene = self.scene.as_ref().expect("scene must be loaded");

        let num = scene.decal_instances.len().max(1);
        let mut data = vec![[1.0f32; 2]; num];

        for (i, inst) in scene.decal_instances.iter().enumerate() {
            data[i] = [inst.u_scale, inst.v_scale];
        }

        log::info!(
            "Creating decal UV scales buffer: {} entries",
            scene.decal_instances.len()
        );

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Decal UV Scales Buffer"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE,
        })
    }
}
