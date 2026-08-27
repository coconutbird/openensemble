//! 3D rendering for the terrain viewer.
//!
//! Implements the `Application3D` trait: GPU initialization, resize, and render pass.

use num_traits::ToPrimitive;
use render::terrain::{LightingParams, RawXtdData, TerrainParams};
use render::{Application3D, RenderContext, wgpu};

use super::TerrainViewer;
use crate::capture::{
    CaptureState, CaptureTarget, ValidationCamera, foliage_camera, top_down_camera,
    top_down_detail_camera, write_foliage_placement_reference, write_foliage_references,
    write_packed_height_reference, write_tessellation_reference, write_xtt_reference,
};
use crate::gpu::create_depth_texture;
use crate::types::terrain_chunk_index;

impl TerrainViewer {
    fn initialize_gpu_resources(&mut self, ctx: &RenderContext<'_>) {
        if self.gpu.is_some() || self.scene.is_none() {
            return;
        }

        let scene = self.scene.as_mut().expect("scene was checked above");
        let Some(raw_data) = &scene.raw_xtd_data else {
            log::error!("Packed XTD terrain data is required by the GPU terrain renderer");
            return;
        };
        let raw_data = RawXtdData {
            packed_positions: raw_data.packed_positions.clone(),
            packed_normals: raw_data.packed_normals.clone(),
            num_verts_per_axis: raw_data.num_verts_per_axis,
            mid: raw_data.mid,
            range: raw_data.range,
            tile_scale: raw_data.tile_scale,
            world_min: raw_data.world_min,
            world_max: raw_data.world_max,
            tessellation: raw_data.tessellation.clone(),
            ao_data: raw_data.ao_data.clone(),
            alpha_data: raw_data.alpha_data.clone(),
        };
        if let (Some(capture), Some(albedo)) = (&self.capture, &scene.albedo) {
            match write_xtt_reference(capture.config(), albedo) {
                Ok(path) => log::info!("Wrote decoded XTT atlas oracle to {}", path.display()),
                Err(error) => log::error!("Failed to write decoded XTT atlas oracle: {error:#}"),
            }
            match write_foliage_placement_reference(
                capture.config(),
                albedo,
                &scene.foliage_qn_chunks,
                &scene.foliage_sets,
                raw_data.num_verts_per_axis,
            ) {
                Ok(path) => {
                    log::info!("Wrote decoded foliage placement map to {}", path.display());
                }
                Err(error) => log::error!("Failed to write foliage placement map: {error:#}"),
            }
        }
        if let Some(capture) = &self.capture {
            match write_packed_height_reference(capture.config(), &raw_data) {
                Ok(path) => log::info!("Wrote unpacked XTD height map to {}", path.display()),
                Err(error) => log::error!("Failed to write unpacked XTD height map: {error:#}"),
            }
            match write_tessellation_reference(capture.config(), &raw_data) {
                Ok(Some(path)) => {
                    log::info!("Wrote XTD tessellation map to {}", path.display());
                }
                Ok(None) => log::warn!("No XTD tessellation metadata available for capture"),
                Err(error) => log::error!("Failed to write XTD tessellation map: {error:#}"),
            }
            match write_foliage_references(capture.config(), &scene.foliage_sets) {
                Ok(paths) => {
                    for path in paths {
                        log::info!("Wrote decoded foliage reference to {}", path.display());
                    }
                }
                Err(error) => log::error!("Failed to write foliage references: {error:#}"),
            }
        }
        let albedo = scene.albedo.take();
        self.create_gpu_tessellation_resources(
            ctx.device,
            ctx.queue,
            &raw_data,
            albedo.as_ref(),
            [ctx.size.0, ctx.size.1],
        );
        if let Some(unit) = &self.ugx_unit {
            self.ugx_renderer = Some(render::ugx::UnitRenderer::new(
                ctx.device,
                ctx.queue,
                ctx.format,
                unit,
                self.ugx_transform,
            ));
        }
    }

