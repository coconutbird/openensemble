//! The `WorldView` creation and the frame draw of the client.

use openensemble::session::{begin_camera_shakes, world_camera};
use render::app::RenderContext;
use render::terrain::LodConfig;
use render::world_view::{WorldFrame, WorldViewSettings};

use super::Client;

/// Terrain display mode 0: the retail lit view.
const RETAIL_DISPLAY_MODE: u32 = 0;
/// Terrain compositor display mode 0: the normal composite.
const NORMAL_COMPOSITE_MODE: u32 = 0;

/// Returns the display settings of a frame. `lod` gives the compositor LOD bands.
fn world_view_settings(bump_power: f32, lod: Option<&LodConfig>) -> WorldViewSettings<'_> {
    WorldViewSettings {
        debug_mode: RETAIL_DISPLAY_MODE,
        compositor_debug_mode: NORMAL_COMPOSITE_MODE,
        bump_power,
        lod,
        foliage_fade: None,
        details: true,
        scenario_tone_map: true,
    }
}

impl Client {
    /// Makes the `WorldView` on the first frame. A GPU error stops the draw.
    fn initialize_world_view(&mut self, ctx: &RenderContext<'_>) {
        if self.load_error.is_some() {
            return;
        }
        let settings = world_view_settings(self.bump_power, None);
        let Some(session) = &mut self.session else {
            return;
        };
        if let Err(error) = session.create_world_view(
            ctx.device,
            ctx.queue,
            [ctx.size.0, ctx.size.1],
            ctx.format,
            &settings,
        ) {
            log::error!("{error}");
            self.load_error = Some(error.to_string());
        }
    }

    /// Updates and records the scenario draw of one frame.
    pub(super) fn render_world(&mut self, ctx: &mut RenderContext<'_>) {
        self.initialize_world_view(ctx);
        if self.load_error.is_some() {
            return;
        }
        let camera = world_camera(&self.camera, ctx.size);
        let settings = world_view_settings(self.bump_power, Some(&self.lod_config));
        let Some(session) = &mut self.session else {
            return;
        };
        session.sync_world_view_units(ctx.device, ctx.queue);
        let frame = WorldFrame {
            camera,
            world: session
                .simulation
                .as_ref()
                .map(|simulation| &simulation.world),
            time_seconds: self.render_time_seconds,
            settings,
            selection_decals: &[],
            hp_bars: &[],
        };
        let Some(world_view) = &mut session.world_view else {
            return;
        };
        let camera_shakes = world_view.update(ctx.device, ctx.queue, &frame);
        begin_camera_shakes(
            &mut self.camera_adapter,
            camera.light_view(),
            camera_shakes,
            self.render_time_seconds,
        );
        world_view.encode(ctx.encoder, ctx.view);
    }
}
