//! 3D rendering for the terrain viewer.
//!
//! Implements the `Application3D` trait: GPU initialization, resize, and render pass.

use num_traits::ToPrimitive;
use render::terrain::{LightingParams, RawXtdData, TerrainParams, TessellationMode};
use render::{Application3D, RenderContext, wgpu};

use super::TerrainViewer;
use crate::gpu::create_depth_texture;
use crate::types::CpuTerrainData;

fn chunk_grid_index(grid_x: i32, grid_z: i32) -> Option<usize> {
    let grid_x = usize::try_from(grid_x).ok()?;
    let grid_z = usize::try_from(grid_z).ok()?;
    grid_x
        .checked_mul(16)?
        .checked_add(grid_z)
        .filter(|&index| index < 256)
}

impl TerrainViewer {
    fn initialize_gpu_resources(&mut self, ctx: &RenderContext<'_>) {
        if self.gpu.is_some() || self.scene.is_none() {
            return;
        }

        let scene = self.scene.as_mut().expect("scene was checked above");
        if self.tessellation_mode == TessellationMode::Gpu
            && let Some(raw_data) = &scene.raw_xtd_data
        {
            let raw_data = RawXtdData {
                packed_positions: raw_data.packed_positions.clone(),
                packed_normals: raw_data.packed_normals.clone(),
                num_verts_per_axis: raw_data.num_verts_per_axis,
                mid: raw_data.mid,
                range: raw_data.range,
                tile_scale: raw_data.tile_scale,
                ao_data: raw_data.ao_data.clone(),
                alpha_data: raw_data.alpha_data.clone(),
            };
            let albedo = scene.albedo.take();
            self.create_gpu_tessellation_resources(
                ctx.device,
                ctx.queue,
                &raw_data,
                albedo.as_ref(),
                [ctx.size.0, ctx.size.1],
            );
            return;
        }

        if self.tessellation_mode == TessellationMode::Gpu {
            log::warn!(
                "GPU tessellation requested but no raw XTD data available, falling back to regular rendering"
            );
        }
        let positions = scene.mesh.positions.clone();
        let normals = scene.mesh.normals.clone();
        let uvs = scene.mesh.uvs.clone();
        let indices = scene.mesh.indices.clone();
        let albedo = scene.albedo.take();
        self.create_gpu_resources_from_data(
            ctx.device,
            ctx.queue,
            &CpuTerrainData {
                positions: &positions,
                normals: &normals,
                uvs: &uvs,
                indices: &indices,
                albedo: albedo.as_ref(),
                surface_size: [ctx.size.0, ctx.size.1],
            },
        );
    }

    fn update_terrain_uniforms(&self, queue: &wgpu::Queue, size: (u32, u32)) {
        let Some(gpu) = &self.gpu else { return };
        let width = size.0.to_f32().expect("surface width must fit f32");
        let height = size.1.to_f32().expect("surface height must fit f32");
        let view_projection =
            self.camera.projection_matrix(width / height) * self.camera.view_matrix();
        queue.write_buffer(
            &gpu.camera_buffer,
            0,
            bytemuck::cast_slice(&view_projection.to_cols_array()),
        );

        let params = TerrainParams {
            terrain_size: gpu.terrain_size,
            chunk_count: [16.0, 16.0],
            texture_tile_scale: gpu.tile_scale,
            debug_mode: self.debug_mode.to_f32().expect("debug mode must fit f32"),
            bump_power: self.bump_power,
            padding: 0.0,
        };
        queue.write_buffer(&gpu.params_buffer, 0, bytemuck::bytes_of(&params));
    }

    fn update_lighting(&mut self, queue: &wgpu::Queue) {
        let Some(gpu) = &self.gpu else { return };
        let light_direction = glam::Vec3::new(0.4, 0.8, 0.3).normalize();
        let mut shadow_columns = [[1.0_f32, 0.0, 0.0, 0.0]; 4];
        let mut shadow_enabled = 0.0;

        if let Some(shadow) = &mut self.shadow_resources {
            let terrain_center =
                glam::Vec3::new(gpu.terrain_size[0] * 0.5, 50.0, gpu.terrain_size[1] * 0.5);
            let terrain_size = glam::Vec3::new(gpu.terrain_size[0], 100.0, gpu.terrain_size[1]);
            shadow_columns = shadow
                .compute_light_vp(light_direction, terrain_center, terrain_size)
                .to_cols_array_2d();
            shadow.update_light_vp(queue);
            shadow_enabled = 1.0;
        }

        if let Some(lighting_buffer) = &gpu.lighting_buffer {
            let mut params = LightingParams {
                world_camera_pos: [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                    0.0,
                ],
                ..Default::default()
            };
            params.shadow_vp_col0 = shadow_columns[0];
            params.shadow_vp_col1 = shadow_columns[1];
            params.shadow_vp_col2 = shadow_columns[2];
            params.shadow_vp_col3 = shadow_columns[3];
            params.shadow_params[2] = shadow_enabled;
            queue.write_buffer(lighting_buffer, 0, bytemuck::bytes_of(&params));
        }
    }