    fn update_terrain_uniforms(
        &self,
        queue: &wgpu::Queue,
        view_projection: glam::Mat4,
        debug_mode: u32,
    ) {
        let Some(gpu) = &self.gpu else { return };
        queue.write_buffer(
            &gpu.camera_buffer,
            0,
            bytemuck::cast_slice(&view_projection.to_cols_array()),
        );

        let params = TerrainParams {
            terrain_size: gpu.terrain_size,
            chunk_count: [16.0, 16.0],
            texture_tile_scale: gpu.tile_scale,
            debug_mode: debug_mode.to_f32().expect("debug mode must fit f32"),
            bump_power: self.bump_power,
            padding: 0.0,
        };
        queue.write_buffer(&gpu.params_buffer, 0, bytemuck::bytes_of(&params));
    }

    fn update_lighting(
        &mut self,
        queue: &wgpu::Queue,
        camera_position: glam::Vec3,
    ) -> LightingParams {
        let Some(gpu) = &self.gpu else {
            return LightingParams::default();
        };
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

        let mut params = LightingParams {
            world_camera_pos: [camera_position.x, camera_position.y, camera_position.z, 0.0],
            ..Default::default()
        };
        params.shadow_vp_col0 = shadow_columns[0];
        params.shadow_vp_col1 = shadow_columns[1];
        params.shadow_vp_col2 = shadow_columns[2];
        params.shadow_vp_col3 = shadow_columns[3];
        params.shadow_params[2] = shadow_enabled;
        if let Some(lighting_buffer) = &gpu.lighting_buffer {
            queue.write_buffer(lighting_buffer, 0, bytemuck::bytes_of(&params));
        }
        if let Some(foliage) = &mut self.foliage_resources {
            foliage.update_frame(queue, &params, 0.0);
        }
        if let Some(roads) = &mut self.road_resources {
            roads.update_frame(queue, &params, self.bump_power);
        }
        params
    }

    fn update_ugx(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: glam::Mat4,
        lighting: &LightingParams,
    ) {
        if let Some(renderer) = &mut self.ugx_renderer {
            renderer.update_frame(queue, view_projection, self.ugx_transform, lighting);
        }
    }

    fn composite_dirty_chunks(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let (Some(compositor), Some(bind_group), Some(scene)) = (
            &mut self.compositor,
            &self.compositor_bind_group,
            &self.scene,
        ) else {
            return;
        };

        let mut layer_counts = vec![1; 256];
        for chunk in &scene.chunk_splat_data {
            if let Some(index) = terrain_chunk_index(chunk.grid_x, chunk.grid_z) {
                layer_counts[index] = u32::try_from(chunk.layer_texture_ids.len())
                    .expect("terrain layer count must fit u32");
            }
        }
        let mut decal_layer_counts = vec![0; 256];
        for chunk in &scene.chunk_decal_data {
            if let Some(index) = terrain_chunk_index(chunk.grid_x, chunk.grid_z) {
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
        shadow.render(
            encoder,
            &gpu.vertex_buffer,
            &gpu.index_buffer,
            gpu.index_count,
            gpu.num_patch_instances,
        );
        if let Some(foliage) = &self.foliage_resources {
            crate::foliage::render_foliage_shadow(encoder, foliage, shadow);
        }
    }

    fn render_terrain_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        color_load: wgpu::LoadOp<wgpu::Color>,
        include_details: bool,
    ) {
        let Some(gpu) = &self.gpu else { return };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Terrain Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
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
        render_pass.set_vertex_buffer(1, gpu.index_buffer.slice(..));
        render_pass.draw(0..gpu.index_count, 0..gpu.num_patch_instances);

        if include_details {
            if let Some(renderer) = &self.ugx_renderer {
                renderer.render(&mut render_pass);
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

    fn capture_camera(&self, size: (u32, u32)) -> Option<ValidationCamera> {
        let config = self
            .capture
            .as_ref()
            .filter(|capture| capture.is_pending())?
            .config();
        let scene = self.scene.as_ref()?;
        Some(match (config.center, config.span) {
            (Some(center), Some(span)) => top_down_detail_camera(&scene.mesh, size, center, span),
            _ => top_down_camera(&scene.mesh, size),
        })
    }

    fn capture_frame(
        &mut self,
        ctx: &RenderContext<'_>,
        size: u32,
        camera: ValidationCamera,
        debug_mode: u32,
        path: &std::path::Path,
        include_details: bool,
    ) -> anyhow::Result<()> {
        self.update_terrain_uniforms(ctx.queue, camera.view_projection, debug_mode);
        let lighting = self.update_lighting(ctx.queue, camera.position);
        self.update_ugx(ctx.queue, camera.view_projection, &lighting);
        let target = CaptureTarget::new(ctx.device, size, ctx.format)?;
        let (_depth_texture, depth_view) = create_depth_texture(ctx.device, size, size);
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Terrain Validation Capture Encoder"),
            });
        self.render_terrain_pass(
            &mut encoder,
            &target.view,
            &depth_view,
            wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.4,
                g: 0.6,
                b: 0.9,
                a: 1.0,
            }),
            include_details,
        );
        target.encode_copy(&mut encoder);
        ctx.queue.submit(std::iter::once(encoder.finish()));
        target.write_png(ctx.device, path)
    }

