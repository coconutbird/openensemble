//! 3D rendering for the terrain viewer.
//!
//! Implements the `Application3D` trait: GPU initialization, resize, and render pass.

use num_traits::ToPrimitive;
use render::postprocess::{ToneMapResources, ToneMapSettings};
use render::terrain::{LightingParams, RawXtdData, TerrainParams};
use render::ugx::RendererResources;
use render::{Application3D, RenderContext, RenderPhase, WorldRenderer, wgpu};

use super::TerrainViewer;
use crate::capture::{
    CaptureState, CaptureTarget, ValidationCamera, foliage_camera, top_down_camera,
    top_down_detail_camera, write_foliage_placement_reference, write_foliage_references,
    write_packed_height_reference, write_tessellation_reference, write_xtt_reference,
};
use crate::gpu::create_depth_texture;

impl TerrainViewer {
    fn render_units<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        if let Some(renderer) = &self.ugx_scene_renderer {
            renderer.render_phase(phase, pass);
        }
    }

    fn initialize_gpu_resources(&mut self, ctx: &RenderContext<'_>) {
        if self.gpu.is_some() || self.scene.is_none() {
            return;
        }

        if self.tone_map_resources.is_none() {
            self.tone_map_resources = Some(ToneMapResources::new(
                ctx.device,
                ctx.format,
                [ctx.size.0, ctx.size.1],
            ));
        }
        let scene = self.scene.as_ref().expect("scene was checked above");
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
        self.write_decoded_capture_references(&raw_data);
        let albedo = self
            .scene
            .as_mut()
            .expect("scene was checked above")
            .albedo
            .take();
        self.create_gpu_tessellation_resources(
            ctx.device,
            ctx.queue,
            &raw_data,
            albedo.as_ref(),
            [ctx.size.0, ctx.size.1],
        );
        let directional_shadow_view = self
            .shadow_resources
            .as_ref()
            .map(|shadow| shadow.shadow_view.clone());
        let terrain_heightfield = self
            .gpu
            .as_ref()
            .map(|gpu| render::ugx::TerrainHeightfield {
                view: &gpu.position_texture_view,
                dimension: raw_data.num_verts_per_axis,
                tile_scale: raw_data.tile_scale,
                world_min_xz: [raw_data.world_min[0], raw_data.world_min[2]],
                y_range: raw_data.range[1],
                y_mid: raw_data.mid[1],
                normalized_y_bias: render::terrain::NORMALIZED_TERRAIN_Y_OFFSET,
            });
        let world = render::ugx::WorldBindings {
            environment: self.environment.as_ref(),
            directional_shadow: directional_shadow_view.as_ref(),
            terrain_heightfield,
            local_lights: self.gpu.as_ref().map(|gpu| &gpu.local_lights),
            ..Default::default()
        };
        let ugx_resources =
            RendererResources::new_with_world(ctx.device, ctx.queue, self.scene_format, world);
        if let Some(sky) = &self.sky_unit {
            self.sky_renderer = Some(render::ugx::UnitRenderer::new_with_resources(
                ctx.device,
                ctx.queue,
                sky,
                glam::Mat4::IDENTITY,
                &ugx_resources,
            ));
        }
        if let Some(scene) = &self.ugx_scene {
            let renderer = render::ugx::UnitSceneRenderer::new_with_resources(
                ctx.device,
                ctx.queue,
                scene,
                &ugx_resources,
            );
            log::info!(
                "Uploaded {} simulation-backed UGX placements",
                renderer.placement_count()
            );
            self.ugx_scene_renderer = Some(renderer);
        }
    }

    fn write_decoded_capture_references(&self, raw_data: &RawXtdData) {
        let (Some(capture), Some(scene)) = (&self.capture, &self.scene) else {
            return;
        };
        if let Some(albedo) = &scene.albedo {
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
        match write_packed_height_reference(capture.config(), raw_data) {
            Ok(path) => log::info!("Wrote unpacked XTD height map to {}", path.display()),
            Err(error) => log::error!("Failed to write unpacked XTD height map: {error:#}"),
        }
        match write_tessellation_reference(capture.config(), raw_data) {
            Ok(Some(path)) => log::info!("Wrote XTD tessellation map to {}", path.display()),
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
            chunk_count: gpu.chunk_grid.dimensions_f32(),
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
        let camera = [camera_position.x, camera_position.y, camera_position.z];
        let mut params = self.lightset.as_ref().map_or_else(
            || LightingParams {
                world_camera_pos: [camera[0], camera[1], camera[2], 0.0],
                ..Default::default()
            },
            |lightset| LightingParams::from_lightset(lightset, camera),
        );
        let light_direction = glam::Vec3::from_array([
            params.dir_light_vec[0],
            params.dir_light_vec[1],
            params.dir_light_vec[2],
        ])
        .normalize_or_zero();
        let shadows_requested = self
            .lightset
            .as_ref()
            .is_none_or(|lightset| lightset.sun_shadows);
        let mut shadow_columns = [[1.0_f32, 0.0, 0.0, 0.0]; 4];
        let mut shadow_enabled = 0.0;

        if shadows_requested
            && light_direction != glam::Vec3::ZERO
            && let Some(shadow) = &mut self.shadow_resources
        {
            let terrain_center =
                glam::Vec3::new(gpu.terrain_size[0] * 0.5, 50.0, gpu.terrain_size[1] * 0.5);
            let terrain_size = glam::Vec3::new(gpu.terrain_size[0], 100.0, gpu.terrain_size[1]);
            shadow_columns = shadow
                .compute_light_vp(light_direction, terrain_center, terrain_size)
                .to_cols_array_2d();
            shadow.update_light_vp(queue);
            shadow_enabled = 1.0;
        }

        params.shadow_vp_col0 = shadow_columns[0];
        params.shadow_vp_col1 = shadow_columns[1];
        params.shadow_vp_col2 = shadow_columns[2];
        params.shadow_vp_col3 = shadow_columns[3];
        params.shadow_params[2] = shadow_enabled;
        self.local_lights.apply_to_lighting(&mut params);
        gpu.local_lights.update(queue, &self.local_lights);
        if let Some(lighting_buffer) = &gpu.lighting_buffer {
            queue.write_buffer(lighting_buffer, 0, bytemuck::bytes_of(&params));
        }
        if let Some(foliage) = &mut self.foliage_resources {
            foliage.update_frame(queue, &params, self.render_time_seconds);
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
        if let Some(renderer) = &mut self.sky_renderer {
            let camera = glam::Vec3::from_array([
                lighting.world_camera_pos[0],
                lighting.world_camera_pos[1],
                lighting.world_camera_pos[2],
            ]);
            renderer.update_frame_at_time(
                queue,
                view_projection,
                glam::Mat4::from_translation(camera),
                lighting,
                self.render_time_seconds,
            );
        }
        if let Some(renderer) = &mut self.ugx_scene_renderer {
            let simulation = self
                .simulation
                .as_ref()
                .expect("a unit scene is always built from simulation state");
            renderer.update_from_world_at_time(
                queue,
                &simulation.world,
                view_projection,
                lighting,
                self.render_time_seconds,
            );
        }
    }

    fn composite_dirty_chunks(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(chunk_grid) = self.gpu.as_ref().map(|gpu| gpu.chunk_grid) else {
            return;
        };
        let (Some(compositor), Some(bind_group), Some(scene)) = (
            &mut self.compositor,
            &self.compositor_bind_group,
            &self.scene,
        ) else {
            return;
        };

        let mut layer_counts = vec![1; chunk_grid.total_chunks()];
        for chunk in &scene.chunk_splat_data {
            if let Some(index) = chunk_grid.chunk_index(chunk.grid_x, chunk.grid_z) {
                layer_counts[index] = u32::try_from(chunk.layer_texture_ids.len())
                    .expect("terrain layer count must fit u32");
            }
        }
        let mut decal_layer_counts = vec![0; chunk_grid.total_chunks()];
        for chunk in &scene.chunk_decal_data {
            if let Some(index) = chunk_grid.chunk_index(chunk.grid_x, chunk.grid_z) {
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
        if self
            .lightset
            .as_ref()
            .is_some_and(|lightset| !lightset.sun_shadows)
        {
            return;
        }
        let Some(shadow) = &self.shadow_resources else {
            return;
        };
        for cascade in 0..shadow.cascade_count() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("World Directional Shadow Cascade"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: shadow.cascade_shadow_view(cascade),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: shadow.cascade_depth_view(cascade),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            let phase = RenderPhase::Shadow { cascade };
            shadow.render_phase(phase, &mut pass);
            if let Some(foliage) = &self.foliage_resources {
                foliage.render_phase(phase, &mut pass);
            }
            self.render_units(phase, &mut pass);
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
        if include_details && let Some(renderer) = &self.sky_renderer {
            renderer.render_phase(RenderPhase::Sky, &mut render_pass);
        }
        gpu.render_phase(RenderPhase::World, &mut render_pass);

        if include_details {
            self.render_units(RenderPhase::World, &mut render_pass);
            if let Some(foliage) = &self.foliage_resources {
                foliage.render_phase(RenderPhase::World, &mut render_pass);
            }
            if let Some(roads) = &self.road_resources {
                roads.render_phase(RenderPhase::World, &mut render_pass);
            }
        }
    }

    fn render_distortion_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        distortion_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        include_details: bool,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("UGX Distortion Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: distortion_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        if include_details {
            self.render_units(RenderPhase::Distortion, &mut render_pass);
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
        let tone_map = self
            .tone_map_resources
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("tone-map resources are unavailable"))?;
        tone_map.resize(ctx.device, [size, size]);
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
        self.render_shadow_pass(&mut encoder);
        let tone_map = self
            .tone_map_resources
            .as_ref()
            .expect("tone-map resources were checked above");
        self.render_terrain_pass(
            &mut encoder,
            tone_map.scene_view(),
            &depth_view,
            wgpu::LoadOp::Clear(scene_clear_color(&lighting)),
            include_details,
        );
        self.render_distortion_pass(
            &mut encoder,
            tone_map.distortion_view(),
            &depth_view,
            include_details,
        );
        let tone_settings = if debug_mode == 12 {
            self.lightset
                .as_ref()
                .map_or_else(ToneMapSettings::default, ToneMapSettings::from)
        } else {
            ToneMapSettings::default()
        };
        tone_map.update(ctx.queue, tone_settings);
        tone_map.encode(&mut encoder, &target.view);
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
        _format: wgpu::TextureFormat,
    ) {
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
        if let Some(tone_map) = &mut self.tone_map_resources {
            tone_map.resize(device, [width, height]);
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
        let tone_map = self
            .tone_map_resources
            .as_ref()
            .expect("GPU initialization creates tone-map resources");
        let tone_settings = self
            .lightset
            .as_ref()
            .map_or_else(ToneMapSettings::default, ToneMapSettings::from);
        tone_map.update(ctx.queue, tone_settings);
        let depth_view = &self
            .gpu
            .as_ref()
            .expect("GPU resources were checked above")
            .depth_view;
        self.render_terrain_pass(
            ctx.encoder,
            tone_map.scene_view(),
            depth_view,
            wgpu::LoadOp::Clear(scene_clear_color(&lighting)),
            true,
        );
        self.render_distortion_pass(ctx.encoder, tone_map.distortion_view(), depth_view, true);
        tone_map.encode(ctx.encoder, ctx.view);
    }
}

fn scene_clear_color(lighting: &LightingParams) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(lighting.fog_color[0]),
        g: f64::from(lighting.fog_color[1]),
        b: f64::from(lighting.fog_color[2]),
        a: 1.0,
    }
}
