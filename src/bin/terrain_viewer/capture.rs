//! Deterministic top-down GPU capture support for terrain validation.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use anyhow::{Context, Result, bail};
use glam::{Mat4, Vec2, Vec3};
use num_traits::ToPrimitive;
use render::terrain::{
    AlbedoData, FoliageQNChunk, FoliageSet, NORMALIZED_TERRAIN_Y_OFFSET, RawXtdData, TerrainMesh,
};
use render::wgpu;

use crate::foliage::parse_foliage_index_buffer;

/// Default width and height of a top-down validation capture.
pub const DEFAULT_CAPTURE_SIZE: u32 = 2048;

/// Command-line configuration for one top-down capture.
#[derive(Clone, Debug)]
pub struct CaptureConfig {
    /// Destination PNG path.
    pub output_path: PathBuf,
    /// Width and height in pixels.
    pub size: u32,
    /// Terrain shader debug/display mode used for the primary capture.
    pub debug_mode: u32,
    /// Optional world-space X/Z center for a focused top-down capture.
    pub center: Option<Vec2>,
    /// Optional world-space width and height for a focused top-down capture.
    pub span: Option<f32>,
}

impl CaptureConfig {
    /// Creates a square top-down capture configuration.
    #[must_use]
    pub fn new(output_path: PathBuf, size: u32) -> Self {
        Self {
            output_path,
            size,
            debug_mode: 0,
            center: None,
            span: None,
        }
    }

    /// Selects the terrain shader mode used for the primary capture.
    #[must_use]
    pub const fn with_debug_mode(mut self, debug_mode: u32) -> Self {
        self.debug_mode = debug_mode;
        self
    }

    /// Restricts the capture to a square world-space region.
    #[must_use]
    pub fn with_region(mut self, center: Vec2, span: f32) -> Self {
        self.center = Some(center);
        self.span = Some(span);
        self
    }

    /// Returns the companion path used for the decoded XTT atlas oracle.
    #[must_use]
    pub fn xtt_reference_path(&self) -> PathBuf {
        let mut path = self.output_path.clone();
        path.set_extension("xtt.png");
        path
    }

    /// Returns the companion path used for the foliage-free GPU height map.
    #[must_use]
    pub fn height_path(&self) -> PathBuf {
        let mut path = self.output_path.clone();
        path.set_extension("height.png");
        path
    }

    /// Returns the companion path used for the unpacked XTD height field.
    #[must_use]
    pub fn packed_height_path(&self) -> PathBuf {
        let stem = self
            .output_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("terrain-capture");
        let mut path = self.output_path.clone();
        path.set_file_name(format!("{stem}.packed-height.png"));
        path
    }

    /// Returns the companion path used for the XTD per-patch tessellation map.
    #[must_use]
    pub fn tessellation_path(&self) -> PathBuf {
        let stem = self
            .output_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("terrain-capture");
        let mut path = self.output_path.clone();
        path.set_file_name(format!("{stem}.tessellation.png"));
        path
    }

    /// Returns the companion path used for the albedo/height-contour overlay.
    #[must_use]
    pub fn alignment_path(&self) -> PathBuf {
        let mut path = self.output_path.clone();
        path.set_extension("alignment.png");
        path
    }

    /// Returns the companion path used for the decoded foliage placement map.
    #[must_use]
    pub fn foliage_placement_path(&self) -> PathBuf {
        let mut path = self.output_path.clone();
        path.set_extension("foliage-map.png");
        path
    }

    /// Returns the companion path used for a foliage-set close view.
    #[must_use]
    pub fn foliage_view_path(&self, set_index: usize) -> PathBuf {
        let stem = self
            .output_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("terrain-capture");
        let mut path = self.output_path.clone();
        path.set_file_name(format!("{stem}.foliage-{set_index}-view.png"));
        path
    }

    fn foliage_reference_path(&self, set_index: usize, kind: &str) -> PathBuf {
        let stem = self
            .output_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("terrain-capture");
        let mut path = self.output_path.clone();
        path.set_file_name(format!("{stem}.foliage-{set_index}-{kind}.png"));
        path
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CaptureStatus {
    Pending,
    Finished,
}

/// Runtime state for a single capture-and-exit run.
#[derive(Debug)]
pub struct CaptureState {
    config: CaptureConfig,
    status: CaptureStatus,
}

impl CaptureState {
    /// Creates pending capture state.
    #[must_use]
    pub fn new(config: CaptureConfig) -> Self {
        Self {
            config,
            status: CaptureStatus::Pending,
        }
    }

    /// Returns the immutable capture configuration.
    #[must_use]
    pub fn config(&self) -> &CaptureConfig {
        &self.config
    }

    /// Returns whether the GPU capture still needs to run.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.status == CaptureStatus::Pending
    }

    /// Returns whether the application should exit after capture.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.status == CaptureStatus::Finished
    }