    fn capture_top_down(&mut self, ctx: &RenderContext<'_>) {
        let Some(config) = self
            .capture
            .as_ref()
            .filter(|capture| capture.is_pending())
            .map(|capture| capture.config().clone())
        else {
            return;
        };

        let Some(camera) = self.capture_camera((config.size, config.size)) else {
            log::error!("Cannot capture top-down terrain without loaded mesh bounds");
            if let Some(capture) = &mut self.capture {
                capture.finish();
            }
            return;
        };

        let foliage_cameras = self
            .scene
            .as_ref()
            .and_then(|scene| {
                scene.raw_xtd_data.as_ref().map(|raw| {
                    scene
                        .foliage_sets
                        .iter()
                        .enumerate()
                        .filter_map(|(set_index, _)| {
                            foliage_camera(
                                raw,
                                &scene.foliage_qn_chunks,
                                &scene.foliage_sets,
                                set_index,
                                (config.size, config.size),
                            )
                            .map(|camera| (set_index, camera))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();

        let mut result =
            self.capture_frame(ctx, config.size, camera, 12, &config.output_path, true);
        if result.is_ok() {
            result = self.capture_frame(ctx, config.size, camera, 14, &config.height_path(), false);
        }
        if result.is_ok() {
            result = self.capture_frame(
                ctx,
                config.size,
                camera,
                15,
                &config.alignment_path(),
                false,
            );
        }
        for (set_index, foliage_camera) in foliage_cameras {
            if result.is_err() {
                break;
            }
            result = self.capture_frame(
                ctx,
                config.size,
                foliage_camera,
                12,
                &config.foliage_view_path(set_index),
                true,
            );
        }

        match result {
            Ok(()) => log::info!(
                "Wrote deterministic terrain and foliage GPU captures to {}, {}, and {}",
                config.output_path.display(),
                config.height_path().display(),
                config.alignment_path().display()
            ),
            Err(error) => log::error!(
                "Failed to write top-down GPU capture {}: {error:#}",
                config.output_path.display()
            ),
        }
        if let Some(capture) = &mut self.capture {
            capture.finish();
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
        let capture_camera = self.capture_camera(ctx.size);
        let (view_projection, camera_position) = capture_camera.map_or_else(
            || {
                let width = ctx.size.0.to_f32().expect("surface width must fit f32");
                let height = ctx.size.1.to_f32().expect("surface height must fit f32");
                (
                    self.camera.projection_matrix(width / height) * self.camera.view_matrix(),
                    self.camera.position,
                )
            },
            |camera| (camera.view_projection, camera.position),
        );
        self.update_terrain_uniforms(ctx.queue, view_projection, self.debug_mode);
        let lighting = self.update_lighting(ctx.queue, camera_position);
        self.update_ugx(ctx.queue, view_projection, &lighting);
        if let (Some(camera), Some(foliage)) = (capture_camera, &mut self.foliage_resources) {
            foliage.set_fade_distances(
                ctx.queue,
                camera.foliage_fade_start,
                camera.foliage_fade_start + 1.0,
            );
        }
        self.composite_dirty_chunks(ctx.device, ctx.queue);
        if self.capture.as_ref().is_some_and(CaptureState::is_pending) {
            if self
                .compositor
                .as_ref()
                .is_some_and(|compositor| compositor.dirty_chunk_count() != 0)
            {
                return;
            }
            self.capture_top_down(ctx);
            return;
        }
        self.render_shadow_pass(ctx.encoder);
        let depth_view = &self
            .gpu
            .as_ref()
            .expect("GPU resources were checked above")
            .depth_view;
        self.render_terrain_pass(ctx.encoder, ctx.view, depth_view, wgpu::LoadOp::Load, true);
    }
}
