//! `TerrainViewer` struct and implementation.
//!
//! Split into sub-modules:
//! - `input` — keyboard/mouse handling, debug mode switching, UI
//! - `render` — GPU initialization and render pass logic

mod input;
mod rendering;

use std::path::PathBuf;

use glam::{FloatExt, Mat4, Vec3};
use num_traits::ToPrimitive;
use pipeline::hw1;
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xtd;
use pipeline::xtt;
use render::terrain::{Camera, CompositorResources, LodConfig, TerrainScene};
use render::ugx::{Unit as UgxUnit, UnitRenderer as UgxUnitRenderer};
use render::wgpu;

use crate::capture::{CaptureConfig, CaptureState};
use crate::types::GpuResources;

const WARTHOG_VISUAL: &str = "unsc_veh_warthog_01";

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
    /// Display mode. Mode 12 is the canonical GPU-composited terrain view.
    pub debug_mode: u32,
    /// Normal map strength (gBumpPower in game, scales XY components).
    pub bump_power: f32,
    /// GPU terrain texture compositor (for pre-baked chunk textures).
    pub compositor: Option<CompositorResources>,
    /// Bind group for compositor (separate from main texture bind group).
    pub compositor_bind_group: Option<wgpu::BindGroup>,
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
    /// Decoded Warthog visual graph used to exercise unit rendering.
    pub ugx_unit: Option<UgxUnit>,
    /// GPU resources for every component in the centered Warthog.
    pub ugx_renderer: Option<UgxUnitRenderer>,
    /// Model-to-world placement for the Warthog.
    pub ugx_transform: Mat4,
    /// A model-loading failure does not prevent terrain diagnostics from running.
    pub ugx_error: Option<String>,
    /// Optional deterministic top-down capture-and-exit state.
    pub capture: Option<CaptureState>,
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
            bump_power: 1.0,
            compositor: None,
            compositor_bind_group: None,
            compositor_debug_mode: 0,
            lod_config: LodConfig::default(),
            chunk_centers: Vec::new(),
            foliage_resources: None,
            shadow_resources: None,
            road_resources: None,
            ugx_unit: None,
            ugx_renderer: None,
            ugx_transform: Mat4::IDENTITY,
            ugx_error: None,
            capture: None,
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
        // Determine which loading path to use
        let (terrain_file, texture_file, asset_source_opt, unit_visual) =
            if let Some(scenario_name) = &self.scenario_name {
                log::info!("Loading scenario via pipeline::World: {scenario_name}");
                let dir = data::paths::game_dir();
                let dir_str = dir.to_string_lossy();

                let (mut world, mut src) = match hw1::World::load(&dir_str) {
                    Ok(ws) => ws,
                    Err(e) => {
                        self.load_error = Some(format!("Failed to load World: {e}"));
                        log::error!("{}", self.load_error.as_ref().unwrap());
                        return;
                    }
                };
                world.swap_scenario(&mut src, scenario_name);
                let unit_visual = world.visuals.get(WARTHOG_VISUAL).cloned();

                let Some(xtd) = world.terrain_data else {
                    self.load_error = Some(format!(
                        "World loaded but no XTD terrain data for scenario '{scenario_name}'"
                    ));
                    log::error!("{}", self.load_error.as_ref().unwrap());
                    return;
                };

                (xtd, world.terrain_textures, Some(src), unit_visual)
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
                            (xtd, xtt, None, None)
                        }
                        Err(e) => {
                            self.load_error = Some(format!("Failed to parse XTD: {e}"));
                            log::error!("{}", self.load_error.as_ref().unwrap());
                            return;
                        }
                    },
                    Err(e) => {
                        self.load_error = Some(format!("Failed to read file: {e}"));
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
            &terrain_file,
            texture_file.as_ref(),
            self.asset_source.as_mut(),
        ) {
            Ok(scene) => {
                if first_load {
                    self.camera.position = scene.mesh.center() + Vec3::new(0.0, 200.0, -300.0);
                }
                self.scene = Some(scene);
                self.load_error = None;
                self.load_warthog(unit_visual.as_ref());
            }
            Err(e) => {
                self.load_error = Some(e.clone());
                log::error!("{e}");
            }
        }
    }

    fn load_warthog(&mut self, visual: Option<&pipeline::database::hw1::Visual>) {
        self.ugx_unit = None;
        self.ugx_renderer = None;
        self.ugx_error = None;
        let Some(scene) = &self.scene else { return };
        let Some(raw) = &scene.raw_xtd_data else {
            self.ugx_error =
                Some("packed terrain data is unavailable for UGX placement".to_owned());
            return;
        };
        let center = scene.mesh.center();
        let terrain_height = terrain_surface_height(raw, center.x, center.z).unwrap_or(center.y);
        let Some(source) = &mut self.asset_source else {
            log::info!("No ERA asset source is available; skipping the centered Warthog");
            return;
        };
        let Some(visual) = visual else {
            self.ugx_error = Some(format!("Visual definition not found: {WARTHOG_VISUAL}"));
            return;
        };
        match UgxUnit::load(source, visual) {
            Ok(unit) => {
                let bounds_min = Vec3::from_array(unit.bounds_min());
                let bounds_max = Vec3::from_array(unit.bounds_max());
                let model_center = Vec3::new(
                    f32::midpoint(bounds_min.x, bounds_max.x),
                    0.0,
                    f32::midpoint(bounds_min.z, bounds_max.z),
                );
                let translation = Vec3::new(center.x, terrain_height + 0.05, center.z)
                    - Vec3::new(model_center.x, bounds_min.y, model_center.z);
                self.ugx_transform = Mat4::from_translation(translation);
                log::info!(
                    "Loaded centered Warthog unit: {} components, {} triangles at ({:.2}, {:.2}, {:.2}) [{}]",
                    unit.component_count(),
                    unit.triangle_count(),
                    center.x,
                    terrain_height,
                    center.z,
                    unit.component_names().collect::<Vec<_>>().join(", ")
                );
                self.ugx_unit = Some(unit);
            }
            Err(error) => {
                let message = format!("Failed to load centered Warthog: {error}");
                log::error!("{message}");
                self.ugx_error = Some(message);
            }
        }
    }
}