    /// Marks the capture attempt finished, whether it succeeded or failed.
    pub fn finish(&mut self) {
        self.status = CaptureStatus::Finished;
    }
}

/// Fixed camera values used by the top-down capture render.
#[derive(Clone, Copy, Debug)]
pub struct ValidationCamera {
    /// Orthographic view-projection matrix.
    pub view_projection: Mat4,
    /// World-to-view matrix used by camera-facing particle geometry.
    pub world_to_view: Mat4,
    /// Depth reconstruction coefficients used by soft particles.
    pub depth_unproject: [f32; 2],
    /// Camera position used by lighting and foliage.
    pub position: Vec3,
    /// Distance that keeps every foliage placement visible in this overview.
    pub foliage_fade_start: f32,
}

/// Fits an orthographic camera to the decoded terrain bounds.
#[must_use]
pub fn top_down_camera(mesh: &TerrainMesh, viewport_size: (u32, u32)) -> ValidationCamera {
    let center = mesh.center();
    let terrain_size = mesh.size().abs();
    top_down_camera_for_bounds(
        mesh,
        viewport_size,
        Vec2::new(center.x, center.z),
        Vec2::new(terrain_size.x, terrain_size.z) * 1.01,
    )
}

/// Builds an orthographic camera for a square world-space detail region.
#[must_use]
pub fn top_down_detail_camera(
    mesh: &TerrainMesh,
    viewport_size: (u32, u32),
    center: Vec2,
    span: f32,
) -> ValidationCamera {
    top_down_camera_for_bounds(mesh, viewport_size, center, Vec2::splat(span))
}

fn top_down_camera_for_bounds(
    mesh: &TerrainMesh,
    viewport_size: (u32, u32),
    center: Vec2,
    extent: Vec2,
) -> ValidationCamera {
    let terrain_size = mesh.size().abs();
    let viewport_width = viewport_size
        .0
        .to_f32()
        .expect("capture width must fit f32");
    let viewport_height = viewport_size
        .1
        .to_f32()
        .expect("capture height must fit f32");
    let viewport_aspect = viewport_width / viewport_height;

    let mut half_width = (extent.x * 0.5).max(1.0);
    let mut half_height = (extent.y * 0.5).max(1.0);
    let terrain_aspect = half_width / half_height;
    if viewport_aspect > terrain_aspect {
        half_width = half_height * viewport_aspect;
    } else {
        half_height = half_width / viewport_aspect;
    }

    let clearance = (terrain_size.y + 64.0).max(64.0);
    let position = Vec3::new(center.x, mesh.world_max[1] + clearance, center.y);
    let target = Vec3::new(center.x, mesh.center().y, center.y);
    // -Z points toward the top of the image, making +X screen-right and +Z
    // screen-down. This matches the terrain texture coordinate convention.
    let view = Mat4::look_at_rh(position, target, Vec3::NEG_Z);
    let far = (position.y - mesh.world_min[1] + clearance).max(1.0);
    let projection =
        Mat4::orthographic_rh(-half_width, half_width, -half_height, half_height, 0.1, far);
    let foliage_fade_start = Vec3::new(terrain_size.x, far, terrain_size.z).length();

    ValidationCamera {
        view_projection: projection * view,
        world_to_view: view,
        // Validation captures are orthographic. A constant far-plane depth
        // keeps soft particles visible without pretending reciprocal
        // perspective reconstruction applies to this diagnostic camera.
        depth_unproject: [0.0, far.recip()],
        position,
        foliage_fade_start,
    }
}

fn foliage_random(blade_index: u32) -> f32 {
    let blade_index = blade_index
        .to_f32()
        .expect("foliage blade index must fit f32");
    let seed = (blade_index * 0.001_238_559_8).fract() * 257.0 + 1.0;
    (seed * seed).fract()
}

