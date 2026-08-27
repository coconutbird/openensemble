//! GPU particle drawing foundation.
//!
//! The original PC path submits one point per particle and expands it in a
//! geometry shader. `wgpu` does not expose geometry shaders, so this renderer
//! performs the equivalent expansion with `vertex_index` and instance data.
//! It deliberately does not simulate emitters: a later simulation layer can
//! supply resolved instances without creating a second rendering path.

use std::collections::HashMap;
use std::mem;

use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;
use pipeline::ddx::DdxTexture;
use pipeline::source::{AssetSource, StdFileProvider};
use wgpu::util::DeviceExt;

use crate::{RenderPhase, WorldRenderer};

use crate::postprocess::DISTORTION_FORMAT;

mod effect;

pub use effect::{
    ParticleEffect, ParticleEffectError, ParticleEmitter, ParticleEmitterKind,
    ParticleMaterialDefinition, ParticleTextureDefinition, ParticleTextureStage,
    ParticleUvAnimation,
};

const SHADER: &str = include_str!("particle.wgsl");
const COLOR_VERTEX_COUNT: u32 = 12;
const MATERIAL_HAS_INTENSITY: u32 = 1 << 0;
const MATERIAL_LIGHT_VOLUME: u32 = 1 << 1;
const MATERIAL_SOFT_PARTICLES: u32 = 1 << 2;
const MATERIAL_SOFT_FADE_RGB: u32 = 1 << 3;

/// Particle geometry families present in shipped PFX data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u32)]
pub enum ParticleGeometry {
    /// Camera-facing quad (`eBillBoard`).
    #[default]
    Billboard = 0,
    /// Horizontal world-space quad (`eUpfacing`).
    UpFacing = 1,
    /// Camera-facing quad constrained to an authored axis.
    OrientedAxial = 2,
    /// Axial quad whose long axis follows particle velocity.
    VelocityAligned = 3,
    /// Camera-facing segment between two beam control points.
    Beam = 4,
    /// Camera-facing trail segment.
    Trail = 5,
    /// Two perpendicular trail ribbons (`eTrailCross`).
    TrailCross = 6,
    /// Horizontal patch used for terrain decals/effects.
    TerrainPatch = 7,
}

impl ParticleGeometry {
    fn is_segment(self) -> bool {
        matches!(
            self,
            Self::Beam | Self::Trail | Self::TrailCross | Self::VelocityAligned
        )
    }
}

/// Fixed-function blend families authored by PFX emitters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParticleBlendMode {
    /// Source-over alpha blending (`eAlphaBlend`).
    #[default]
    Alpha,
    /// Source-alpha additive blending (`eAdditive`).
    Additive,
    /// Premultiplied source-over blending (`ePremultipliedAlpha`).
    PremultipliedAlpha,
    /// Reverse-subtractive source-alpha blending (`eSubtractive`).
    Subtractive,
    /// Writes a signed screen-space offset into the distortion target.
    Distortion,
}

/// How the next diffuse texture layer combines with the accumulated color.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u32)]
pub enum ParticleLayerBlend {
    /// Multiplies alpha-weighted RGB, matching `eBlendMultiply`.
    #[default]
    Multiply = 0,
    /// Interpolates by the incoming layer alpha, matching `eBlendAlpha`.
    Alpha = 1,
}

/// One resolved particle ready for GPU expansion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleInstance {
    /// World-space center.
    pub position: [f32; 3],
    /// Rotation in radians around the facing axis.
    pub rotation: f32,
    /// Long/facing axis. Segment constructors normalize this automatically.
    pub axis: [f32; 3],
    /// Half-length for beam/trail/velocity-aligned geometry.
    pub half_length: f32,
    /// Full quad width and height. Segment geometry uses width and ignores
    /// height in favor of `half_length`.
    pub size: [f32; 2],
    /// Progression/tint RGBA already evaluated by the simulation layer.
    pub color: [f32; 4],
    /// RGB intensity progression and alpha multiplier.
    pub intensity: [f32; 4],
    /// Per-map UV rectangles: diffuse 1/2/3 followed by intensity.
    pub uv_rects: [[f32; 4]; 4],
    /// Texture-array layer for diffuse 1/2/3 and intensity.
    pub texture_layers: [u32; 4],
    /// Geometry expansion family.
    pub geometry: ParticleGeometry,
    /// World/view-depth distance over which a soft particle fades in.
    pub soft_fade_range: f32,
}

