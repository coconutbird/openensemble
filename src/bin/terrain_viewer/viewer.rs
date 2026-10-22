//! `TerrainViewer` struct and implementation.
//!
//! Split into sub-modules:
//! - `input` — keyboard/mouse handling, debug mode switching, UI
//! - `rendering` — `WorldView` creation, frame cameras and captures

mod input;
mod rendering;

use std::path::{Path, PathBuf};

use glam::Vec3;
use pipeline::hw1;
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xtd;
use pipeline::xtt;
use render::environment::EnvironmentMap;
use render::terrain::{Camera, LodConfig, SimulationCameraAdapter, TerrainScene};
use render::ugx::{Unit as UgxUnit, UnitScene as UgxUnitScene};
use render::ui::SimulationTimerAdapter;
use render::world_view::WorldView;

use crate::capture::{CaptureConfig, CaptureState};
use crate::frame_stats::FrameStats;

struct TerrainLoadInputs {
    terrain: xtd::XtdFile,
    textures: Option<xtt::XttFile>,
    lightset: Option<hw1::LightSetData>,
    environment: Option<EnvironmentMap>,
    sky: Option<UgxUnit>,
    source: Option<AssetSource<StdFileProvider>>,
    simulation: Option<sim::LoadedScenario>,
    content: Option<pipeline::hw1::World>,
    ugx_scene: Option<UgxUnitScene>,
}

/// The terrain viewer application.
pub struct TerrainViewer {
    pub xtd_path: Option<PathBuf>,
    /// Scenario name (for ERA loading).
    pub scenario_name: Option<String>,
    /// Asset source for loading textures from ERA archives.
    pub asset_source: Option<AssetSource<StdFileProvider>>,
    /// Authoritative game state loaded by `sim` from the scenario database.
    pub simulation: Option<sim::LoadedScenario>,
    /// Database and presentation catalog layered for the active scenario.
    pub game_content: Option<pipeline::hw1::World>,
    /// Fixed-step clock and command queue advancing the authoritative world.
    pub simulation_clock: sim::Simulation,
    /// All decoded terrain data (mesh, textures, splat, decals, foliage, roads).
    pub scene: Option<TerrainScene>,
    /// Scenario GLS/FLS constants. The `WorldView` takes them when the viewer creates it.
    pub lightset: Option<hw1::LightSetData>,
    /// Scenario-global HDR environment cubemap used by reflective materials.
    pub environment: Option<EnvironmentMap>,
    /// Authored scenario sky visual, resolved directly through VIS/UGX.
    pub sky_unit: Option<UgxUnit>,
    pub camera: Camera,
    /// Renderer-local application state for sim-authored camera directives.
    pub camera_adapter: SimulationCameraAdapter,
    /// Renderer-local selection for the sim-owned single timer widget.
    pub timer_adapter: SimulationTimerAdapter,
    pub show_info: bool,
    pub wireframe: bool,
    pub load_error: Option<String>,
    /// GPU resources that draw the loaded scenario.
    pub world_view: Option<WorldView>,
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
    /// Visual assets bound to entities in the authoritative simulation world.
    pub ugx_scene: Option<UgxUnitScene>,
    /// Whether the GPU placement roster must be synchronized from `ugx_scene`.
    pub ugx_roster_dirty: bool,
    /// Optional deterministic top-down capture-and-exit state.
    pub capture: Option<CaptureState>,
    /// Frame, simulation and render timings for the information window and the log.
    pub frame_stats: FrameStats,
    /// Game time of the last trigger loop-guard warning, so that the log gets one warning per
    /// second at most.
    pub trigger_loop_warning_time_ms: Option<u32>,
}

