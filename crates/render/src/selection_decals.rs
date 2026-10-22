//! Selection decals: the rings on the terrain under the selected and hovered squads.
//!
//! Retail `BSquad_drawSelectionDecal` (0x1403ABEA0) puts a decal into the decal manager, and
//! `BTerrainHeightField_renderDecalPatches` (0x140666FC0) draws it with the
//! `terrainHeightField` effect as a terrain-conforming patch. The [`TerrainPatchRenderer`]
//! is the translation of that effect, so this module only supplies the decal texture and the
//! instances. The unlit patch path gives the retail pixel:
//! `rgb = tex.rgb² · color.rgb² · intensity · color.a`, `a = tex.a · color.a`, with a discard
//! below `tex.a < 0.1`. The blend is source alpha over the destination, with the depth test
//! on and no depth write (decal type 1 in `BDecalManager_renderDecalBatch`, 0x1406F36A0).
//!
//! The retail decal texture comes from the Flash UI decal movies (`hud_selection.gfx`,
//! `hud_unsc_infantry.gfx`). These movies are vector art without a bitmap, so the renderer
//! uses a generated soft ring as a substitute.

use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;

use crate::terrain::{
    LightingParams, TerrainPatchError, TerrainPatchImage, TerrainPatchInstance,
    TerrainPatchMaterial, TerrainPatchRenderer, TerrainPatchRendererDescriptor,
    TerrainPatchShading, TerrainPatchWorldBindings,
};
use crate::wgpu;

/// Height of the decal above the terrain, in world units. The impact decals use the same
/// height, which keeps the patch above the tessellated terrain.
const SELECTION_DECAL_Y_OFFSET: f32 = 0.25;

/// Width and height of the generated ring texture, in texels.
const RING_TEXTURE_SIZE: u32 = 128;

/// Radius of the most opaque part of the ring, as a fraction of the decal radius.
const RING_PEAK_RADIUS: f32 = 0.85;

/// Width of the soft ring falloff, as a fraction of the decal radius.
const RING_WIDTH: f32 = 0.08;

/// One selection decal of a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionDecal {
    /// World position of the decal center, with the protosquad `<Selection>` `ZOffset`. The
    /// decal conforms to the terrain, so only X and Z place the decal; the Y value is not
    /// used.
    pub center: Vec3,
    /// Forward direction of the squad. Only the horizontal part turns the decal.
    pub forward: Vec3,
    /// Half width of the decal along the squad right direction (`<Selection>` `RadiusX`).
    pub radius_x: f32,
    /// Half length of the decal along the squad forward direction (`<Selection>` `RadiusZ`).
    pub radius_z: f32,
    /// Display color (0 to 1) and opacity of the decal. Retail uses the player selection
    /// color with opacity 1, or `SubSelectDecalAlpha` (0.4) outside the sub-selection.
    pub color: [f32; 4],
    /// HDR intensity: retail `BaseDecalIntensity` (6.0) for a selected squad and
    /// `SelectionDecalIntensity` (15.0) for a hovered squad.
    pub intensity: f32,
}

/// GPU state that draws the selection decals of each frame.
pub struct SelectionDecalRenderer {
    renderer: TerrainPatchRenderer,
    instances: Vec<TerrainPatchInstance>,
}

impl SelectionDecalRenderer {
    /// Makes the ring material and the patch pipeline.
    ///
    /// `color_format` is the HDR scene format: the decals draw in the lit world pass before
    /// the tone mapping, as in retail.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        world: TerrainPatchWorldBindings<'_>,
    ) -> Self {
        let mut material = TerrainPatchMaterial::from_images(
            "selection-ring",
            selection_ring_image(),
            TerrainPatchImage::solid([128, 128, 255, 255]),
            TerrainPatchImage::solid([255; 4]),
        );
        material.shading = TerrainPatchShading::Unlit;
        let renderer = TerrainPatchRenderer::new(
            device,
            queue,
            TerrainPatchRendererDescriptor {
                color_format,
                depth_format,
                material: &material,
                world,
            },
        );
        Self {
            renderer,
            instances: Vec::new(),
        }
    }

    /// Replaces the decals of the frame and updates the camera and the lighting.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainPatchError::TooManyInstances`] if one draw cannot address all decals.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        lighting: &LightingParams,
        decals: &[SelectionDecal],
    ) -> Result<(), TerrainPatchError> {
        self.instances.clear();
        self.instances
            .extend(decals.iter().filter_map(decal_instance));
        if !self.instances.is_empty() {
            self.renderer.update_frame(queue, view_projection, lighting);
        }
        self.renderer
            .update_instances(device, queue, &self.instances)
    }

    /// Draws the decals of the frame. A frame without decals draws nothing.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.renderer.render(pass);
    }
}