impl ParticleInstance {
    /// Creates a camera-facing particle.
    #[must_use]
    pub fn billboard(position: [f32; 3], size: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            rotation: 0.0,
            axis: [0.0, 1.0, 0.0],
            half_length: size[1] * 0.5,
            size,
            color,
            intensity: [1.0; 4],
            uv_rects: [[0.0, 0.0, 1.0, 1.0]; 4],
            texture_layers: [0; 4],
            geometry: ParticleGeometry::Billboard,
            soft_fade_range: 0.5,
        }
    }

    /// Creates a beam/trail segment from two world-space endpoints.
    #[must_use]
    pub fn segment(
        start: [f32; 3],
        end: [f32; 3],
        width: f32,
        color: [f32; 4],
        geometry: ParticleGeometry,
    ) -> Self {
        let start = Vec3::from_array(start);
        let end = Vec3::from_array(end);
        let delta = end - start;
        let length = delta.length();
        let axis = if length > f32::EPSILON {
            delta / length
        } else {
            Vec3::Y
        };
        let mut instance =
            Self::billboard(((start + end) * 0.5).to_array(), [width, length], color);
        instance.axis = axis.to_array();
        instance.half_length = length * 0.5;
        instance.geometry = if geometry.is_segment() {
            geometry
        } else {
            ParticleGeometry::Trail
        };
        instance
    }

    fn packed(self) -> PackedParticleInstance {
        PackedParticleInstance {
            position_rotation: [
                self.position[0],
                self.position[1],
                self.position[2],
                self.rotation,
            ],
            axis_half_length: [
                self.axis[0],
                self.axis[1],
                self.axis[2],
                self.half_length.max(0.0),
            ],
            half_size_softness: [
                self.size[0].abs() * 0.5,
                self.size[1].abs() * 0.5,
                self.soft_fade_range.max(f32::EPSILON),
                0.0,
            ],
            color: self.color,
            intensity: self.intensity,
            uv_rect0: self.uv_rects[0],
            uv_rect1: self.uv_rects[1],
            uv_rect2: self.uv_rects[2],
            uv_rect_intensity: self.uv_rects[3],
            texture_layers: self.texture_layers,
            geometry: [self.geometry as u32, 0, 0, 0],
        }
    }
}

/// One decoded RGBA particle texture layer.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleImage {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Tightly packed RGBA8 pixels.
    pub pixels: Vec<u8>,
    /// DDX HDR scale.
    pub hdr_scale: f32,
}

impl ParticleImage {
    /// Creates a validated image from decoded pixels.
    ///
    /// # Errors
    ///
    /// Returns [`ParticleError::InvalidImage`] when the dimensions are zero or
    /// the byte count does not equal `width * height * 4`.
    pub fn from_rgba(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        hdr_scale: f32,
    ) -> Result<Self, ParticleError> {
        validate_image(width, height, pixels.len())?;
        Ok(Self {
            width,
            height,
            pixels,
            hdr_scale,
        })
    }

    /// Resolves and decodes a particle texture from the active asset stack.
    /// PFX `.tga` references are mapped to the shipped `.ddx` file in-place.
    ///
    /// # Errors
    ///
    /// Returns an error when the texture is missing or the DDX cannot decode.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        path: &str,
    ) -> Result<Self, ParticleError> {
        let canonical = canonical_particle_texture_path(path);
        let bytes = source
            .resolve_with_fallback(&canonical, &[".ddx"])
            .ok_or_else(|| ParticleError::TextureNotFound(canonical.clone()))?;
        let texture =
            DdxTexture::from_bytes(&bytes).map_err(|error| ParticleError::TextureDecode {
                path: canonical.clone(),
                reason: error.to_string(),
            })?;
        let hdr_scale = texture.info.hdr_scale;
        let decoded = texture
            .decode_to_rgba()
            .map_err(|error| ParticleError::TextureDecode {
                path: canonical,
                reason: error.to_string(),
            })?;
        Self::from_rgba(decoded.width, decoded.height, decoded.pixels, hdr_scale)
    }
}