fn foliage_grid_position(grid_x: i32, grid_z: i32, blade_index: u32) -> Option<Vec2> {
    let world_chunk_x = u32::try_from(grid_x).ok()?;
    let world_chunk_z = u32::try_from(grid_z).ok()?;
    let random = foliage_random(blade_index);
    let jitter = (random * 2.0 - 1.0) * 0.9;
    let world_x = world_chunk_x
        .checked_mul(64)
        .and_then(|value| value.to_f32())
        .expect("foliage world X chunk offset must fit f32")
        + (blade_index % 64)
            .to_f32()
            .expect("foliage source local Z must fit f32")
        + jitter;
    let world_z = world_chunk_z
        .checked_mul(64)
        .and_then(|value| value.to_f32())
        .expect("foliage world Z chunk offset must fit f32")
        + (blade_index / 64)
            .to_f32()
            .expect("foliage source local X must fit f32")
        + 0.5
        + jitter;

    Some(Vec2::new(world_x, world_z))
}

fn placements_for_set(
    foliage_chunks: &[FoliageQNChunk],
    foliage_sets: &[FoliageSet],
    set_filter: Option<usize>,
) -> Vec<(Vec2, usize)> {
    let mut placements = Vec::new();
    for chunk in foliage_chunks {
        for (set_slot, &raw_set_index) in chunk.set_indices.iter().enumerate() {
            let Ok(set_index) = usize::try_from(raw_set_index) else {
                continue;
            };
            if set_filter.is_some_and(|filter| filter != set_index) {
                continue;
            }
            let Some(set) = foliage_sets.get(set_index) else {
                continue;
            };
            let Some(index_buffer) = chunk.index_buffers.get(set_slot) else {
                continue;
            };
            for [blade_index, _blade_type] in
                parse_foliage_index_buffer(index_buffer, set.num_verts_per_blade)
            {
                if let Some(position) =
                    foliage_grid_position(chunk.grid_x, chunk.grid_z, blade_index)
                {
                    placements.push((position, set_index));
                }
            }
        }
    }
    placements
}

fn decode_position_displacement(raw: &RawXtdData, packed: u32) -> Vec3 {
    // Foliage VS #0/#2 consumes the sampled value as `.zyx` and, unlike the
    // terrain VS/DS, does not subtract the terrain-only normalized Y bias.
    let x = ((packed >> 20) & 0x3FF)
        .to_f32()
        .expect("packed X position must fit f32")
        / 1023.0
        * raw.range[0]
        - raw.mid[0];
    let y = ((packed >> 10) & 0x3FF)
        .to_f32()
        .expect("packed Y position must fit f32")
        / 1023.0
        * raw.range[1]
        - raw.mid[1];
    let z = (packed & 0x3FF)
        .to_f32()
        .expect("packed Z position must fit f32")
        / 1023.0
        * raw.range[2]
        - raw.mid[2];
    Vec3::new(x, y, z)
}

fn terrain_position_at(raw: &RawXtdData, grid_position: Vec2) -> Option<Vec3> {
    let dimension = raw.num_verts_per_axis;
    if dimension == 0 {
        return None;
    }
    let max_grid = dimension - 1;
    let max_grid = max_grid.to_f32()?;
    let world_grid_x = grid_position.x.round().clamp(0.0, max_grid).to_u32()?;
    let world_grid_z = grid_position.y.round().clamp(0.0, max_grid).to_u32()?;
    let source_grid_x = world_grid_z;
    let source_grid_z = world_grid_x;
    let index = source_grid_x
        .checked_mul(dimension)?
        .checked_add(source_grid_z)?;
    let packed = *raw.packed_positions.get(usize::try_from(index).ok()?)?;
    let displacement = decode_position_displacement(raw, packed);
    Some(Vec3::new(
        grid_position.x * raw.tile_scale + displacement.z,
        displacement.y,
        grid_position.y * raw.tile_scale + displacement.x,
    ))
}

