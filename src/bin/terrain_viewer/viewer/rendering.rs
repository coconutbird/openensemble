//! 3D rendering for the terrain viewer.
//!
//! Gives the loaded scenario to a `WorldView`, selects the camera of each frame and runs the
//! top-down captures. The `Application` callbacks call `resize_gpu_resources` and
//! `render_scene`.

use num_traits::ToPrimitive;
use render::app::RenderContext;
use render::lighting::LocalLightView;
use render::particle::ParticleScene;
use render::terrain::RawXtdData;
use render::ugx::AnimationCameraShake;
use render::wgpu;
use render::world_view::{
    WorldCamera, WorldFrame, WorldView, WorldViewInputs, WorldViewSettings, WorldViewUnits,
};

use super::TerrainViewer;
use crate::capture::{
    CaptureState, CaptureTarget, ValidationCamera, foliage_camera, top_down_camera,
    top_down_detail_camera, write_foliage_placement_reference, write_foliage_references,
    write_packed_height_reference, write_tessellation_reference, write_xtt_reference,
};

impl TerrainViewer {
    fn initialize_world_view(&mut self, ctx: &RenderContext<'_>) {
        if self.world_view.is_some() || self.load_error.is_some() {
            return;
        }
        let Some(scene) = &self.scene else {
            return;
        };
        if let Some(raw_data) = &scene.raw_xtd_data {
            self.write_decoded_capture_references(raw_data);
        }
        let units = match (&self.ugx_scene, &self.simulation) {
            (Some(scene), Some(simulation)) => Some(WorldViewUnits {
                scene,
                world: &simulation.world,
            }),
            _ => None,
        };
        let inputs = WorldViewInputs {
            terrain: scene,
            lightset: self.lightset.take(),
            environment: self.environment.as_ref(),
            sky: self.sky_unit.as_ref(),
            units,
            asset_source: self.asset_source.as_mut(),
            size: [ctx.size.0, ctx.size.1],
            output_format: ctx.format,
        };
        let settings = WorldViewSettings {
            debug_mode: self.debug_mode,
            compositor_debug_mode: self.compositor_debug_mode,
            bump_power: self.bump_power,
            lod: None,
            foliage_fade: None,
            details: true,
            scenario_tone_map: true,
        };
        match WorldView::new(ctx.device, ctx.queue, inputs, &settings) {
            Ok(world_view) => {
                self.world_view = Some(world_view);
                // The GPU has the composited atlas, so release the decoded XTT albedo.
                if let Some(scene) = &mut self.scene {
                    scene.albedo = None;
                }
            }
            Err(error) => {
                log::error!("{error}");
                self.load_error = Some(error.to_string());
            }
        }
    }

    fn sync_unit_roster(&mut self, ctx: &RenderContext<'_>) {
        if !self.ugx_roster_dirty {
            return;
        }
        if let (Some(world_view), Some(scene), Some(simulation)) =
            (&mut self.world_view, &self.ugx_scene, &self.simulation)
        {
            world_view.sync_units(ctx.device, ctx.queue, scene, &simulation.world);
        }
        self.ugx_roster_dirty = false;
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

    /// Starts the visible animation camera shakes on the camera adapter.
    fn begin_camera_shakes(&mut self, view: LocalLightView, shakes: Vec<AnimationCameraShake>) {
        for shake in shakes {
            // The terrain viewer does not yet expose entity selection. Retail
            // suppresses the two shipped checkSelected tags unless their owner
            // is selected, so conservatively suppress them here too.
            if shake.check_selected()
                || view
                    .project_sphere(shake.position().to_array(), 1.0)
                    .is_none()
            {
                continue;
            }
            self.camera_adapter.begin_animation_shake(
                shake.duration_seconds(),
                shake.strength(),
                self.render_time_seconds,
            );
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

    fn interactive_camera(&self, size: (u32, u32)) -> WorldCamera {
        let width = size.0.to_f32().expect("surface width must fit f32");
        let height = size.1.to_f32().expect("surface height must fit f32");
        let world_to_view = self.camera.view_matrix();
        let projection = self.camera.projection_matrix(width / height);
        WorldCamera {
            view_projection: projection * world_to_view,
            world_to_view,
            position: self.camera.position,
            depth_unproject: ParticleScene::perspective_depth_unproject(projection),
            viewport: [size.0, size.1],
        }
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
        let world_view = self
            .world_view
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("the world view is unavailable"))?;
        let target = CaptureTarget::new(ctx.device, size, ctx.format)?;
        let frame = WorldFrame {
            camera: camera.world_camera([size, size]),
            world: self.simulation.as_ref().map(|simulation| &simulation.world),
            time_seconds: self.render_time_seconds,
            settings: WorldViewSettings {
                debug_mode,
                compositor_debug_mode: self.compositor_debug_mode,
                bump_power: self.bump_power,
                lod: None,
                foliage_fade: None,
                details: include_details,
                scenario_tone_map: debug_mode == 0 || debug_mode == 12,
            },
        };
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Terrain Validation Capture Encoder"),
            });
        world_view.render_offscreen(ctx.device, ctx.queue, &mut encoder, &frame, &target.view);
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

        let mut result = self.capture_frame(
            ctx,
            config.size,
            camera,
            config.debug_mode,
            &config.output_path,
            true,
        );
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
        if let Some(world_view) = &mut self.world_view {
            world_view.end_offscreen(ctx.device);
        }
    }

    /// Recreates the GPU resources that depend on the window size.
    pub(super) fn resize_gpu_resources(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if let Some(world_view) = &mut self.world_view {
            world_view.resize(device, width, height);
        }
    }

    /// Records the GPU commands of one frame.
    pub(super) fn render_scene(&mut self, ctx: &mut RenderContext<'_>) {
        self.initialize_world_view(ctx);
        if self.world_view.is_none() {
            return;
        }
        self.sync_unit_roster(ctx);
        let capture_camera = self.capture_camera(ctx.size);
        let camera = capture_camera.map_or_else(
            || self.interactive_camera(ctx.size),
            |camera| camera.world_camera([ctx.size.0, ctx.size.1]),
        );
        let frame = WorldFrame {
            camera,
            world: self.simulation.as_ref().map(|simulation| &simulation.world),
            time_seconds: self.render_time_seconds,
            settings: WorldViewSettings {
                debug_mode: self.debug_mode,
                compositor_debug_mode: self.compositor_debug_mode,
                bump_power: self.bump_power,
                // A capture camera is fixed, so the capture keeps the initial chunk LOD.
                lod: self.capture.is_none().then_some(&self.lod_config),
                foliage_fade: capture_camera
                    .map(|camera| [camera.foliage_fade_start, camera.foliage_fade_start + 1.0]),
                details: true,
                scenario_tone_map: true,
            },
        };
        let Some(world_view) = &mut self.world_view else {
            return;
        };
        let camera_shakes = world_view.update(ctx.device, ctx.queue, &frame);
        let dirty_chunks = world_view.dirty_terrain_chunk_count();
        self.begin_camera_shakes(camera.light_view(), camera_shakes);
        if self.capture.as_ref().is_some_and(CaptureState::is_pending) {
            if dirty_chunks == 0 {
                self.capture_top_down(ctx);
            }
            return;
        }
        if let Some(world_view) = &self.world_view {
            world_view.encode(ctx.encoder, ctx.view);
        }
    }
}