/// Same-sized images uploaded as one particle texture array.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleTextureArray {
    layers: Vec<ParticleImage>,
}

impl ParticleTextureArray {
    /// Creates a checked texture array.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty array or mismatched layer dimensions.
    pub fn new(layers: Vec<ParticleImage>) -> Result<Self, ParticleError> {
        let Some(first) = layers.first() else {
            return Err(ParticleError::EmptyTextureArray);
        };
        validate_image(first.width, first.height, first.pixels.len())?;
        for (index, layer) in layers.iter().enumerate().skip(1) {
            validate_image(layer.width, layer.height, layer.pixels.len())?;
            if (layer.width, layer.height) != (first.width, first.height) {
                return Err(ParticleError::TextureArrayDimensions {
                    layer: index,
                    expected: [first.width, first.height],
                    actual: [layer.width, layer.height],
                });
            }
        }
        u32::try_from(layers.len()).map_err(|_| ParticleError::TooManyTextureLayers {
            actual: layers.len(),
        })?;
        Ok(Self { layers })
    }

    /// Loads all named DDX layers from PFX-style texture references.
    ///
    /// # Errors
    ///
    /// Propagates missing/invalid texture and array validation errors.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        paths: &[String],
    ) -> Result<Self, ParticleError> {
        let layers = paths
            .iter()
            .map(|path| ParticleImage::load(source, path))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(layers)
    }

    /// Returns decoded layers in texture-array order.
    #[must_use]
    pub fn layers(&self) -> &[ParticleImage] {
        &self.layers
    }

    fn hdr_scale(&self) -> f32 {
        self.layers
            .iter()
            .map(|image| image.hdr_scale)
            .fold(1.0_f32, f32::max)
            .max(1.0)
    }
}

/// Static material data shared by an emitter's particle instances.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleMaterial {
    /// Up to three independently animated diffuse texture arrays.
    pub diffuse: [Option<ParticleTextureArray>; 3],
    /// Optional intensity texture array.
    pub intensity: Option<ParticleTextureArray>,
    /// Blend between diffuse layers one and two.
    pub layer_1_to_2: ParticleLayerBlend,
    /// Blend between the accumulated color and diffuse layer three.
    pub layer_2_to_3: ParticleLayerBlend,
    /// Framebuffer blend family.
    pub blend: ParticleBlendMode,
    /// Enables oracle depth-based soft-particle fading.
    pub soft_particles: bool,
    /// Applies the scenario light-volume texture to RGB.
    pub light_volume: bool,
    /// Multiplier for sampled light-volume RGB.
    pub light_volume_intensity: f32,
}

impl Default for ParticleMaterial {
    fn default() -> Self {
        Self {
            diffuse: [None, None, None],
            intensity: None,
            layer_1_to_2: ParticleLayerBlend::Multiply,
            layer_2_to_3: ParticleLayerBlend::Multiply,
            blend: ParticleBlendMode::Alpha,
            soft_particles: false,
            light_volume: false,
            light_volume_intensity: 1.0,
        }
    }
}

/// Per-frame particle camera and optional light-volume transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleScene {
    /// World-to-clip matrix.
    pub view_projection: Mat4,
    /// World-to-view matrix.
    pub world_to_view: Mat4,
    /// World-space camera position.
    pub camera_position: [f32; 3],
    /// Render-target width and height.
    pub viewport_size: [u32; 2],
    /// Coefficients for `eye_depth = 1 / (device_depth * x + y)`.
    pub depth_unproject: [f32; 2],
    /// World-to-light-volume rows producing normalized texture coordinates.
    pub light_volume_rows: [[f32; 4]; 3],
}