impl TerrainViewer {
    fn defaults() -> Self {
        Self {
            xtd_path: None,
            scenario_name: None,
            asset_source: None,
            simulation: None,
            game_content: None,
            simulation_clock: sim::Simulation::new(),
            scene: None,
            lightset: None,
            environment: None,
            sky_unit: None,
            camera: Camera::default(),
            camera_adapter: SimulationCameraAdapter::default(),
            timer_adapter: SimulationTimerAdapter::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            world_view: None,
            render_time_seconds: 0.0,
            debug_mode: 0,
            bump_power: 1.0,
            compositor_debug_mode: 0,
            lod_config: LodConfig::default(),
            ugx_scene: None,
            ugx_roster_dirty: false,
            capture: None,
            frame_stats: FrameStats::default(),
            trigger_loop_warning_time_ms: None,
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

    fn load_terrain(&mut self) {
        let inputs = if let Some(scenario_name) = &self.scenario_name {
            load_scenario_inputs(scenario_name)
        } else if let Some(path) = &self.xtd_path {
            load_file_inputs(path)
        } else {
            Err("No terrain source specified".to_owned())
        };
        let inputs = match inputs {
            Ok(inputs) => inputs,
            Err(error) => {
                log::error!("{error}");
                self.load_error = Some(error);
                return;
            }
        };

        // Store asset source for road texture loading later
        self.asset_source = inputs.source;
        self.simulation = inputs.simulation;
        self.game_content = inputs.content;
        self.simulation_clock.reset();
        self.timer_adapter.reset();
        if self.simulation.is_some() {
            self.simulation_clock.start();
        }
        self.lightset = inputs.lightset;
        self.environment = inputs.environment;
        self.sky_unit = inputs.sky;
        self.world_view = None;
        if let Some(lightset) = &self.lightset {
            self.bump_power = lightset.terrain_bump_strength;
        }
        self.ugx_scene = inputs.ugx_scene;
        self.ugx_roster_dirty = false;

        // Load entire terrain scene in one call
        let first_load = self.scene.is_none();
        match TerrainScene::load(
            &inputs.terrain,
            inputs.textures.as_ref(),
            self.asset_source.as_mut(),
        ) {
            Ok(scene) => {
                let hover_point = scene.mesh.center();
                if first_load {
                    self.camera.position = hover_point + Vec3::new(0.0, 200.0, -300.0);
                }
                self.camera_adapter.reset(&mut self.camera, hover_point);
                self.scene = Some(scene);
                self.load_error = None;
            }
            Err(e) => {
                self.load_error = Some(e.clone());
                log::error!("{e}");
            }
        }
    }
}

fn load_scenario_inputs(scenario_name: &str) -> Result<TerrainLoadInputs, String> {
    log::info!("Loading scenario database and game state via sim: {scenario_name}");
    let dir = openensemble::game_dir();
    let dir_str = dir.to_string_lossy();
    // Default lobby: a human UNSC Cutter (team 1) against a computer AI Covenant Arbiter
    // (team 2).
    let settings = sim::GameSettings::skirmish_1v1(0);
    let sim::LoadedGameScenario {
        simulation,
        mut content,
        mut source,
    } = sim::load_scenario_from_game_dir(&dir_str, scenario_name, &settings)
        .map_err(|error| format!("Failed to load simulation scenario: {error}"))?;
    log::info!(
        "Entered sim scenario with {} players, {} initial bases, {} squads, {} units/buildings, and {} projectiles",
        simulation.world.player_count(),
        simulation.initial_base_ids.len(),
        simulation.world.squads.len(),
        simulation.world.units.len(),
        simulation.world.projectiles.len(),
    );
    for player in simulation.world.players() {
        log::info!(
            "Player {} '{}': {:?} civ {} leader {} team {} state {:?}",
            player.id,
            player.name,
            player.player_type,
            player.civ_id,
            player.leader_id,
            player.team_id,
            player.state,
        );
    }

    let sky = content
        .manifest
        .sky_ref
        .as_deref()
        .map(|reference| load_sky_unit(&mut source, reference))
        .transpose()?;
    if let (Some(reference), Some(sky)) = (&content.manifest.sky_ref, &sky) {
        log::info!(
            "Loaded scenario sky '{}': {} components, {} triangles",
            reference,
            sky.component_count(),
            sky.triangle_count(),
        );
    }
    let environment = content
        .manifest
        .terrain_env_ref
        .as_deref()
        .map(|path| EnvironmentMap::load(&mut source, path))
        .transpose()
        .map_err(|error| format!("Failed to load scenario environment: {error}"))?;
    if let Some(environment) = &environment {
        log::info!(
            "Loaded scenario environment {} ({}x{}, {} mips, HDR scale {})",
            environment.path(),
            environment.size(),
            environment.size(),
            environment.mip_count(),
            environment.hdr_scale(),
        );
    }
    let active_proto_names =
        render::ugx::simulation_proto_names(&simulation.world).collect::<Vec<_>>();
    let loaded_visuals = content.load_visuals_for(&mut source, active_proto_names.iter().copied());
    log::info!("Loaded {loaded_visuals} active proto visual definitions");
    let ugx_scene = UgxUnitScene::load_world_with_gameplay(
        &mut source,
        &simulation.world,
        &simulation.gameplay,
        &content.visuals,
        &content.database.objects,
    );
    log_ugx_scene_summary(&ugx_scene);

    let terrain = content.terrain_data.take().ok_or_else(|| {
        format!("World loaded but no XTD terrain data for scenario '{scenario_name}'")
    })?;
    Ok(TerrainLoadInputs {
        terrain,
        textures: content.terrain_textures.take(),
        lightset: content.lightset.take(),
        environment,
        sky,
        source: Some(source),
        simulation: Some(simulation),
        content: Some(content),
        ugx_scene: Some(ugx_scene),
    })
}

fn log_ugx_scene_summary(scene: &UgxUnitScene) {
    log::info!(
        "Resolved simulation UGX scene: {} placements from {} sim entities across {} visuals ({} NoRender, {} missing visuals, {} missing prototypes, {} invalid transforms, {} asset load failures)",
        scene.placement_count(),
        scene.simulation_entity_count(),
        scene.unique_visual_count(),
        scene.skipped_no_render_count(),
        scene.missing_visual_count(),
        scene.missing_proto_count(),
        scene.invalid_transform_count(),
        scene.load_failure_count(),
    );
    for issue in scene.issues() {
        log::warn!(
            "Scenario visual '{}' was skipped: {}",
            issue.proto_name(),
            issue.reason()
        );
    }
    log::info!(
        "Resolved {} PFX graphs ({} issues) and {} LGT graphs ({} issues)",
        scene.particle_effect_count(),
        scene.particle_effect_issue_count(),
        scene.light_effect_count(),
        scene.light_effect_issue_count(),
    );
    for issue in scene.light_effect_issues() {
        log::warn!("Scenario LGT attachment was skipped: {issue}");
    }
}

fn load_file_inputs(path: &Path) -> Result<TerrainLoadInputs, String> {
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
    Ok(TerrainLoadInputs {
        terrain,
        textures,
        lightset: None,
        environment: None,
        sky: None,
        source: None,
        simulation: None,
        content: None,
        ugx_scene: None,
    })
}

fn load_sky_unit(
    source: &mut AssetSource<StdFileProvider>,
    reference: &str,
) -> Result<UgxUnit, String> {
    let path = canonical_sky_visual_path(reference);
    let document = source
        .read_xmb(&path)
        .ok_or_else(|| format!("scenario sky visual was not found: {path}"))?;
    let visual = pipeline::database::hw1::visual::parse(&document)
        .map_err(|error| format!("failed to parse scenario sky visual '{path}': {error}"))?;
    UgxUnit::load(source, &visual)
        .map_err(|error| format!("failed to load scenario sky visual '{path}': {error}"))
}

fn canonical_sky_visual_path(reference: &str) -> String {
    let mut normalized = reference
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if normalized.to_ascii_lowercase().ends_with(".xmb") {
        normalized.truncate(normalized.len() - 4);
    }
    if !normalized.to_ascii_lowercase().ends_with(".vis") {
        normalized.push_str(".vis");
    }
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}
