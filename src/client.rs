//! The game client: one skirmish scenario that the local human player plays.
//!
//! [`Client`] owns the [`ScenarioSession`] and the client state of the local player. It
//! implements the `Application` trait of `render`. Each module has one topic:
//!
//! - `camera`: the RTS camera with the retail camera values.
//! - `controls`: the keyboard and mouse camera controls.
//! - `view`: the `WorldView` creation and the frame draw.
//! - `hud`: the egui interface (now the frame-rate line).
//!
//! Selection, hover, orders, overlays (selection decals and HP bars) and the full HUD go into
//! new modules next to these. Their state goes into [`Client`].

mod camera;
mod controls;
mod hud;
mod selection;
mod view;

use std::time::Instant;

use glam::Vec3;
use openensemble::frame_stats::FrameStats;
use openensemble::session::{ScenarioInputs, ScenarioSession};
use render::app::{Application, Color, FrameContext, RenderContext};
use render::input::{Input, KeyCode};
use render::terrain::{Camera, LodConfig, SimulationCameraAdapter};
use render::wgpu;

use camera::RtsCamera;
use controls::CameraControls;
use selection::Selection;

/// The lobby of a client game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClientOptions {
    /// The scenario name, for example `blood_gulch`.
    pub(crate) scenario: String,
    /// The random seed of the game settings.
    pub(crate) seed: u32,
}

/// The game client application.
pub(crate) struct Client {
    options: ClientOptions,
    /// The loaded scenario. `None` before the load and after a load error.
    session: Option<ScenarioSession>,
    /// The message of the last load or GPU error.
    load_error: Option<String>,
    /// The simulation player of the local human.
    local_player: sim::PlayerId,
    /// The units that the local player selected.
    pub(crate) selection: Selection,
    /// The client-owned RTS camera state.
    rts_camera: RtsCamera,
    /// The mouse state of the camera controls.
    controls: CameraControls,
    /// The render camera that `rts_camera` and the simulation directives set.
    camera: Camera,
    /// Renderer-local state for the camera directives and shakes of the simulation.
    camera_adapter: SimulationCameraAdapter,
    /// The zoom distance of `camera_adapter`. A directive puts the eye at this distance.
    adapter_zoom: f32,
    /// Distance bands of the terrain compositor LOD.
    lod_config: LodConfig,
    /// Normal map strength (`gBumpPower`). The scenario lightset sets it.
    bump_power: f32,
    /// Monotonic render time for animations, in seconds.
    render_time_seconds: f32,
    /// Frame, simulation and render timings for the frame-rate line.
    frame_stats: FrameStats,
}

impl Client {
    /// Makes a client that loads the scenario of `options` in `Application::init`.
    pub(crate) fn new(options: ClientOptions) -> Self {
        Self {
            options,
            session: None,
            load_error: None,
            local_player: 0,
            selection: Selection::default(),
            rts_camera: RtsCamera::new(Vec3::ZERO),
            controls: CameraControls::default(),
            camera: Camera::default(),
            camera_adapter: SimulationCameraAdapter::default(),
            adapter_zoom: camera::ZOOM_DEFAULT,
            lod_config: LodConfig::default(),
            bump_power: 1.0,
            render_time_seconds: 0.0,
            frame_stats: FrameStats::default(),
        }
    }