impl ParticleScene {
    /// Creates camera state with identity light-volume addressing.
    #[must_use]
    pub fn new(
        view_projection: Mat4,
        world_to_view: Mat4,
        camera_position: [f32; 3],
        viewport_size: [u32; 2],
        depth_unproject: [f32; 2],
    ) -> Self {
        Self {
            view_projection,
            world_to_view,
            camera_position,
            viewport_size,
            depth_unproject,
            light_volume_rows: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
            ],
        }
    }
}

/// Optional shared scene textures used by particle permutations.
#[derive(Clone, Copy)]
pub struct ParticleSceneTextures<'a> {
    /// Sampleable scene depth texture.
    pub depth: &'a wgpu::TextureView,
    /// Optional 3D light-volume color texture.
    pub light_volume: Option<&'a wgpu::TextureView>,
}

/// GPU resources for one particle material/emitter batch.
pub struct ParticleRenderer {
    scene_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    material_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: u32,
    color_pipeline: Option<wgpu::RenderPipeline>,
    distortion_pipeline: Option<wgpu::RenderPipeline>,
}

impl ParticleRenderer {
    /// Uploads a particle material and creates its oracle-equivalent draw
    /// pipeline. The instance stream starts empty.
    ///
    /// # Errors
    ///
    /// Returns texture validation/upload errors from the material payload.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        material: &ParticleMaterial,
        scene_textures: ParticleSceneTextures<'_>,
    ) -> Result<Self, ParticleError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Oracle Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let scene_layout = create_scene_layout(device);
        let material_layout = create_material_layout(device);
        let scene_uniform = PackedParticleScene::default();
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Particle Scene Uniform"),
            contents: bytemuck::bytes_of(&scene_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle Scene Bind Group"),
            layout: &scene_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene_buffer.as_entire_binding(),
            }],
        });
        let material_bind_group =
            create_material_bind_group(device, queue, &material_layout, material, scene_textures)?;
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Particle Pipeline Layout"),
            bind_group_layouts: &[&scene_layout, &material_layout],
            push_constant_ranges: &[],
        });
        let color_pipeline = (material.blend != ParticleBlendMode::Distortion).then(|| {
            create_pipeline(
                device,
                &shader,
                &layout,
                color_format,
                "fs_color",
                color_blend_state(material.blend),
                "Particle Color Pipeline",
            )
        });
        let distortion_pipeline = (material.blend == ParticleBlendMode::Distortion).then(|| {
            create_pipeline(
                device,
                &shader,
                &layout,
                DISTORTION_FORMAT,
                "fs_distortion",
                Some(wgpu::BlendState::ALPHA_BLENDING),
                "Particle Distortion Pipeline",
            )
        });
        let instance_capacity = 1;
        let instance_buffer = create_instance_buffer(device, instance_capacity);
        Ok(Self {
            scene_buffer,
            scene_bind_group,
            material_bind_group,
            instance_buffer,
            instance_capacity,
            instance_count: 0,
            color_pipeline,
            distortion_pipeline,
        })
    }

    /// Updates camera, depth reconstruction, and light-volume addressing.
    pub fn update_scene(&self, queue: &wgpu::Queue, scene: &ParticleScene) {
        let packed = PackedParticleScene::from_scene(scene);
        queue.write_buffer(&self.scene_buffer, 0, bytemuck::bytes_of(&packed));
    }

    /// Uploads resolved particle instances, growing the vertex buffer when
    /// necessary.
    ///
    /// # Errors
    ///
    /// Returns [`ParticleError::TooManyInstances`] when the draw count cannot
    /// fit the GPU's `u32` instance range.
    pub fn update_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[ParticleInstance],
    ) -> Result<(), ParticleError> {
        self.instance_count =
            u32::try_from(instances.len()).map_err(|_| ParticleError::TooManyInstances {
                actual: instances.len(),
            })?;
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = create_instance_buffer(device, self.instance_capacity);
        }
        if instances.is_empty() {
            return Ok(());
        }
        let packed = instances
            .iter()
            .copied()
            .map(ParticleInstance::packed)
            .collect::<Vec<_>>();
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&packed));
        Ok(())
    }

    /// Draws non-distortion particles into the HDR scene target.
    pub fn render_color<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        let Some(pipeline) = &self.color_pipeline else {
            return;
        };
        self.render_with_pipeline(pass, pipeline);
    }

    /// Draws distortion particles into the compositor's signed-offset target.
    pub fn render_distortion<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        let Some(pipeline) = &self.distortion_pipeline else {
            return;
        };
        self.render_with_pipeline(pass, pipeline);
    }

    fn render_with_pipeline<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        pipeline: &'pass wgpu::RenderPipeline,
    ) {
        if self.instance_count == 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        pass.set_bind_group(1, &self.material_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..COLOR_VERTEX_COUNT, 0..self.instance_count);
    }
}