fn terrain_surface_height(
    raw: &render::terrain::RawXtdData,
    world_x: f32,
    world_z: f32,
) -> Option<f32> {
    let dimension = raw.num_verts_per_axis;
    let last = dimension.checked_sub(1)?;
    let scale = raw.tile_scale.abs().max(f32::EPSILON);
    let grid_x = (world_x / scale).clamp(0.0, last.to_f32()?);
    let grid_z = (world_z / scale).clamp(0.0, last.to_f32()?);
    let x0 = grid_x.floor().to_u32()?;
    let z0 = grid_z.floor().to_u32()?;
    let x1 = x0.saturating_add(1).min(last);
    let z1 = z0.saturating_add(1).min(last);
    let tx = grid_x - x0.to_f32()?;
    let tz = grid_z - z0.to_f32()?;
    let h00 = packed_height(raw, x0, z0)?;
    let h10 = packed_height(raw, x1, z0)?;
    let h01 = packed_height(raw, x0, z1)?;
    let h11 = packed_height(raw, x1, z1)?;
    Some(h00.lerp(h10, tx).lerp(h01.lerp(h11, tx), tz))
}

fn packed_height(raw: &render::terrain::RawXtdData, x: u32, z: u32) -> Option<f32> {
    let index = z.checked_mul(raw.num_verts_per_axis)?.checked_add(x)?;
    let packed = *raw.packed_positions.get(usize::try_from(index).ok()?)?;
    let normalized = ((packed >> 10) & 0x3ff).to_f32()? / 1023.0;
    Some((normalized - render::terrain::NORMALIZED_TERRAIN_Y_OFFSET) * raw.range[1] - raw.mid[1])
}

#[cfg(test)]
mod tests {
    use super::terrain_surface_height;
    use num_traits::ToPrimitive;
    use render::terrain::{NORMALIZED_TERRAIN_Y_OFFSET, RawXtdData};

    fn packed_y(normalized: f32) -> u32 {
        let quantized = (normalized * 1023.0)
            .round()
            .to_u32()
            .expect("normalized test height must fit u32");
        quantized << 10
    }

    #[test]
    fn ugx_ground_placement_bilinearly_samples_world_xz() {
        let raw = RawXtdData {
            packed_positions: vec![packed_y(0.25), packed_y(0.5), packed_y(0.75), packed_y(1.0)],
            packed_normals: vec![0; 4],
            num_verts_per_axis: 2,
            mid: [0.0, 0.0, 0.0],
            range: [1.0, 100.0, 1.0],
            tile_scale: 2.0,
            world_min: [0.0; 3],
            world_max: [2.0, 100.0, 2.0],
            tessellation: None,
            ao_data: None,
            alpha_data: None,
        };
        let expected = (0.625 - NORMALIZED_TERRAIN_Y_OFFSET) * 100.0;
        let actual = terrain_surface_height(&raw, 1.0, 1.0).expect("center sample");
        assert!((actual - expected).abs() < 0.1);
    }
}
