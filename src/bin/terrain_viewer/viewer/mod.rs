//! TerrainViewer struct and implementation.
//!
//! Split into sub-modules:
//! - `input` — keyboard/mouse handling, debug mode switching, UI
//! - `render` — GPU initialization and render pass logic

mod input;
mod rendering;

use std::path::PathBuf;

use glam::Vec3;
use pipeline::hw1;
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xtd;
use pipeline::xtt;
use render::terrain::{Camera, CompositorResources, LodConfig, TerrainScene, TessellationMode};
use render::wgpu;

use crate::types::GpuResources;

/// The terrain viewer application.
pub struct TerrainViewer {
    pub xtd_path: Option<PathBuf>,
    /// Scenario name (for ERA loading).
    pub scenario_name: Option<String>,
    /// Asset source for loading textures from ERA archives.
    pub asset_source: Option<AssetSource<StdFileProvider>>,
    /// All decoded terrain data (mesh, textures, splat, decals, foliage, roads).
    pub scene: Option<TerrainScene>,
    pub camera: Camera,
    pub show_info: bool,
    pub wireframe: bool,
    pub load_error: Option<String>,
    pub gpu: Option<GpuResources>,
    pub surface_format: wgpu::TextureFormat,
    /// Debug mode: 0=normal, 1=alpha values, 2=in-chunk UV, 3=raw atlas, 4=terrain UV
    pub debug_mode: u32,
    /// Tessellation mode: None, CPU, or GPU.
    pub tessellation_mode: TessellationMode,
    /// Normal map strength (gBumpPower in game, scales XY components).
    pub bump_power: f32,
    /// GPU terrain texture compositor (for pre-baked chunk textures).
    pub compositor: Option<CompositorResources>,
    /// Bind group for compositor (separate from main texture bind group).
    pub compositor_bind_group: Option<wgpu::BindGroup>,
    /// Whether to use GPU compositing (vs runtime splatting).
    pub use_gpu_compositing: bool,
    /// Debug mode for compositor: 0=normal, 1=UV, 2=chunk ID, 3=alpha, 4=layer0.
    pub compositor_debug_mode: u32,
    /// LOD configuration for distance-based compositing quality.
    pub lod_config: LodConfig,
    /// Pre-calculated chunk center positions [x, y, z] for LOD calculations.
    pub chunk_centers: Vec<[f32; 3]>,
    /// Foliage GPU resources (pipeline, textures, bind groups).
    pub foliage_resources: Option<crate::foliage::FoliageResources>,
    /// Shadow map resources (pipeline, depth texture, light VP).
    pub shadow_resources: Option<crate::shadow::ShadowResources>,
    /// Road GPU resources (pipeline, vertex buffer, textures).
    pub road_resources: Option<crate::roads::RoadResources>,
}

impl TerrainViewer {
    fn defaults() -> Self {
        Self {
            xtd_path: None,
            scenario_name: None,
            asset_source: None,
            scene: None,
            camera: Camera::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            gpu: None,
            surface_format: wgpu::TextureFormat::Bgra8UnormSrgb,
            debug_mode: 12,
            tessellation_mode: TessellationMode::Gpu,
            bump_power: 1.0,
            compositor: None,
            compositor_bind_group: None,
            use_gpu_compositing: true,
            compositor_debug_mode: 0,
            lod_config: LodConfig::default(),
            chunk_centers: Vec::new(),
            foliage_resources: None,
            shadow_resources: None,
            road_resources: None,
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

    fn load_terrain(&mut self) {
        // Determine which loading path to use
        let (xtd_file, xtt_file, asset_source_opt) =
            if let Some(scenario_name) = &self.scenario_name {
                log::info!("Loading scenario via pipeline::World: {}", scenario_name);
                let dir = data::paths::game_dir();
                let dir_str = dir.to_string_lossy();

                let (mut world, mut src) = match hw1::World::load(&dir_str) {
                    Ok(ws) => ws,
                    Err(e) => {
                        self.load_error = Some(format!("Failed to load World: {}", e));
                        log::error!("{}", self.load_error.as_ref().unwrap());
                        return;
                    }
                };
                world.swap_scenario(&mut src, scenario_name);

                let Some(xtd) = world.terrain_data else {
                    self.load_error = Some(format!(
                        "World loaded but no XTD terrain data for scenario '{}'",
                        scenario_name
                    ));
                    log::error!("{}", self.load_error.as_ref().unwrap());
                    return;
                };

                (xtd, world.terrain_textures, Some(src))
            } else if let Some(path) = &self.xtd_path {
                // Load from file path (legacy mode — no ERA, no World)
                log::info!("Loading XTD from file: {}", path.display());
                match std::fs::read(path) {
                    Ok(data) => match xtd::Reader::read(&data) {
                        Ok(xtd) => {
                            let xtt_path = path.with_extension("xtt");
                            let xtt = if xtt_path.exists() {
                                std::fs::read(&xtt_path)
                                    .ok()
                                    .and_then(|d| xtt::Reader::read(&d).ok())
                            } else {
                                None
                            };
                            (xtd, xtt, None)
                        }
                        Err(e) => {
                            self.load_error = Some(format!("Failed to parse XTD: {}", e));
                            log::error!("{}", self.load_error.as_ref().unwrap());
                            return;
                        }
                    },
                    Err(e) => {
                        self.load_error = Some(format!("Failed to read file: {}", e));
                        log::error!("{}", self.load_error.as_ref().unwrap());
                        return;
                    }
                }
            } else {
                self.load_error = Some("No terrain source specified".to_string());
                return;
            };

        // Store asset source for road texture loading later
        self.asset_source = asset_source_opt;

        // Load entire terrain scene in one call
        let first_load = self.scene.is_none();
        match TerrainScene::load(
            &xtd_file,
            xtt_file.as_ref(),
            self.asset_source.as_mut(),
            self.tessellation_mode,
        ) {
            Ok(scene) => {
                if first_load {
                    self.camera.position = scene.mesh.center() + Vec3::new(0.0, 200.0, -300.0);
                }
                self.scene = Some(scene);
                self.load_error = None;
            }
            Err(e) => {
                self.load_error = Some(e.clone());
                log::error!("{}", e);
            }
        }
    }
}