impl WorldRenderer for ParticleRenderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        match phase {
            RenderPhase::World => self.render_color(pass),
            RenderPhase::Distortion => self.render_distortion(pass),
            RenderPhase::Sky | RenderPhase::Shadow { .. } => {}
        }
    }
}

/// Particle asset or upload error.
#[derive(Debug, thiserror::Error)]
pub enum ParticleError {
    /// The requested DDX texture was absent.
    #[error("particle texture not found: {0}")]
    TextureNotFound(String),
    /// The DDX texture could not be parsed or decoded.
    #[error("failed to decode particle texture '{path}': {reason}")]
    TextureDecode {
        /// Canonical game path.
        path: String,
        /// Decoder diagnostic.
        reason: String,
    },
    /// A decoded RGBA image has invalid dimensions or byte count.
    #[error(
        "invalid particle image {width}x{height}: expected {expected} RGBA bytes, got {actual}"
    )]
    InvalidImage {
        /// Width in texels.
        width: u32,
        /// Height in texels.
        height: u32,
        /// Required byte count.
        expected: usize,
        /// Supplied byte count.
        actual: usize,
    },
    /// Texture arrays require at least one layer.
    #[error("particle texture array is empty")]
    EmptyTextureArray,
    /// Every image in one GPU texture array must have identical dimensions.
    #[error(
        "particle texture layer {layer} is {}x{}, expected {}x{}",
        actual[0],
        actual[1],
        expected[0],
        expected[1]
    )]
    TextureArrayDimensions {
        /// Zero-based layer index.
        layer: usize,
        /// First-layer dimensions.
        expected: [u32; 2],
        /// Mismatched dimensions.
        actual: [u32; 2],
    },
    /// Texture-array layer count cannot fit the GPU API.
    #[error("particle texture layer count {actual} exceeds the u32 GPU range")]
    TooManyTextureLayers {
        /// Requested layer count.
        actual: usize,
    },
    /// Instance count cannot fit the GPU draw API.
    #[error("particle instance count {actual} exceeds the u32 GPU range")]
    TooManyInstances {
        /// Requested instance count.
        actual: usize,
    },
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedParticleInstance {
    position_rotation: [f32; 4],
    axis_half_length: [f32; 4],
    half_size_softness: [f32; 4],
    color: [f32; 4],
    intensity: [f32; 4],
    uv_rect0: [f32; 4],
    uv_rect1: [f32; 4],
    uv_rect2: [f32; 4],
    uv_rect_intensity: [f32; 4],
    texture_layers: [u32; 4],
    geometry: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedParticleScene {
    view_projection: [[f32; 4]; 4],
    world_to_view: [[f32; 4]; 4],
    view_to_world: [[f32; 4]; 4],
    camera_position: [f32; 4],
    viewport_depth: [f32; 4],
    light_volume_row0: [f32; 4],
    light_volume_row1: [f32; 4],
    light_volume_row2: [f32; 4],
}

impl PackedParticleScene {
    fn from_scene(scene: &ParticleScene) -> Self {
        Self {
            view_projection: scene.view_projection.to_cols_array_2d(),
            world_to_view: scene.world_to_view.to_cols_array_2d(),
            view_to_world: scene.world_to_view.inverse().to_cols_array_2d(),
            camera_position: [
                scene.camera_position[0],
                scene.camera_position[1],
                scene.camera_position[2],
                0.0,
            ],
            viewport_depth: [
                scene.viewport_size[0].to_f32().unwrap_or(f32::MAX),
                scene.viewport_size[1].to_f32().unwrap_or(f32::MAX),
                scene.depth_unproject[0],
                scene.depth_unproject[1],
            ],
            light_volume_row0: scene.light_volume_rows[0],
            light_volume_row1: scene.light_volume_rows[1],
            light_volume_row2: scene.light_volume_rows[2],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedParticleMaterial {
    flags: [u32; 4],
    hdr_scales: [f32; 4],
    light_params: [f32; 4],
}

impl PackedParticleMaterial {
    fn from_material(material: &ParticleMaterial) -> Self {
        let layer_count = material
            .diffuse
            .iter()
            .rposition(Option::is_some)
            .map_or(1, |index| index + 1);
        let mut flags = 0;
        for (enabled, flag) in [
            (material.intensity.is_some(), MATERIAL_HAS_INTENSITY),
            (material.light_volume, MATERIAL_LIGHT_VOLUME),
            (material.soft_particles, MATERIAL_SOFT_PARTICLES),
            (
                matches!(
                    material.blend,
                    ParticleBlendMode::Additive
                        | ParticleBlendMode::Subtractive
                        | ParticleBlendMode::Distortion
                ),
                MATERIAL_SOFT_FADE_RGB,
            ),
        ] {
            if enabled {
                flags |= flag;
            }
        }
        Self {
            flags: [
                flags,
                u32::try_from(layer_count).unwrap_or(1),
                material.layer_1_to_2 as u32,
                material.layer_2_to_3 as u32,
            ],
            hdr_scales: [
                material.diffuse[0]
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
                material.diffuse[1]
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
                material.diffuse[2]
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
                material
                    .intensity
                    .as_ref()
                    .map_or(1.0, ParticleTextureArray::hdr_scale),
            ],
            light_params: [material.light_volume_intensity.max(0.0), 0.0, 0.0, 0.0],
        }
    }
}

fn validate_image(width: u32, height: u32, actual: usize) -> Result<(), ParticleError> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|texels| texels.checked_mul(4))
        .unwrap_or(usize::MAX);
    if width == 0 || height == 0 || actual != expected {
        Err(ParticleError::InvalidImage {
            width,
            height,
            expected,
            actual,
        })
    } else {
        Ok(())
    }
}

fn canonical_particle_texture_path(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    let normalized = normalized
        .strip_suffix(".tga")
        .or_else(|| normalized.strip_suffix(".TGA"))
        .or_else(|| normalized.strip_suffix(".ddx"))
        .or_else(|| normalized.strip_suffix(".DDX"))
        .unwrap_or(&normalized);
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized.to_owned()
    } else {
        format!("art\\{normalized}")
    }
}

fn create_scene_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Particle Scene Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn create_material_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let mut entries = (0..4)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        })
        .collect::<Vec<_>>();
    entries.extend([
        wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ]);
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Particle Material Layout"),
        entries: &entries,
    })
}

