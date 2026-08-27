//! `TerrainViewer` struct and implementation.
//!
//! Split into sub-modules:
//! - `input` — keyboard/mouse handling, debug mode switching, UI
//! - `render` — GPU initialization and render pass logic

mod input;
mod rendering;

use std::path::{Path, PathBuf};

use glam::{FloatExt, Mat4, Vec3};
use num_traits::ToPrimitive;
use pipeline::hw1;
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xtd;
use pipeline::xtt;
use render::environment::EnvironmentMap;
use render::lighting::LocalLightSet;
use render::postprocess::{HDR_COLOR_FORMAT, ToneMapResources};
use render::terrain::{Camera, CompositorResources, LodConfig, TerrainScene};
use render::ugx::{
    Unit as UgxUnit, UnitRenderer as UgxUnitRenderer, UnitScene as UgxUnitScene,
    UnitSceneRenderer as UgxUnitSceneRenderer,
};
use render::wgpu;

use crate::capture::{CaptureConfig, CaptureState};
use crate::types::GpuResources;

const WARTHOG_VISUAL: &str = "unsc_veh_warthog_01";

struct TerrainLoadInputs {
    terrain: xtd::XtdFile,
    textures: Option<xtt::XttFile>,
    lightset: Option<hw1::LightSetData>,
    environment: Option<EnvironmentMap>,
    sky: Option<UgxUnit>,
    source: Option<AssetSource<StdFileProvider>>,
    warthog_visual: Option<pipeline::database::hw1::Visual>,
    ugx_scene: Option<UgxUnitScene>,
}

/// The terrain viewer application.
pub struct TerrainViewer {
    pub xtd_path: Option<PathBuf>,
    /// Scenario name (for ERA loading).
    pub scenario_name: Option<String>,
    /// Asset source for loading textures from ERA archives.
    pub asset_source: Option<AssetSource<StdFileProvider>>,
    /// All decoded terrain data (mesh, textures, splat, decals, foliage, roads).
    pub scene: Option<TerrainScene>,
    /// Scenario GLS/FLS constants used by every lit renderer.
    pub lightset: Option<hw1::LightSetData>,
    /// Runtime local lights shared by all lit world renderers.
    pub local_lights: LocalLightSet,
    /// Scenario-global HDR environment cubemap used by reflective materials.
    pub environment: Option<EnvironmentMap>,
    /// Authored scenario sky visual, resolved directly through VIS/UGX.
    pub sky_unit: Option<UgxUnit>,
    /// GPU resources for the camera-relative authored sky visual.
    pub sky_renderer: Option<UgxUnitRenderer>,
    pub camera: Camera,
    pub show_info: bool,
    pub wireframe: bool,
    pub load_error: Option<String>,
    pub gpu: Option<GpuResources>,
    /// Linear color format shared by all scene pipelines.
    pub scene_format: wgpu::TextureFormat,
    /// HDR scene target, luminance reduction, and presentation pipeline.
    pub tone_map_resources: Option<ToneMapResources>,
    /// Monotonic render time used by authored material and foliage animation.
    pub render_time_seconds: f32,
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
    /// Scenario objects resolved through their database visuals.
    pub ugx_scene: Option<UgxUnitScene>,
    /// GPU resources for renderable scenario object placements.
    pub ugx_scene_renderer: Option<UgxUnitSceneRenderer>,
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
            lightset: None,
            local_lights: LocalLightSet::default(),
            environment: None,
            sky_unit: None,
            sky_renderer: None,
            camera: Camera::default(),
            show_info: true,
            wireframe: false,
            load_error: None,
            gpu: None,
            scene_format: HDR_COLOR_FORMAT,
            tone_map_resources: None,
            render_time_seconds: 0.0,
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
            ugx_scene: None,
            ugx_scene_renderer: None,
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
        self.lightset = inputs.lightset;
        self.environment = inputs.environment;
        self.sky_unit = inputs.sky;
        self.sky_renderer = None;
        if let Some(lightset) = &self.lightset {
            self.bump_power = lightset.terrain_bump_strength;
        }
        self.ugx_scene = inputs.ugx_scene;
        self.ugx_scene_renderer = None;