/// Builds a deterministic close camera around the densest 16x16-cell region
/// for one foliage set.
#[must_use]
pub fn foliage_camera(
    raw: &RawXtdData,
    foliage_chunks: &[FoliageQNChunk],
    foliage_sets: &[FoliageSet],
    set_index: usize,
    viewport_size: (u32, u32),
) -> Option<ValidationCamera> {
    let placements = placements_for_set(foliage_chunks, foliage_sets, Some(set_index));
    let mut bins = std::collections::BTreeMap::<(i32, i32), (Vec2, u32)>::new();
    for (position, _) in placements {
        let Some(bin_x) = (position.x / 16.0).floor().to_i32() else {
            continue;
        };
        let Some(bin_z) = (position.y / 16.0).floor().to_i32() else {
            continue;
        };
        let key = (bin_x, bin_z);
        let entry = bins.entry(key).or_insert((Vec2::ZERO, 0));
        entry.0 += position;
        entry.1 += 1;
    }
    let (sum, count) = bins
        .into_values()
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| *count > 0)?;
    let grid_focus = sum / count.to_f32()?;
    let terrain_focus = terrain_position_at(raw, grid_focus)?;
    let tile_scale = raw.tile_scale.abs().max(0.25);
    let target = terrain_focus + Vec3::Y * (2.5 * tile_scale);
    let position = target + Vec3::new(18.0, 13.0, 18.0) * tile_scale;
    let view = Mat4::look_at_rh(position, target, Vec3::Y);
    let aspect = viewport_size.0.to_f32()? / viewport_size.1.to_f32()?;
    let projection = Mat4::perspective_rh(40.0_f32.to_radians(), aspect, 0.1, 4096.0);
    Some(ValidationCamera {
        view_projection: projection * view,
        world_to_view: view,
        depth_unproject: render::particle::ParticleScene::perspective_depth_unproject(projection),
        position,
        foliage_fade_start: 400.0,
    })
}

/// Writes the XTT's decoded precomposited albedo beside a GPU capture.
pub fn write_xtt_reference(config: &CaptureConfig, albedo: &AlbedoData) -> Result<PathBuf> {
    let path = config.xtt_reference_path();
    write_rgba_png(&path, albedo.width, albedo.height, &albedo.pixels)?;
    Ok(path)
}

fn capture_grid_bounds(config: &CaptureConfig, raw: &RawXtdData) -> (Vec2, Vec2) {
    if let (Some(center), Some(span)) = (config.center, config.span) {
        let scale = raw.tile_scale.abs().max(f32::EPSILON);
        let grid_center = center / scale;
        let grid_span = Vec2::splat(span / scale);
        (grid_center - grid_span * 0.5, grid_center + grid_span * 0.5)
    } else {
        let extent = raw
            .num_verts_per_axis
            .to_f32()
            .expect("terrain dimension must fit f32");
        (Vec2::ZERO, Vec2::splat(extent))
    }
}

fn packed_height(raw: &RawXtdData, x: u32, z: u32) -> Option<f32> {
    let dimension = raw.num_verts_per_axis;
    let world_x = x.min(dimension.checked_sub(1)?);
    let world_z = z.min(dimension.checked_sub(1)?);
    // Viewer world (x, z) corresponds to XTD source (z, x).
    let index = world_z.checked_mul(dimension)?.checked_add(world_x)?;
    let packed = *raw.packed_positions.get(usize::try_from(index).ok()?)?;
    let normalized = ((packed >> 10) & 0x3ff).to_f32()? / 1023.0;
    Some((normalized - NORMALIZED_TERRAIN_Y_OFFSET) * raw.range[1] - raw.mid[1])
}

/// Writes the unpacked source height field for the same region as the GPU capture.
pub fn write_packed_height_reference(config: &CaptureConfig, raw: &RawXtdData) -> Result<PathBuf> {
    let size = config.size;
    let texel_count = usize::try_from(size.checked_mul(size).context("height map overflow")?)
        .context("height map dimensions do not fit memory")?;
    let mut pixels = vec![0_u8; texel_count * 4];
    let (grid_min, grid_max) = capture_grid_bounds(config, raw);
    let extent = grid_max - grid_min;
    let size_f = size.to_f32().context("capture size does not fit f32")?;
    let height_min = raw.world_min[1];
    let height_range = (raw.world_max[1] - height_min).max(f32::EPSILON);
    for y in 0..size {
        for x in 0..size {
            let sample = Vec2::new(
                (x.to_f32().context("height map X does not fit f32")? + 0.5) / size_f,
                (y.to_f32().context("height map Z does not fit f32")? + 0.5) / size_f,
            );
            let grid = grid_min + sample * extent;
            let grid_x = grid.x.round().max(0.0).to_u32().unwrap_or(0);
            let grid_z = grid.y.round().max(0.0).to_u32().unwrap_or(0);
            let height = packed_height(raw, grid_x, grid_z).unwrap_or(height_min);
            let gray = ((height - height_min) / height_range)
                .clamp(0.0, 1.0)
                .mul_add(255.0, 0.0)
                .round()
                .to_u8()
                .context("height intensity does not fit u8")?;
            let destination =
                usize::try_from((y * size + x) * 4).context("height reference offset overflow")?;
            pixels[destination..destination + 4].copy_from_slice(&[gray, gray, gray, 255]);
        }
    }

    let path = config.packed_height_path();
    write_rgba_png(&path, size, size, &pixels)?;
    Ok(path)
}