fn create_material_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    material: &ParticleMaterial,
    scene_textures: ParticleSceneTextures<'_>,
) -> Result<wgpu::BindGroup, ParticleError> {
    let fallback = ParticleImage::from_rgba(1, 1, vec![255; 4], 1.0)?;
    let fallback_array = ParticleTextureArray::new(vec![fallback])?;
    let views = material
        .diffuse
        .iter()
        .map(|texture| {
            create_texture_array_view(device, queue, texture.as_ref().unwrap_or(&fallback_array))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let intensity = create_texture_array_view(
        device,
        queue,
        material.intensity.as_ref().unwrap_or(&fallback_array),
    )?;
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Particle Texture Sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let fallback_light_volume = create_fallback_light_volume(device, queue);
    let light_volume = scene_textures
        .light_volume
        .unwrap_or(&fallback_light_volume);
    let light_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Particle Light Volume Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let packed = PackedParticleMaterial::from_material(material);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Particle Material Uniform"),
        contents: bytemuck::bytes_of(&packed),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Particle Material Bind Group"),
        layout,
        entries: &[
            texture_entry(0, &views[0]),
            texture_entry(1, &views[1]),
            texture_entry(2, &views[2]),
            texture_entry(3, &intensity),
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            texture_entry(5, scene_textures.depth),
            texture_entry(6, light_volume),
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(&light_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: uniform.as_entire_binding(),
            },
        ],
    }))
}

