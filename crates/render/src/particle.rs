//! GPU particle drawing foundation.
//!
//! The original PC path submits one point per particle and expands it in a
//! geometry shader. `wgpu` does not expose geometry shaders, so this renderer
//! performs the equivalent expansion with `vertex_index` and instance data.
//! The authored emitter runtime resolves PFX timing, motion, appearance, beams,
//! and trails into the same instance stream used by direct callers.

use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;
use pipeline::ddx::DdxTexture;
use pipeline::source::{AssetSource, StdFileProvider};
use wgpu::util::DeviceExt;

use crate::{RenderPhase, WorldRenderer};

use crate::postprocess::DISTORTION_FORMAT;

mod effect;
mod gpu;
mod image;
mod packed;
mod runtime;
#[cfg(test)]
mod tests;

use gpu::{
    create_instance_buffer, create_material_bind_group, create_material_layout, create_pipeline,
    create_scene_layout, particle_blend_state,
};
use image::substitute_failed_particle_layers;
use packed::{PackedParticleInstance, PackedParticleMaterial, PackedParticleScene};

pub(crate) use effect::canonical_effect_path;
pub use effect::{
    ParticleColorDefinition, ParticleColorKey, ParticleColorKind, ParticleColorProgression,
    ParticleEffect, ParticleEffectError, ParticleEmitter, ParticleEmitterKind,
    ParticleEmitterShape, ParticleEmitterShapeKind, ParticleEmitterTiming, ParticleForceDefinition,
    ParticleMagnetDefinition, ParticleMagnetKind, ParticleMaterialDefinition, ParticlePaletteEntry,
    ParticleRuntimeDefinition, ParticleScalarKey, ParticleScalarProgression,
    ParticleScalarProperty, ParticleTextureDefinition, ParticleTextureStage, ParticleTrailEmission,
    ParticleTrailUv, ParticleUvAnimation, ParticleVarying, ParticleVectorProperty,
};
pub use runtime::{
    ParticleEffectRuntime, ParticleEmitterRuntime, ParticleEmitterState, ParticleNestedEvent,
    ParticleRenderContext,
};

const SHADER: &str = include_str!("particle.wgsl");
const COLOR_VERTEX_COUNT: u32 = 12;
const MATERIAL_HAS_INTENSITY: u32 = 1 << 0;
const MATERIAL_LIGHT_VOLUME: u32 = 1 << 1;
const MATERIAL_SOFT_PARTICLES: u32 = 1 << 2;
const MATERIAL_SOFT_FADE_RGB: u32 = 1 << 3;
const MATERIAL_PREMULTIPLY_COLOR_ALPHA: u32 = 1 << 4;
const MATERIAL_ALPHA_TEST: u32 = 1 << 5;

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
    /// Quad whose surface normal follows particle velocity.
    VelocityAligned = 3,
    /// Camera-facing segment between two beam control points.
    Beam = 4,
    /// Camera-facing trail segment.
    Trail = 5,
    /// Two perpendicular trail ribbons (`eTrailCross`).
    TrailCross = 6,
    /// Horizontal patch used for terrain decals/effects.
    TerrainPatch = 7,
    /// Beam ribbon whose width follows world up.
    BeamVertical = 8,
    /// Beam ribbon whose width is horizontal to its direction.
    BeamHorizontal = 9,
}

impl ParticleGeometry {
    fn is_segment(self) -> bool {
        matches!(
            self,
            Self::Beam | Self::BeamVertical | Self::BeamHorizontal | Self::Trail | Self::TrailCross
        )
    }
}

/// Fixed-function blend families authored by PFX emitters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParticleBlendMode {
    /// Source-over alpha blending (`eAlphaBlend`).
    #[default]
    Alpha,
    /// One-plus-one blending after the shader premultiplies particle alpha
    /// (`eAdditive`).
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
    /// Secondary up axis used by crossed trails.
    pub up_axis: [f32; 3],
    /// Half-length for beam/trail geometry.
    pub half_length: f32,
    /// Full quad width and height. Segment constructors seed both ribbon widths
    /// from `width`; their longitudinal extent lives in `half_length`.
    pub size: [f32; 2],
    /// Progression/tint RGBA already evaluated by the simulation layer.
    pub color: [f32; 4],
    /// RGB intensity progression. The fourth lane is reserved to preserve the
    /// retail vertex layout.
    pub intensity: [f32; 4],
    /// Per-map UV rectangles: diffuse 1/2/3 followed by intensity.
    pub uv_rects: [[f32; 4]; 4],
    /// Texture-array layer for diffuse 1/2/3 and intensity.
    pub texture_layers: [u32; 4],
    /// Geometry expansion family.
    pub geometry: ParticleGeometry,
    /// Retail multiplier applied to the particle/scene view-depth delta.
    pub soft_fade_scale: f32,
}