/// Writes the per-patch XTD tessellation levels for the GPU capture region.
pub fn write_tessellation_reference(
    config: &CaptureConfig,
    raw: &RawXtdData,
) -> Result<Option<PathBuf>> {
    let Some(tessellation) = &raw.tessellation else {
        return Ok(None);
    };
    let size = config.size;
    let texel_count = usize::try_from(size.checked_mul(size).context("tess map overflow")?)
        .context("tess map dimensions do not fit memory")?;
    let mut pixels = vec![0_u8; texel_count * 4];
    let (grid_min, grid_max) = capture_grid_bounds(config, raw);
    let extent = grid_max - grid_min;
    let size_f = size.to_f32().context("capture size does not fit f32")?;
    let colors = [
        [255_u8, 255, 255, 255],
        [190_u8, 220, 255, 255],
        [100_u8, 165, 255, 255],
        [25_u8, 70, 160, 255],
    ];
    for y in 0..size {
        for x in 0..size {
            let sample = Vec2::new(
                (x.to_f32().context("tess map X does not fit f32")? + 0.5) / size_f,
                (y.to_f32().context("tess map Z does not fit f32")? + 0.5) / size_f,
            );
            let grid = grid_min + sample * extent;
            let patch_x = (grid.x / 16.0).floor().max(0.0).to_u32().unwrap_or(0);
            let patch_z = (grid.y / 16.0).floor().max(0.0).to_u32().unwrap_or(0);
            let patch_x = patch_x.min(tessellation.patches_x.saturating_sub(1));
            let patch_z = patch_z.min(tessellation.patches_z.saturating_sub(1));
            let index = patch_z
                .checked_mul(tessellation.patches_x)
                .and_then(|row| row.checked_add(patch_x))
                .and_then(|index| usize::try_from(index).ok());
            let level = index
                .and_then(|index| tessellation.levels.get(index).copied())
                .unwrap_or(0)
                .min(3);
            let destination =
                usize::try_from((y * size + x) * 4).context("tess reference offset overflow")?;
            pixels[destination..destination + 4].copy_from_slice(&colors[usize::from(level)]);
        }
    }

    let path = config.tessellation_path();
    write_rgba_png(&path, size, size, &pixels)?;
    Ok(Some(path))
}

/// Writes decoded foliage material inputs beside a GPU capture.
pub fn write_foliage_references(
    config: &CaptureConfig,
    foliage_sets: &[FoliageSet],
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (set_index, set) in foliage_sets.iter().enumerate() {
        if !set.albedo_pixels.is_empty() {
            let path = config.foliage_reference_path(set_index, "albedo");
            write_rgba_png(
                &path,
                set.albedo_width,
                set.albedo_height,
                &set.albedo_pixels,
            )?;
            paths.push(path);
        }
        if !set.opacity_pixels.is_empty() {
            let path = config.foliage_reference_path(set_index, "opacity");
            write_rgba_png(
                &path,
                set.opacity_width,
                set.opacity_height,
                &set.opacity_pixels,
            )?;
            paths.push(path);
        }
    }
    Ok(paths)
}