    fn update_foliage_camera(&self, queue: &wgpu::Queue) {
        if let Some(foliage) = &self.foliage_resources {
            foliage.update_camera(
                queue,
                [
                    self.camera.position.x,
                    self.camera.position.y,
                    self.camera.position.z,
                ],
                0.0,
            );
        }
    }

    fn composite_dirty_chunks(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.use_gpu_compositing {
            return;
        }
        let (Some(compositor), Some(bind_group), Some(scene)) = (
            &mut self.compositor,
            &self.compositor_bind_group,
            &self.scene,
        ) else {
            return;
        };

        let mut layer_counts = vec![1; 256];
        for chunk in &scene.chunk_splat_data {
            if let Some(index) = chunk_grid_index(chunk.grid_x, chunk.grid_z) {
                layer_counts[index] = u32::try_from(chunk.layer_texture_ids.len())
                    .expect("terrain layer count must fit u32");
            }
        }
        let mut decal_layer_counts = vec![0; 256];
        for chunk in &scene.chunk_decal_data {
            if let Some(index) = chunk_grid_index(chunk.grid_x, chunk.grid_z) {
                decal_layer_counts[index] = u32::try_from(chunk.decal_layer_ids.len())
                    .expect("decal layer count must fit u32");
            }
        }

        compositor.composite_all_dirty(
            bind_group,
            queue,
            &layer_counts,
            &decal_layer_counts,
            device,
            self.compositor_debug_mode,
        );
    }

    fn render_shadow_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(gpu), Some(shadow)) = (&self.gpu, &self.shadow_resources) else {
            return;
        };
        if gpu.use_gpu_tessellation {
            shadow.render(
                encoder,
                &gpu.vertex_buffer,
                &gpu.index_buffer,
                gpu.index_count,
                gpu.num_patch_instances,
            );
        }
    }

    fn render_terrain_pass(&self, ctx: &mut RenderContext<'_>) {
        let Some(gpu) = &self.gpu else { return };
        let mut render_pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Terrain Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
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
            render_pass.set_vertex_buffer(1, gpu.index_buffer.slice(..));
            render_pass.draw(0..gpu.index_count, 0..gpu.num_patch_instances);
        } else {
            render_pass.set_index_buffer(gpu.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..gpu.index_count, 0, 0..1);
        }

        if let Some(foliage) = &self.foliage_resources {
            crate::foliage::render_foliage(
                &mut render_pass,
                foliage,
                &gpu.camera_bind_group,
                self.scene
                    .as_ref()
                    .map_or(&[], |scene| scene.foliage_qn_chunks.as_slice()),
            );
        }
        if let Some(roads) = &self.road_resources {
            crate::roads::render_roads(&mut render_pass, roads, &gpu.camera_bind_group);
        }
    }
}

impl Application3D for TerrainViewer {
    fn init_gpu(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        self.surface_format = format;
    }

    fn resize_gpu(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let Some(gpu) = &mut self.gpu {
            let (depth_texture, depth_view) = create_depth_texture(device, width, height);
            gpu.depth_texture = depth_texture;
            gpu.depth_view = depth_view;
        }
    }

    fn render_3d(&mut self, ctx: &mut RenderContext<'_>) {
        self.initialize_gpu_resources(ctx);
        if self.gpu.is_none() {
            return;
        }
        self.update_terrain_uniforms(ctx.queue, ctx.size);
        self.update_lighting(ctx.queue);
        self.update_foliage_camera(ctx.queue);
        self.composite_dirty_chunks(ctx.device, ctx.queue);
        self.render_shadow_pass(ctx.encoder);
        self.render_terrain_pass(ctx);
    }
}
