//! 3D rendering for the terrain viewer.
//!
//! Implements the `Application3D` trait: GPU initialization, resize, and render pass.

use render::terrain::{LightingParams, TerrainParams, TessellationMode};
use render::{Application3D, RenderContext, wgpu};

use super::TerrainViewer;
use crate::gpu::create_depth_texture;
use crate::types::RawXtdData;

impl Application3D for TerrainViewer {
    fn init_gpu(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        self.surface_format = format;

        // We'll initialize GPU resources after terrain is loaded
        // This is called before init(), so terrain isn't loaded yet
    }

    fn resize_gpu(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        // Recreate depth texture
        if let Some(gpu) = &mut self.gpu {
            let (depth_texture, depth_view) = create_depth_texture(device, width, height);
            gpu.depth_texture = depth_texture;
            gpu.depth_view = depth_view;
        }
    }

    fn render_3d(&mut self, ctx: &mut RenderContext) {
        // Create GPU resources if not yet created and terrain is loaded
        if self.gpu.is_none() && self.terrain.is_some() {
            // Check if we should use GPU tessellation
            if self.tessellation_mode == TessellationMode::Gpu {
                if let Some(raw_data) = &self.raw_xtd_data {
                    let raw_data_clone = RawXtdData {
                        packed_positions: raw_data.packed_positions.clone(),
                        packed_normals: raw_data.packed_normals.clone(),
                        num_verts_per_axis: raw_data.num_verts_per_axis,
                        mid: raw_data.mid,
                        range: raw_data.range,
                        tile_scale: raw_data.tile_scale,
                        ao_data: raw_data.ao_data.clone(),
                        alpha_data: raw_data.alpha_data.clone(),
                    };
                    let albedo = self.albedo.take();
                    self.create_gpu_tessellation_resources(
                        ctx.device,
                        ctx.queue,
                        &raw_data_clone,
                        albedo,
                        ctx.size.0,
                        ctx.size.1,
                    );
                } else {
                    // Fallback to regular rendering if no raw data
                    log::warn!(
                        "GPU tessellation requested but no raw XTD data available, falling back to regular rendering"
                    );
                    let terrain = self.terrain.as_ref().unwrap();
                    let positions = terrain.positions.clone();
                    let normals = terrain.normals.clone();
                    let uvs = terrain.uvs.clone();
                    let indices = terrain.indices.clone();
                    let albedo = self.albedo.take();
                    self.create_gpu_resources_from_data(
                        ctx.device, ctx.queue, &positions, &normals, &uvs, &indices, albedo,
                        ctx.size.0, ctx.size.1,
                    );
                }
            } else {
                // Regular rendering (no tessellation or CPU tessellation)
                let terrain = self.terrain.as_ref().unwrap();
                let positions = terrain.positions.clone();
                let normals = terrain.normals.clone();
                let uvs = terrain.uvs.clone();
                let indices = terrain.indices.clone();
                let albedo = self.albedo.take();
                self.create_gpu_resources_from_data(
                    ctx.device, ctx.queue, &positions, &normals, &uvs, &indices, albedo,
                    ctx.size.0, ctx.size.1,
                );
            }
        }

        let Some(gpu) = &self.gpu else {
            return;
        };

        // Update camera uniform
        let aspect = ctx.size.0 as f32 / ctx.size.1 as f32;
        let view = self.camera.view_matrix();
        let proj = self.camera.projection_matrix(aspect);
        let view_proj = proj * view;
        ctx.queue.write_buffer(
            &gpu.camera_buffer,
            0,
            bytemuck::cast_slice(&view_proj.to_cols_array()),
        );

        // Update terrain params (for debug mode and bump power changes)
        let params = TerrainParams {
            terrain_size: gpu.terrain_size,
            chunk_count: [16.0, 16.0],
            texture_tile_scale: gpu.tile_scale,
            debug_mode: self.debug_mode as f32,
            bump_power: self.bump_power,
            _padding: 0.0,
        };
        ctx.queue
            .write_buffer(&gpu.params_buffer, 0, bytemuck::bytes_of(&params));

        // Compute shadow VP and update lighting params
        let light_dir = glam::Vec3::new(0.4, 0.8, 0.3).normalize();
        let mut shadow_vp_cols = [[1.0f32, 0.0, 0.0, 0.0]; 4];
        let mut shadow_enabled = 0.0f32;

        if let Some(shadow) = &mut self.shadow_resources {
            let terrain_center = glam::Vec3::new(
                gpu.terrain_size[0] * 0.5,
                50.0, // approximate center height
                gpu.terrain_size[1] * 0.5,
            );
            let terrain_size = glam::Vec3::new(gpu.terrain_size[0], 100.0, gpu.terrain_size[1]);
            let vp = shadow.compute_light_vp(light_dir, terrain_center, terrain_size);
            shadow.update_light_vp(ctx.queue);

            let cols = vp.to_cols_array_2d();
            shadow_vp_cols = cols;
            shadow_enabled = 1.0;
        }

        // Update lighting params (camera position for fog calculations + shadow VP)
        if let Some(ref lighting_buffer) = gpu.lighting_buffer {
            let mut lighting_params = LightingParams {
                world_camera_pos: [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                    0.0,
                ],
                ..Default::default()
            };
            // Wire shadow VP matrix into lighting params
            lighting_params.shadow_vp_col0 = shadow_vp_cols[0];
            lighting_params.shadow_vp_col1 = shadow_vp_cols[1];
            lighting_params.shadow_vp_col2 = shadow_vp_cols[2];
            lighting_params.shadow_vp_col3 = shadow_vp_cols[3];
            lighting_params.shadow_params[2] = shadow_enabled; // enabled flag
            ctx.queue
                .write_buffer(lighting_buffer, 0, bytemuck::bytes_of(&lighting_params));
        }

        // Update foliage camera position
        if let Some(foliage) = &self.foliage_resources {
            foliage.update_camera(
                ctx.queue,
                [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                ],
                0.0, // time (unused for now)
            );
        }

        // Run GPU compositing pass for dirty chunks (if enabled)
        if self.use_gpu_compositing
            && let (Some(compositor), Some(bind_group)) =
                (&mut self.compositor, &self.compositor_bind_group)
        {
            let mut chunk_layer_counts = vec![1u32; 256];
            for chunk in &self.chunk_splat_data {
                let grid_idx = (chunk.grid_x * 16 + chunk.grid_z) as usize;
                if grid_idx < 256 {
                    chunk_layer_counts[grid_idx] = chunk.layer_texture_ids.len() as u32;
                }
            }

            compositor.composite_all_dirty(
                ctx.encoder,
                bind_group,
                ctx.queue,
                &chunk_layer_counts,
                ctx.device,
                self.compositor_debug_mode,
            );
        }

        // Shadow pass (renders terrain from light's perspective)
        if let Some(shadow) = &self.shadow_resources
            && gpu.use_gpu_tessellation
        {
            shadow.render(
                ctx.encoder,
                &gpu.vertex_buffer,
                &gpu.index_buffer,
                gpu.index_count,
                gpu.num_patch_instances,
            );
        }

        // Render terrain
        {
            let mut render_pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Terrain Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear - already cleared
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&gpu.pipeline);
            render_pass.set_bind_group(0, &gpu.camera_bind_group, &[]);
            render_pass.set_bind_group(1, &gpu.texture_bind_group, &[]);
            render_pass.set_vertex_buffer(0, gpu.vertex_buffer.slice(..));

            if gpu.use_gpu_tessellation {
                // GPU tessellation: instanced draw with patch vertices
                // index_buffer contains instance indices (patch indices)
                render_pass.set_vertex_buffer(1, gpu.index_buffer.slice(..));
                // Draw non-indexed triangles, instanced per patch
                render_pass.draw(0..gpu.index_count, 0..gpu.num_patch_instances);
            } else {
                // Regular indexed draw
                render_pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..gpu.index_count, 0, 0..1);
            }

            // Render foliage on top of terrain
            if let Some(foliage) = &self.foliage_resources {
                crate::foliage::render_foliage(
                    &mut render_pass,
                    foliage,
                    &gpu.camera_bind_group,
                    &self.foliage_qn_chunks,
                );
            }

            // Render roads on top of terrain
            if let Some(roads) = &self.road_resources {
                crate::roads::render_roads(&mut render_pass, roads, &gpu.camera_bind_group);
            }
        }
    }
}