fn create_texture_array_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    array: &ParticleTextureArray,
) -> Result<wgpu::TextureView, ParticleError> {
    let first = array
        .layers
        .first()
        .ok_or(ParticleError::EmptyTextureArray)?;
    let layer_count =
        u32::try_from(array.layers.len()).map_err(|_| ParticleError::TooManyTextureLayers {
            actual: array.layers.len(),
        })?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Particle Texture Array"),
        size: wgpu::Extent3d {
            width: first.width,
            height: first.height,
            depth_or_array_layers: layer_count,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, image) in array.layers.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: u32::try_from(layer).map_err(|_| ParticleError::TooManyTextureLayers {
                        actual: array.layers.len(),
                    })?,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
    }
    Ok(texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Particle Texture Array View"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    }))
}

fn create_fallback_light_volume(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("Particle Fallback Light Volume"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &[0, 0, 0, 255],
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let byte_size = capacity
        .checked_mul(mem::size_of::<PackedParticleInstance>())
        .and_then(|size| u64::try_from(size).ok())
        .unwrap_or(u64::MAX);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Particle Instance Buffer"),
        size: byte_size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn particle_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Uint32x4,
        10 => Uint32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<PackedParticleInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    fragment_entry: &str,
    blend: Option<wgpu::BlendState>,
    label: &str,
) -> wgpu::RenderPipeline {
    let constants = HashMap::new();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[particle_vertex_layout()],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                zero_initialize_workgroup_memory: false,
            },
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                zero_initialize_workgroup_memory: false,
            },
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn color_blend_state(mode: ParticleBlendMode) -> Option<wgpu::BlendState> {
    match mode {
        ParticleBlendMode::Alpha => Some(wgpu::BlendState::ALPHA_BLENDING),
        ParticleBlendMode::Additive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        }),
        ParticleBlendMode::PremultipliedAlpha => {
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING)
        }
        ParticleBlendMode::Subtractive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::ReverseSubtract,
            },
            alpha: wgpu::BlendComponent::OVER,
        }),
        ParticleBlendMode::Distortion => None,
    }
}

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::{
        PackedParticleInstance, ParticleGeometry, ParticleImage, ParticleInstance,
        ParticleTextureArray, canonical_particle_texture_path,
    };

    #[test]
    fn pfx_tga_references_resolve_to_art_ddx_stems() {
        assert_eq!(
            canonical_particle_texture_path("effects/flares/glow_01_fx.tga"),
            "art\\effects\\flares\\glow_01_fx"
        );
        assert_eq!(
            canonical_particle_texture_path("art\\effects\\white.ddx"),
            "art\\effects\\white"
        );
    }

    #[test]
    fn particle_instance_stride_is_whole_vec4_slots() {
        assert_eq!(mem::size_of::<PackedParticleInstance>(), 11 * 16);
    }

    #[test]
    fn segment_constructor_preserves_endpoints() {
        let instance = ParticleInstance::segment(
            [0.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            2.0,
            [1.0; 4],
            ParticleGeometry::Beam,
        );
        assert_eq!(
            instance.axis.map(f32::to_bits),
            [0.0, 1.0, 0.0].map(f32::to_bits)
        );
        assert_eq!(
            instance.position.map(f32::to_bits),
            [0.0, 2.0, 0.0].map(f32::to_bits)
        );
        assert_eq!(instance.half_length.to_bits(), 2.0_f32.to_bits());
    }

    #[test]
    fn texture_arrays_reject_mismatched_dimensions() {
        let first = ParticleImage::from_rgba(1, 1, vec![255; 4], 1.0).unwrap();
        let second = ParticleImage::from_rgba(2, 1, vec![255; 8], 1.0).unwrap();
        assert!(ParticleTextureArray::new(vec![first, second]).is_err());
    }
}
