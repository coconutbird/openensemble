//! `TerrainViewer` struct and implementation.
//!
//! Split into sub-modules:
//! - `input` — keyboard/mouse handling, debug mode switching, UI
//! - `rendering` — `WorldView` creation, frame cameras and captures

mod input;
mod rendering;

use std::path::{Path, PathBuf};

use glam::Vec3;
use openensemble::frame_stats::FrameStats;
use openensemble::session::{ScenarioInputs, ScenarioSession};
use pipeline::xtd;
use pipeline::xtt;
use render::terrain::{Camera, LodConfig, SimulationCameraAdapter};
use render::ui::SimulationTimerAdapter;

use crate::capture::{CaptureConfig, CaptureState};

/// The terrain viewer application.
pub struct TerrainViewer {
    pub xtd_path: Option<PathBuf>,
    /// Scenario name (for ERA loading).
    pub scenario_name: Option<String>,
    /// The loaded scenario: terrain, simulation, unit visuals and the `WorldView`.
    pub session: Option<ScenarioSession>,
    pub camera: Camera,
    /// Renderer-local application state for sim-authored camera directives.
    pub camera_adapter: SimulationCameraAdapter,
    /// Renderer-local selection for the sim-owned single timer widget.
    pub timer_adapter: SimulationTimerAdapter,
    pub show_info: bool,
    pub wireframe: bool,
    pub load_error: Option<String>,
    /// Monotonic render time used by authored material and foliage animation.
    pub render_time_seconds: f32,
    /// Display mode. Mode 0 is the retail-style lit terrain view.
    pub debug_mode: u32,
    /// Normal map strength (gBumpPower in game, scales XY components).
    pub bump_power: f32,
    /// Debug mode for compositor: 0=normal, 1=UV, 2=chunk ID, 3=alpha, 4=layer0.
    pub compositor_debug_mode: u32,
    /// LOD configuration for distance-based compositing quality.
    pub lod_config: LodConfig,
    /// Optional deterministic top-down capture-and-exit state.
    pub capture: Option<CaptureState>,
    /// Frame, simulation and render timings for the information window and the log.
    pub frame_stats: FrameStats,
}

impl TerrainViewer {
    fn defaults() -> Self {
        Self {
            xtd_path: None,
            scenario_name: None,
            session: None,
            camera: Camera::default(),
            camera_adapter: SimulationCameraAdapter::default(),
            timer_adapter: SimulationTimerAdapter::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            render_time_seconds: 0.0,
            debug_mode: 0,
            bump_power: 1.0,
            compositor_debug_mode: 0,
            lod_config: LodConfig::default(),
            capture: None,
            frame_stats: FrameStats::default(),
        }
    }

    /// Create a terrain viewer from a file path (legacy mode).
    pub fn new(xtd_path: Option<PathBuf>) -> Self {
        Self {
            xtd_path,
            ..Self::defaults()
        }
    }

    /// Create a terrain viewer from a scenario name (loads from game directory/ERA).
    pub fn from_scenario(scenario_name: String) -> Self {
        Self {
            scenario_name: Some(scenario_name),
            ..Self::defaults()
        }
    }

    /// Configures this viewer to write one top-down GPU capture and exit.
    #[must_use]
    pub fn with_capture(mut self, config: CaptureConfig) -> Self {
        self.show_info = false;
        self.capture = Some(CaptureState::new(config));
        self
    }

    /// Returns the authoritative simulation, if the loaded scenario has one.
    fn simulation(&self) -> Option<&sim::LoadedScenario> {
        self.session.as_ref()?.simulation.as_ref()
    }

    fn load_terrain(&mut self) {
        let inputs = if let Some(scenario_name) = &self.scenario_name {
            // Default lobby: a human UNSC Cutter (team 1) against a computer AI Covenant Arbiter
            // (team 2).
            ScenarioInputs::load(scenario_name, &sim::GameSettings::skirmish_1v1(0))
        } else if let Some(path) = &self.xtd_path {
            load_file_inputs(path)
        } else {
            Err("No terrain source specified".to_owned())
        };
        let session = inputs.and_then(ScenarioSession::new);
        self.timer_adapter.reset();
        match session {
            Ok(session) => {
                if let Some(lightset) = &session.lightset {
                    self.bump_power = lightset.terrain_bump_strength;
                }
                let hover_point = session.scene.mesh.center();
                if self.session.is_none() {
                    self.camera.position = hover_point + Vec3::new(0.0, 200.0, -300.0);
                }
                self.camera_adapter.reset(&mut self.camera, hover_point);
                self.session = Some(session);
                self.load_error = None;
            }
            Err(error) => {
                log::error!("{error}");
                self.session = None;
                self.load_error = Some(error);
            }
        }
    }
}

fn load_file_inputs(path: &Path) -> Result<ScenarioInputs, String> {
    log::info!("Loading XTD from file: {}", path.display());
    let data = std::fs::read(path).map_err(|error| format!("Failed to read file: {error}"))?;
    let terrain =
        xtd::Reader::read(&data).map_err(|error| format!("Failed to parse XTD: {error}"))?;
    let xtt_path = path.with_extension("xtt");
    let textures = xtt_path
        .exists()
        .then(|| std::fs::read(&xtt_path).ok())
        .flatten()
        .and_then(|data| xtt::Reader::read(&data).ok());
    Ok(ScenarioInputs {
        terrain,
        textures,
        lightset: None,
        environment: None,
        sky: None,
        source: None,
        simulation: None,
        content: None,
        units: None,
    })
}