impl ParticleInstance {
    /// Creates a camera-facing particle.
    #[must_use]
    pub fn billboard(position: [f32; 3], size: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            rotation: 0.0,
            axis: [0.0, 1.0, 0.0],
            up_axis: [0.0, 1.0, 0.0],
            half_length: size[1] * 0.5,
            size,
            color,
            intensity: [1.0; 4],
            uv_rects: [[0.0, 0.0, 1.0, 1.0]; 4],
            texture_layers: [0; 4],
            geometry: ParticleGeometry::Billboard,
            soft_fade_scale: 1.0,
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
        let mut instance = Self::billboard(((start + end) * 0.5).to_array(), [width, width], color);
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
            up_axis: [self.up_axis[0], self.up_axis[1], self.up_axis[2], 0.0],
            half_size_softness: [
                self.size[0].abs() * 0.5,
                self.size[1].abs() * 0.5,
                self.soft_fade_scale.max(0.0),
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

/// GPU-compatible images uploaded as one particle texture array.
///
/// Authored sets occasionally mix resolutions or reference an absent frame.
/// Since every layer is sampled in normalized UV space, smaller layers are
/// resampled to the set's largest dimensions. Failed stages reuse the first
/// decodable stage in the same set, avoiding the retail uploader's dependency
/// on unrelated texture-array load order.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleTextureArray {
    layers: Vec<ParticleImage>,
    resampled_layer_count: usize,
    fallback_layer_count: usize,
}

impl ParticleTextureArray {
    /// Creates a checked texture array.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty array or invalid image payload.
    pub fn new(mut layers: Vec<ParticleImage>) -> Result<Self, ParticleError> {
        let Some(first) = layers.first() else {
            return Err(ParticleError::EmptyTextureArray);
        };
        validate_image(first.width, first.height, first.pixels.len())?;
        let dimensions =
            layers
                .iter()
                .try_fold([first.width, first.height], |[width, height], layer| {
                    validate_image(layer.width, layer.height, layer.pixels.len())?;
                    Ok::<_, ParticleError>([width.max(layer.width), height.max(layer.height)])
                })?;
        let mut resampled_layer_count = 0;
        for layer in &mut layers {
            validate_image(layer.width, layer.height, layer.pixels.len())?;
            if [layer.width, layer.height] != dimensions {
                *layer = resize_particle_image(layer, dimensions)?;
                resampled_layer_count += 1;
            }
        }
        u32::try_from(layers.len()).map_err(|_| ParticleError::TooManyTextureLayers {
            actual: layers.len(),
        })?;
        Ok(Self {
            layers,
            resampled_layer_count,
            fallback_layer_count: 0,
        })
    }

    /// Loads all named DDX layers from PFX-style texture references.
    ///
    /// # Errors
    ///
    /// Failed stages reuse the first stage in the set that decodes. Returns an
    /// error when no authored stage can be decoded or array validation fails.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        paths: &[String],
    ) -> Result<Self, ParticleError> {
        let decoded = paths
            .iter()
            .map(|path| ParticleImage::load(source, path))
            .collect::<Vec<_>>();
        let (layers, fallback_layer_count) = substitute_failed_particle_layers(decoded)?;
        let mut array = Self::new(layers)?;
        array.fallback_layer_count = fallback_layer_count;
        Ok(array)
    }

    /// Returns decoded layers in texture-array order.
    #[must_use]
    pub fn layers(&self) -> &[ParticleImage] {
        &self.layers
    }

    /// Returns the shared layer dimensions in texels.
    #[must_use]
    pub fn dimensions(&self) -> [u32; 2] {
        let first = &self.layers[0];
        [first.width, first.height]
    }

    /// Returns how many authored layers required normalized-UV resampling.
    #[must_use]
    pub const fn resampled_layer_count(&self) -> usize {
        self.resampled_layer_count
    }

    /// Returns how many authored stages reused the first decodable layer.
    #[must_use]
    pub const fn fallback_layer_count(&self) -> usize {
        self.fallback_layer_count
    }

    fn hdr_scale(&self) -> f32 {
        self.layers
            .iter()
            .map(|image| image.hdr_scale)
            .fold(1.0_f32, f32::max)
            .max(1.0)
    }
}

fn resize_particle_image(
    image: &ParticleImage,
    [width, height]: [u32; 2],
) -> Result<ParticleImage, ParticleError> {
    let expected = image_byte_len(width, height).unwrap_or(usize::MAX);
    if width == 0 || height == 0 || expected == usize::MAX {
        return Err(ParticleError::InvalidImage {
            width,
            height,
            expected,
            actual: 0,
        });
    }
    let mut pixels = vec![0; expected];
    let source_width = image.width.to_f32().expect("texture width must fit f32");
    let source_height = image.height.to_f32().expect("texture height must fit f32");
    let x_scale = source_width / width.to_f32().expect("texture width must fit f32");
    let y_scale = source_height / height.to_f32().expect("texture height must fit f32");
    for y in 0..height {
        let source_y = ((y.to_f32().expect("texture Y must fit f32") + 0.5) * y_scale - 0.5)
            .clamp(0.0, source_height - 1.0);
        let y0 = source_y.floor().to_u32().unwrap_or(image.height - 1);
        let y1 = (y0 + 1).min(image.height - 1);
        let y_alpha = source_y - y0.to_f32().expect("texture Y must fit f32");
        for x in 0..width {
            let source_x = ((x.to_f32().expect("texture X must fit f32") + 0.5) * x_scale - 0.5)
                .clamp(0.0, source_width - 1.0);
            let x0 = source_x.floor().to_u32().unwrap_or(image.width - 1);
            let x1 = (x0 + 1).min(image.width - 1);
            let x_alpha = source_x - x0.to_f32().expect("texture X must fit f32");
            let destination = usize::try_from((y * width + x) * 4)
                .expect("validated texture index must fit usize");
            for channel in 0..4 {
                let top = sample_channel(image, x0, y0, channel).mul_add(
                    1.0 - x_alpha,
                    sample_channel(image, x1, y0, channel) * x_alpha,
                );
                let bottom = sample_channel(image, x0, y1, channel).mul_add(
                    1.0 - x_alpha,
                    sample_channel(image, x1, y1, channel) * x_alpha,
                );
                pixels[destination + channel] = top
                    .mul_add(1.0 - y_alpha, bottom * y_alpha)
                    .round()
                    .to_u8()
                    .unwrap_or(u8::MAX);
            }
        }
    }
    Ok(ParticleImage {
        width,
        height,
        pixels,
        hdr_scale: image.hdr_scale,
    })
}

fn sample_channel(image: &ParticleImage, x: u32, y: u32, channel: usize) -> f32 {
    let index = usize::try_from((y * image.width + x) * 4)
        .expect("validated texture index must fit usize")
        + channel;
    f32::from(image.pixels[index])
}

/// Static material data shared by an emitter's particle instances.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleMaterial {
    /// Up to three independently animated diffuse texture arrays.
    pub diffuse: [Option<ParticleTextureArray>; 3],
    /// Optional intensity texture array.
    pub intensity: Option<ParticleTextureArray>,
    /// Authored optional texture sets that could not be decoded and were disabled.
    pub unavailable_texture_sets: usize,
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
    /// Authored quad-corner modulation colors in retail vertex order.
    pub corner_colors: [[f32; 4]; 4],
}