/// Converts one decal into a terrain patch. Returns `None` for a decal that cannot draw.
fn decal_instance(decal: &SelectionDecal) -> Option<TerrainPatchInstance> {
    let valid = decal.center.is_finite()
        && decal.radius_x.is_finite()
        && decal.radius_z.is_finite()
        && decal.radius_x > 0.0
        && decal.radius_z > 0.0
        && decal.intensity.is_finite()
        && decal.color.iter().all(|channel| channel.is_finite());
    if !valid {
        return None;
    }
    let forward = Vec3::new(decal.forward.x, 0.0, decal.forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z);
    let right = Vec3::Y.cross(forward);
    Some(TerrainPatchInstance {
        center: decal.center.to_array(),
        axis_u: (right * decal.radius_x).to_array(),
        axis_v: (forward * decal.radius_z).to_array(),
        y_offset: SELECTION_DECAL_Y_OFFSET,
        intensity: decal.intensity.max(0.0),
        color: decal.color,
        uv_rect: [0.0, 0.0, 1.0, 1.0],
    })
}

/// Makes the soft ring texture of the selection decals.
///
/// The color is white and the alpha holds the ring shape. The patch shader discards texels
/// with an alpha below 0.1, as retail does.
#[must_use]
pub fn selection_ring_image() -> TerrainPatchImage {
    let size = RING_TEXTURE_SIZE.to_f32().unwrap_or(1.0);
    let texel_count = usize::try_from(RING_TEXTURE_SIZE * RING_TEXTURE_SIZE).unwrap_or(0);
    let mut pixels = Vec::with_capacity(texel_count * 4);
    for y in 0..RING_TEXTURE_SIZE {
        for x in 0..RING_TEXTURE_SIZE {
            let u = (x.to_f32().unwrap_or(0.0) + 0.5) / size * 2.0 - 1.0;
            let v = (y.to_f32().unwrap_or(0.0) + 0.5) / size * 2.0 - 1.0;
            let opacity = ring_opacity(u.hypot(v));
            let alpha = (opacity * 255.0).round().to_u8().unwrap_or(0);
            pixels.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    TerrainPatchImage {
        width: RING_TEXTURE_SIZE,
        height: RING_TEXTURE_SIZE,
        pixels,
    }
}

/// Opacity of the ring at `radius`, a fraction of the decal radius.
fn ring_opacity(radius: f32) -> f32 {
    if radius >= 1.0 {
        return 0.0;
    }
    let distance = (radius - RING_PEAK_RADIUS) / RING_WIDTH;
    (-distance * distance).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_is_opaque_at_the_peak_and_empty_outside() {
        let image = selection_ring_image();
        assert_eq!(image.pixels.len(), 128 * 128 * 4);
        let texel = |x: usize, y: usize| &image.pixels[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4];
        // The center and the corners are empty, the peak on the X axis is opaque.
        assert_eq!(texel(64, 64), &[255, 255, 255, 0]);
        assert_eq!(texel(0, 0), &[255, 255, 255, 0]);
        let peak = texel(118, 64);
        assert!(peak[3] > 240, "peak opacity {}", peak[3]);
    }

    #[test]
    fn decal_instance_turns_with_forward_and_keeps_the_intensity() {
        let decal = SelectionDecal {
            center: Vec3::new(10.0, 3.0, 20.0),
            forward: Vec3::new(1.0, 0.5, 0.0),
            radius_x: 2.0,
            radius_z: 3.0,
            color: [0.2, 0.4, 1.0, 0.4],
            intensity: 6.0,
        };
        let instance = decal_instance(&decal).expect("decal is valid");
        assert_eq!(instance.center, [10.0, 3.0, 20.0]);
        assert_eq!(instance.axis_v, [3.0, 0.0, 0.0]);
        assert_eq!(instance.axis_u, [0.0, 0.0, -2.0]);
        assert!((instance.intensity - 6.0).abs() < f32::EPSILON);
        assert_eq!(instance.color, decal.color);

        let flat = SelectionDecal {
            forward: Vec3::Y,
            ..decal
        };
        assert_eq!(decal_instance(&flat).unwrap().axis_v, [0.0, 0.0, 3.0]);
        let empty = SelectionDecal {
            radius_x: 0.0,
            ..decal
        };
        assert_eq!(decal_instance(&empty), None);
    }
}
