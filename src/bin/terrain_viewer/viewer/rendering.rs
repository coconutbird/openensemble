//! 3D rendering for the terrain viewer.
//!
//! Gives the loaded scenario to a `WorldView`, selects the camera of each frame and runs the
//! top-down captures. The `Application` callbacks call `resize_gpu_resources` and
//! `render_scene`.

use openensemble::session::{begin_camera_shakes, world_camera};
use render::app::RenderContext;
use render::terrain::RawXtdData;
use render::wgpu;
use render::world_view::{WorldFrame, WorldViewSettings};

use super::TerrainViewer;
use crate::capture::{
    CaptureState, CaptureTarget, ValidationCamera, foliage_camera, top_down_camera,
    top_down_detail_camera, write_foliage_placement_reference, write_foliage_references,
    write_packed_height_reference, write_tessellation_reference, write_xtt_reference,
};

impl TerrainViewer {
    fn initialize_world_view(&mut self, ctx: &RenderContext<'_>) {
        if self.load_error.is_some() {
            return;
        }
        let Some(session) = &self.session else {
            return;
        };
        if session.world_view.is_some() {
            return;
        }
        if let Some(raw_data) = &session.scene.raw_xtd_data {
            self.write_decoded_capture_references(raw_data);
        }

        let settings = WorldViewSettings {
            debug_mode: self.debug_mode,
            compositor_debug_mode: self.compositor_debug_mode,
            bump_power: self.bump_power,
            lod: None,
            foliage_fade: None,
            details: true,
            scenario_tone_map: true,
        };
        let Some(session) = &mut self.session else {
            return;
        };
        if let Err(error) = session.create_world_view(
            ctx.device,
            ctx.queue,
            [ctx.size.0, ctx.size.1],
            ctx.format,
            &settings,
            None,
        ) {
            log::error!("{error}");
            self.load_error = Some(error.to_string());
        }
    }

    fn write_decoded_capture_references(&self, raw_data: &RawXtdData) {
        let (Some(capture), Some(session)) = (&self.capture, &self.session) else {
            return;
        };
        let scene = &session.scene;
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

    fn capture_camera(&self, size: (u32, u32)) -> Option<ValidationCamera> {
        let config = self
            .capture
            .as_ref()
            .filter(|capture| capture.is_pending())?
            .config();
        let scene = &self.session.as_ref()?.scene;
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
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("the world view is unavailable"))?;
        let world = session
            .simulation
            .as_ref()
            .map(|simulation| &simulation.world);
        let world_view = session
            .world_view
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("the world view is unavailable"))?;
        let target = CaptureTarget::new(ctx.device, size, ctx.format)?;
        let frame = WorldFrame {
            camera: camera.world_camera([size, size]),
            world,
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
            selection_decals: &[],
            hp_bars: &[],
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
            .session
            .as_ref()
            .map(|session| &session.scene)
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
        if let Some(world_view) = self
            .session
            .as_mut()
            .and_then(|session| session.world_view.as_mut())
        {
            world_view.end_offscreen(ctx.device);
        }
    }

    /// Recreates the GPU resources that depend on the window size.
    pub(super) fn resize_gpu_resources(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if let Some(session) = &mut self.session {
            session.resize(device, width, height);
        }
    }

    /// Records the GPU commands of one frame.
    pub(super) fn render_scene(&mut self, ctx: &mut RenderContext<'_>) {
        self.initialize_world_view(ctx);
        let capture_camera = self.capture_camera(ctx.size);
        let camera = capture_camera.map_or_else(
            || world_camera(&self.camera, ctx.size),
            |camera| camera.world_camera([ctx.size.0, ctx.size.1]),
        );
        let Some(session) = &mut self.session else {
            return;
        };
        if session.world_view.is_none() {
            return;
        }
        session.sync_world_view_units(ctx.device, ctx.queue);
        let frame = WorldFrame {
            camera,
            world: session
                .simulation
                .as_ref()
                .map(|simulation| &simulation.world),
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
            selection_decals: &[],
            hp_bars: &[],
        };
        let Some(world_view) = &mut session.world_view else {
            return;
        };
        let camera_shakes = world_view.update(ctx.device, ctx.queue, &frame);
        let dirty_chunks = world_view.dirty_terrain_chunk_count();
        begin_camera_shakes(
            &mut self.camera_adapter,
            camera.light_view(),
            camera_shakes,
            self.render_time_seconds,
        );
        if self.capture.as_ref().is_some_and(CaptureState::is_pending) {
            if dirty_chunks == 0 {
                self.capture_top_down(ctx);
            }
            return;
        }
        if let Some(world_view) = &session.world_view {
            world_view.encode(ctx.encoder, ctx.view);
        }
    }
}