/// Writes exact foliage blade placements over a dimmed XTT albedo reference.
pub fn write_foliage_placement_reference(
    config: &CaptureConfig,
    albedo: &AlbedoData,
    foliage_chunks: &[FoliageQNChunk],
    foliage_sets: &[FoliageSet],
    num_verts_per_axis: u32,
) -> Result<PathBuf> {
    let size = config.size;
    let texel_count = usize::try_from(size.checked_mul(size).context("placement map overflow")?)
        .context("placement map dimensions do not fit memory")?;
    let mut pixels = vec![0_u8; texel_count * 4];
    for y in 0..size {
        let source_y = y * albedo.height / size;
        for x in 0..size {
            let source_x = x * albedo.width / size;
            let source = usize::try_from((source_y * albedo.width + source_x) * 4)
                .context("albedo reference offset overflow")?;
            let destination = usize::try_from((y * size + x) * 4)
                .context("placement reference offset overflow")?;
            pixels[destination] = albedo.pixels[source] / 3;
            pixels[destination + 1] = albedo.pixels[source + 1] / 3;
            pixels[destination + 2] = albedo.pixels[source + 2] / 3;
            pixels[destination + 3] = 255;
        }
    }

    let palette = [
        [80_u8, 255, 80, 255],
        [255, 170, 40, 255],
        [80, 190, 255, 255],
        [255, 80, 220, 255],
    ];
    let denominator = num_verts_per_axis
        .max(1)
        .to_f32()
        .context("terrain dimension does not fit f32")?;
    let capture_extent = size.to_f32().context("capture size does not fit f32")?;
    let signed_extent = i32::try_from(size).context("capture size does not fit i32")?;
    for (position, set_index) in placements_for_set(foliage_chunks, foliage_sets, None) {
        let pixel_x = (position.x / denominator * capture_extent)
            .floor()
            .to_i32()
            .context("foliage X marker does not fit i32")?;
        let pixel_y = (position.y / denominator * capture_extent)
            .floor()
            .to_i32()
            .context("foliage Z marker does not fit i32")?;
        let color = palette[set_index % palette.len()];
        for offset_y in -1..=1 {
            for offset_x in -1..=1 {
                let x = pixel_x + offset_x;
                let y = pixel_y + offset_y;
                if x < 0 || y < 0 || x >= signed_extent || y >= signed_extent {
                    continue;
                }
                let x = u32::try_from(x).context("foliage X marker is negative")?;
                let y = u32::try_from(y).context("foliage Z marker is negative")?;
                let destination = usize::try_from((y * size + x) * 4)
                    .context("foliage marker offset overflow")?;
                pixels[destination..destination + 4].copy_from_slice(&color);
            }
        }
    }

    let path = config.foliage_placement_path();
    write_rgba_png(&path, size, size, &pixels)?;
    Ok(path)
}

/// Offscreen render target and CPU-readable copy buffer.
pub struct CaptureTarget {
    texture: wgpu::Texture,
    /// View used as the terrain render-pass color attachment.
    pub view: wgpu::TextureView,
    readback: wgpu::Buffer,
    size: u32,
    padded_bytes_per_row: u32,
    format: wgpu::TextureFormat,
}

impl CaptureTarget {
    /// Allocates a same-format render target and aligned readback buffer.
    pub fn new(device: &wgpu::Device, size: u32, format: wgpu::TextureFormat) -> Result<Self> {
        if !matches!(
            format,
            wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
                | wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
        ) {
            bail!("top-down PNG capture does not support surface format {format:?}");
        }
        let padded_bytes_per_row = aligned_bytes_per_row(size)?;
        let buffer_size = u64::from(padded_bytes_per_row)
            .checked_mul(u64::from(size))
            .context("capture readback buffer size overflow")?;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Terrain Top-Down Capture"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Top-Down Readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            texture,
            view,
            readback,
            size,
            padded_bytes_per_row,
            format,
        })
    }

    /// Appends the texture-to-buffer copy to the supplied encoder.
    pub fn encode_copy(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.size),
                },
            },
            self.texture.size(),
        );
    }

    /// Blocks until the submitted readback completes, then writes a PNG.
    pub fn write_png(&self, device: &wgpu::Device, path: &Path) -> Result<()> {
        let buffer_slice = self.readback.slice(..);
        let (sender, receiver) = mpsc::sync_channel(1);
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            sender
                .send(result)
                .expect("capture map receiver must remain alive");
        });
        let _poll_result = device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .context("capture map callback ended unexpectedly")?
            .context("failed to map capture buffer")?;

        let write_result = {
            let mapped = buffer_slice.get_mapped_range();
            let rgba = unpack_rgba(
                &mapped,
                self.size,
                self.size,
                self.padded_bytes_per_row,
                self.format,
            )?;
            write_rgba_png(path, self.size, self.size, &rgba)
        };
        self.readback.unmap();
        write_result
    }
}

fn aligned_bytes_per_row(width: u32) -> Result<u32> {
    let unpadded = width
        .checked_mul(4)
        .context("capture row byte count overflow")?;
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded
        .div_ceil(alignment)
        .checked_mul(alignment)
        .context("aligned capture row byte count overflow")
}