impl Default for ParticleMaterial {
    fn default() -> Self {
        Self {
            diffuse: [None, None, None],
            intensity: None,
            unavailable_texture_sets: 0,
            layer_1_to_2: ParticleLayerBlend::Multiply,
            layer_2_to_3: ParticleLayerBlend::Multiply,
            blend: ParticleBlendMode::Alpha,
            soft_particles: false,
            light_volume: false,
            light_volume_intensity: 1.0,
            corner_colors: [[1.0; 4]; 4],
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
    /// Color-field decode scale multiplied by the scenario particle-light scale.
    pub light_volume_intensity_scale: f32,
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
            light_volume_intensity_scale: crate::light_volume::LIGHT_VOLUME_DECODE_SCALE,
        }
    }

    /// Supplies shared field addressing and the scenario's particle-light scale.
    #[must_use]
    pub fn with_light_volume(mut self, rows: [[f32; 4]; 3], particle_intensity_scale: f32) -> Self {
        self.light_volume_rows = rows;
        self.light_volume_intensity_scale =
            crate::light_volume::LIGHT_VOLUME_DECODE_SCALE * particle_intensity_scale.max(0.0);
        self
    }

    /// Derives the retail reciprocal eye-depth reconstruction coefficients
    /// from a right-handed perspective projection matrix.
    #[must_use]
    pub fn perspective_depth_unproject(projection: Mat4) -> [f32; 2] {
        let depth_offset = projection.w_axis.z;
        if !depth_offset.is_finite() || depth_offset.abs() <= f32::EPSILON {
            return [0.0, 0.0];
        }
        [depth_offset.recip(), projection.z_axis.z / depth_offset]
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
                Some(particle_blend_state(material.blend)),
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
                Some(particle_blend_state(material.blend)),
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

    /// Evaluates a live authored emitter and uploads its resolved instances.
    ///
    /// # Errors
    ///
    /// Returns [`ParticleError::TooManyInstances`] when the evaluated draw
    /// count cannot fit the GPU's `u32` instance range.
    pub fn update_emitter_runtime(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        runtime: &ParticleEmitterRuntime,
        material: &ParticleMaterial,
        context: ParticleRenderContext,
    ) -> Result<(), ParticleError> {
        let instances = runtime.instances(material, context);
        self.update_instances(device, queue, &instances)
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
            RenderPhase::Sky | RenderPhase::Shadow { .. } | RenderPhase::LocalShadow { .. } => {}
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

fn validate_image(width: u32, height: u32, actual: usize) -> Result<(), ParticleError> {
    let expected = image_byte_len(width, height).unwrap_or(usize::MAX);
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

fn image_byte_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|texels| texels.checked_mul(4))
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