        // Load entire terrain scene in one call
        let first_load = self.scene.is_none();
        match TerrainScene::load(
            &inputs.terrain,
            inputs.textures.as_ref(),
            self.asset_source.as_mut(),
        ) {
            Ok(scene) => {
                if first_load {
                    self.camera.position = scene.mesh.center() + Vec3::new(0.0, 200.0, -300.0);
                }
                self.scene = Some(scene);
                self.load_error = None;
                self.load_warthog(inputs.warthog_visual.as_ref());
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
                    "Loaded centered Warthog unit: {} components, {} triangles, {} authored attachments ({} unresolved target bones) at ({:.2}, {:.2}, {:.2}) [{}]",
                    unit.component_count(),
                    unit.triangle_count(),
                    unit.attachments().len(),
                    unit.unresolved_attachment_bone_count(),
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

fn load_scenario_inputs(scenario_name: &str) -> Result<TerrainLoadInputs, String> {
    log::info!("Loading scenario via pipeline::World: {scenario_name}");
    let dir = data::paths::game_dir();
    let dir_str = dir.to_string_lossy();
    let (mut world, mut source) =
        hw1::World::load(&dir_str).map_err(|error| format!("Failed to load World: {error}"))?;
    world.swap_scenario(&mut source, scenario_name);

    let warthog_visual = world.visuals.get(WARTHOG_VISUAL).cloned();
    let sky = world
        .manifest
        .sky_ref
        .as_deref()
        .map(|reference| load_sky_unit(&mut source, reference))
        .transpose()?;
    if let (Some(reference), Some(sky)) = (&world.manifest.sky_ref, &sky) {
        log::info!(
            "Loaded scenario sky '{}': {} components, {} triangles",
            reference,
            sky.component_count(),
            sky.triangle_count(),
        );
    }
    let environment = world
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
    let player_start_proto = world
        .database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.code_proto_objects.as_ref())
        .and_then(|mappings| {
            mappings.entries.iter().find(|entry| {
                entry
                    .object_type
                    .eq_ignore_ascii_case("SkirmishEmptyBaseObject")
            })
        })
        .map(|entry| entry.proto_name.clone());
    let ugx_scene = world.scenario_data.as_ref().map(|scenario| {
        if let Some(proto_name) = player_start_proto.as_deref() {
            UgxUnitScene::load_scenario(
                &mut source,
                scenario,
                &world.visuals,
                &world.database.objects,
                proto_name,
            )
        } else {
            if !scenario.positions().is_empty() {
                log::warn!(
                    "Game data has no SkirmishEmptyBaseObject mapping; player-start pads are unavailable"
                );
            }
            UgxUnitScene::load(
                &mut source,
                scenario.objects(),
                &world.visuals,
                &world.database.objects,
            )
        }
    });
    if let Some(scene) = &ugx_scene {
        log_ugx_scene_summary(scene);
    }

    let terrain = world.terrain_data.take().ok_or_else(|| {
        format!("World loaded but no XTD terrain data for scenario '{scenario_name}'")
    })?;
    Ok(TerrainLoadInputs {
        terrain,
        textures: world.terrain_textures.take(),
        lightset: world.lightset.take(),
        environment,
        sky,
        source: Some(source),
        warthog_visual,
        ugx_scene,
    })
}

fn log_ugx_scene_summary(scene: &UgxUnitScene) {
    log::info!(
        "Resolved scenario UGX scene: {} placements from {} objects + {} player starts across {} visuals ({} NoRender, {} gameplay-only, {} squads, {} malformed, {} invalid transforms, {} object load failures, {} start failures)",
        scene.placement_count(),
        scene.object_count(),
        scene.player_start_count(),
        scene.unique_visual_count(),
        scene.skipped_no_render_count(),
        scene.missing_visual_count(),
        scene.skipped_squad_count(),
        scene.missing_proto_count(),
        scene.invalid_transform_count(),
        scene.load_failure_count(),
        scene.player_start_failure_count(),
    );
    for issue in scene.issues() {
        log::warn!(
            "Scenario visual '{}' was skipped: {}",
            issue.proto_name(),
            issue.reason()
        );
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
        warthog_visual: None,
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