fn unpack_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    format: wgpu::TextureFormat,
) -> Result<Vec<u8>> {
    let unpadded_bytes_per_row = width
        .checked_mul(4)
        .context("capture output row byte count overflow")?;
    let output_len = usize::try_from(
        u64::from(unpadded_bytes_per_row)
            .checked_mul(u64::from(height))
            .context("capture output size overflow")?,
    )
    .context("capture output does not fit memory address space")?;
    let padded_row = usize::try_from(padded_bytes_per_row)
        .context("capture row stride does not fit memory address space")?;
    let unpadded_row = usize::try_from(unpadded_bytes_per_row)
        .context("capture row size does not fit memory address space")?;
    let row_count =
        usize::try_from(height).context("capture height does not fit memory address space")?;
    let required_len = padded_row
        .checked_mul(row_count)
        .context("mapped capture size overflow")?;
    if data.len() < required_len {
        bail!(
            "mapped capture is {} bytes, expected at least {required_len}",
            data.len()
        );
    }

    let mut rgba = Vec::with_capacity(output_len);
    for row in data.chunks_exact(padded_row).take(row_count) {
        let pixels = &row[..unpadded_row];
        match format {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
                rgba.extend_from_slice(pixels);
            }
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                for pixel in pixels.as_chunks::<4>().0 {
                    rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                }
            }
            _ => bail!("unsupported capture format {format:?}"),
        }
    }
    Ok(rgba)
}

fn write_rgba_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create capture directory {}", parent.display()))?;
    }
    let file = File::create(path)
        .with_context(|| format!("failed to create capture {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .context("failed to write PNG header")?;
    writer
        .write_image_data(rgba)
        .context("failed to write PNG pixels")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        aligned_bytes_per_row, foliage_grid_position, foliage_random, packed_height, unpack_rgba,
    };
    use render::terrain::{NORMALIZED_TERRAIN_Y_OFFSET, RawXtdData};
    use render::wgpu;

    #[test]
    fn capture_rows_use_webgpu_copy_alignment() {
        assert_eq!(aligned_bytes_per_row(1).expect("valid width"), 256);
        assert_eq!(aligned_bytes_per_row(64).expect("valid width"), 256);
        assert_eq!(aligned_bytes_per_row(65).expect("valid width"), 512);
    }

    #[test]
    fn bgra_readback_is_unpadded_and_swizzled() {
        let mut mapped = vec![0_u8; 16];
        mapped[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let rgba = unpack_rgba(&mapped, 2, 1, 16, wgpu::TextureFormat::Bgra8UnormSrgb)
            .expect("valid BGRA capture");
        assert_eq!(rgba, [3, 2, 1, 4, 7, 6, 5, 8]);
    }

    #[test]
    fn foliage_grid_axes_preserve_parent_chunk_and_transpose_local_index() {
        let random = foliage_random(64);
        let jitter = (random * 2.0 - 1.0) * 0.9;
        let position = foliage_grid_position(2, 3, 64).expect("valid XTT chunk");
        let expected_x = 2.0 * 64.0 + jitter;
        let expected_z = 3.0 * 64.0 + 1.5 + jitter;
        assert!((position.x - expected_x).abs() < 0.000_01);
        assert!((position.y - expected_z).abs() < 0.000_01);
    }

    #[test]
    fn packed_height_applies_the_xtd_world_diagonal_mirror() {
        let raw = RawXtdData {
            packed_positions: [10_u32, 11, 20, 21]
                .into_iter()
                .map(|height| height << 10)
                .collect(),
            packed_normals: vec![0; 4],
            num_verts_per_axis: 2,
            mid: [0.0; 3],
            range: [1.0, 1023.0, 1.0],
            tile_scale: 1.0,
            world_min: [0.0; 3],
            world_max: [1.0; 3],
            tessellation: None,
            ao_data: None,
            alpha_data: None,
        };
        let terrain_bias = 1023.0 * NORMALIZED_TERRAIN_Y_OFFSET;

        let x_neighbor = packed_height(&raw, 1, 0).expect("valid native XTD coordinate");
        let z_neighbor = packed_height(&raw, 0, 1).expect("valid native XTD coordinate");

        assert!((x_neighbor - (11.0 - terrain_bias)).abs() < 0.000_01);
        assert!((z_neighbor - (20.0 - terrain_bias)).abs() < 0.000_01);
    }
}