    /// Loads the scenario, finds the local player and puts the camera over the player base.
    fn load(&mut self) -> Result<(), String> {
        let settings = sim::GameSettings::skirmish_1v1(self.options.seed);
        let inputs = ScenarioInputs::load(&self.options.scenario, &settings)?;
        let session = ScenarioSession::new(inputs)?;
        let simulation = session
            .simulation
            .as_ref()
            .ok_or("the scenario has no simulation")?;
        let world = &simulation.world;
        let local_player = world
            .active_players()
            .find(|player| player.player_type == sim::PlayerType::Human)
            .ok_or("the scenario has no human player")?;
        let local_player = local_player.id;
        let start = simulation
            .get_initial_base_id(local_player)
            .and_then(|base_id| world.get_base(base_id))
            .map(|base| base.position)
            .or_else(|| {
                world
                    .squads
                    .iter()
                    .find(|(_, squad)| squad.base.player_id == local_player)
                    .map(|(_, squad)| squad.base.position)
            })
            .unwrap_or_else(|| session.scene.mesh.center());
        log::info!(
            "Local player {local_player}; camera starts at ({:.1}, {:.1}, {:.1})",
            start.x,
            start.y,
            start.z
        );

        self.rts_camera = RtsCamera::new(start);
        self.rts_camera
            .snap_to_terrain(world.terrain_height(start, true));
        self.rts_camera.apply_to(&mut self.camera);
        self.camera_adapter
            .reset(&mut self.camera, self.rts_camera.hover_point());
        self.adapter_zoom = self.rts_camera.zoom();
        if let Some(lightset) = &session.lightset {
            self.bump_power = lightset.terrain_bump_strength;
        }
        self.local_player = local_player;
        self.session = Some(session);
        Ok(())
    }

    /// Applies the simulation camera directives and the input of one frame to the camera.
    fn update_camera(&mut self, input: &Input, ctx: &FrameContext) {
        let Some(world) = self.session.as_ref().and_then(ScenarioSession::world) else {
            return;
        };
        self.rts_camera.apply_to(&mut self.camera);
        let before = (self.camera.position, self.camera.yaw);
        let state = self.camera_adapter.synchronize_at_time(
            &mut self.camera,
            world,
            self.local_player,
            self.render_time_seconds,
        );
        if before != (self.camera.position, self.camera.yaw) {
            self.rts_camera
                .take_directive(&self.camera, self.adapter_zoom);
        }
        self.controls.update(
            &mut self.rts_camera,
            input,
            ctx.window_size,
            state,
            ctx.delta_time,
        );
        if let Some(bounds) = world.effective_playable_bounds() {
            self.rts_camera.clamp_to(bounds);
        }
        let hover_point = self.rts_camera.hover_point();
        self.rts_camera
            .follow_terrain(world.terrain_height(hover_point, true), ctx.delta_time);
        self.rts_camera.apply_to(&mut self.camera);
    }
}

impl Application for Client {
    fn init(&mut self) {
        log::info!("Loading scenario '{}'", self.options.scenario);
        if let Err(error) = self.load() {
            log::error!("{error}");
            self.load_error = Some(error);
        }
    }

    fn update(&mut self, input: &Input, ctx: &FrameContext) -> bool {
        // Retail `Clear` (Escape) clears the selection (`BUser_clearSelection`, `0x1405B1A10`).
        if input.is_key_pressed(KeyCode::Escape) && !self.selection.is_empty() {
            log::debug!(
                "Clear selection: {} squads, {} units, menu {:?}",
                self.selection.squads().len(),
                self.selection.units().len(),
                self.selection.menu_building()
            );
            self.selection.clear();
        }
        self.render_time_seconds += ctx.delta_time;
        self.frame_stats.record_frame(ctx.delta_time);
        let started = Instant::now();
        if let Some(session) = &mut self.session {
            session.advance(ctx.delta_time);
        }
        if let Some(world) = self.session.as_ref().and_then(ScenarioSession::world) {
            self.selection.rebuild_squads(world, |_| 0);
        }
        self.frame_stats.record_simulation(started.elapsed());
        self.update_camera(input, ctx);
        true
    }

    fn ui(&mut self, ctx: &egui::Context) {
        self.draw_hud(ctx);
    }

    fn render(&mut self, _ctx: &FrameContext) -> Color {
        Color::BLACK
    }

    fn render_3d(&mut self, ctx: &mut RenderContext<'_>) {
        let started = Instant::now();
        self.render_world(ctx);
        self.frame_stats.record_render(started.elapsed());
    }

    fn resize_gpu(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if let Some(session) = &mut self.session {
            session.resize(device, width, height);
        }
    }

    fn shutdown(&mut self) {
        if let Some(session) = &self.session {
            log::info!(
                "Client shutting down at simulation tick {}",
                session.clock.tick
            );
        }
    }
}
