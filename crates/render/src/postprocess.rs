//! HDR scene target and the tone-map path used by the PC renderer.

use bytemuck::{Pod, Zeroable};
use pipeline::hw1::LightSetData;
use wgpu::util::DeviceExt;

/// Linear HDR format used before presentation.
pub const HDR_COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Signed screen-space offset format used by the legacy distortion pass.
pub const DISTORTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Scenario-controlled tone-map values.
#[derive(Clone, Copy, Debug)]
pub struct ToneMapSettings {
    /// Whether to apply tone mapping. Disabled copies/clamps the scene target.
    pub enabled: bool,
    /// Key value used to scale scene luminance.
    pub middle_grey: f32,
    /// Minimum adapted log-average luminance.
    pub log_average_min: f32,
    /// Maximum adapted log-average luminance.
    pub log_average_max: f32,
    /// Minimum adapted white point.
    pub white_point_min: f32,
    /// Maximum adapted white point.
    pub white_point_max: f32,
}

impl Default for ToneMapSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            middle_grey: 1.0,
            log_average_min: f32::MIN_POSITIVE,
            log_average_max: f32::MAX,
            white_point_min: f32::MIN_POSITIVE,
            white_point_max: f32::MAX,
        }
    }
}

impl From<&LightSetData> for ToneMapSettings {
    fn from(lightset: &LightSetData) -> Self {
        Self {
            enabled: true,
            middle_grey: lightset.middle_grey,
            log_average_min: lightset.log_ave_min,
            log_average_max: lightset.log_ave_max,
            white_point_min: lightset.white_point_min,
            white_point_max: lightset.white_point_max,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ToneMapUniform {
    output_size: [u32; 2],
    enabled: u32,
    _padding: u32,
    middle_grey: f32,
    log_average_min: f32,
    log_average_max: f32,
    white_point_min: f32,
    white_point_max: f32,
    _padding2: [f32; 3],
}

impl ToneMapUniform {
    fn new(size: [u32; 2], settings: ToneMapSettings) -> Self {
        Self {
            output_size: size,
            enabled: u32::from(settings.enabled),
            _padding: 0,
            middle_grey: settings.middle_grey,
            log_average_min: settings.log_average_min,
            log_average_max: settings.log_average_max,
            white_point_min: settings.white_point_min,
            white_point_max: settings.white_point_max,
            _padding2: [0.0; 3],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ReductionUniform {
    source_size: [u32; 2],
    source_is_scene: u32,
    _padding: u32,
}

struct ReductionLevel {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: [u32; 2],
}

struct ToneMapTargets {
    _scene_texture: wgpu::Texture,
    scene_view: wgpu::TextureView,
    _distortion_texture: wgpu::Texture,
    distortion_view: wgpu::TextureView,
    _tone_sampler: wgpu::Sampler,
    reduction_levels: Vec<ReductionLevel>,
    _reduction_buffers: Vec<wgpu::Buffer>,
    reduction_bind_groups: Vec<wgpu::BindGroup>,
    tone_bind_group: wgpu::BindGroup,
    size: [u32; 2],
}

/// Size-dependent resources for HDR rendering, luminance reduction, and tone
/// mapping into an SDR output target.
pub struct ToneMapResources {
    reduction_layout: wgpu::BindGroupLayout,
    tone_layout: wgpu::BindGroupLayout,
    reduction_pipeline: wgpu::ComputePipeline,
    tone_pipeline: wgpu::RenderPipeline,
    settings_buffer: wgpu::Buffer,
    targets: ToneMapTargets,
}

impl ToneMapResources {
    /// Creates an HDR target matching `size` and a tone-map pipeline targeting
    /// `output_format`.
    #[must_use]
    pub fn new(device: &wgpu::Device, output_format: wgpu::TextureFormat, size: [u32; 2]) -> Self {
        let reduction_layout = create_reduction_layout(device);
        let tone_layout = create_tone_layout(device);
        let settings_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tone Map Settings"),
            contents: bytemuck::bytes_of(&ToneMapUniform::new(size, ToneMapSettings::default())),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let reduction_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tone Map Luminance Reduction"),
            source: wgpu::ShaderSource::Wgsl(REDUCTION_SHADER.into()),
        });
        let reduction_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Tone Map Reduction Pipeline Layout"),
                bind_group_layouts: &[&reduction_layout],
                push_constant_ranges: &[],
            });
        let reduction_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Tone Map Reduction Pipeline"),
            layout: Some(&reduction_pipeline_layout),
            module: &reduction_shader,
            entry_point: Some("reduce_luminance"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let tone_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tone Map Shader Translation"),
            source: wgpu::ShaderSource::Wgsl(TONE_MAP_SHADER.into()),
        });
        let tone_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tone Map Pipeline Layout"),
            bind_group_layouts: &[&tone_layout],
            push_constant_ranges: &[],
        });
        let tone_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Tone Map Pipeline"),
            layout: Some(&tone_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &tone_shader,
                entry_point: Some("fullscreen_vertex"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &tone_shader,
                entry_point: Some("tone_map_fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let targets = create_targets(
            device,
            size,
            &reduction_layout,
            &tone_layout,
            &settings_buffer,
        );
        Self {
            reduction_layout,
            tone_layout,
            reduction_pipeline,
            tone_pipeline,
            settings_buffer,
            targets,
        }
    }

    /// Recreates the HDR and reduction targets for a non-zero output size.
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if size[0] == 0 || size[1] == 0 || self.targets.size == size {
            return;
        }
        self.targets = create_targets(
            device,
            size,
            &self.reduction_layout,
            &self.tone_layout,
            &self.settings_buffer,
        );
    }

    /// Returns the linear HDR color attachment used for scene rendering.
    #[must_use]
    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.targets.scene_view
    }

    /// Returns the signed screen-space offset attachment used by distortion
    /// materials. It must be cleared to transparent black before each frame.
    #[must_use]
    pub fn distortion_view(&self) -> &wgpu::TextureView {
        &self.targets.distortion_view
    }

    /// Updates scenario tone-map parameters.
    pub fn update(&self, queue: &wgpu::Queue, settings: ToneMapSettings) {
        let uniform = ToneMapUniform::new(self.targets.size, settings);
        queue.write_buffer(&self.settings_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Reduces scene luminance and presents the tone-mapped result.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Tone Map Luminance Reduction Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.reduction_pipeline);
            for (level, bind_group) in self
                .targets
                .reduction_levels
                .iter()
                .zip(&self.targets.reduction_bind_groups)
            {
                pass.set_bind_group(0, bind_group, &[]);
                pass.dispatch_workgroups(level.size[0].div_ceil(8), level.size[1].div_ceil(8), 1);
            }
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Tone Map Presentation Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.tone_pipeline);
        pass.set_bind_group(0, &self.targets.tone_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn create_reduction_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Tone Map Reduction Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rg32Float,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_tone_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Tone Map Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_targets(
    device: &wgpu::Device,
    size: [u32; 2],
    reduction_layout: &wgpu::BindGroupLayout,
    tone_layout: &wgpu::BindGroupLayout,
    settings_buffer: &wgpu::Buffer,
) -> ToneMapTargets {
    let size = [size[0].max(1), size[1].max(1)];
    let (scene_texture, scene_view) = create_scene_target(device, size);
    let (distortion_texture, distortion_view) = create_distortion_target(device, size);
    let tone_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Tone Map Scene Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let reduction_levels = create_reduction_levels(device, size);
    let (reduction_buffers, reduction_bind_groups) = create_reduction_bindings(
        device,
        size,
        &scene_view,
        &reduction_levels,
        reduction_layout,
    );
    let final_reduction = &reduction_levels
        .last()
        .expect("the reduction chain always contains a 1x1 level")
        .view;
    let tone_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Tone Map Bind Group"),
        layout: tone_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&scene_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(final_reduction),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: settings_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&distortion_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&tone_sampler),
            },
        ],
    });
    ToneMapTargets {
        _scene_texture: scene_texture,
        scene_view,
        _distortion_texture: distortion_texture,
        distortion_view,
        _tone_sampler: tone_sampler,
        reduction_levels,
        _reduction_buffers: reduction_buffers,
        reduction_bind_groups,
        tone_bind_group,
        size,
    }
}

fn create_scene_target(
    device: &wgpu::Device,
    size: [u32; 2],
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("HDR Scene Texture"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_distortion_target(
    device: &wgpu::Device,
    size: [u32; 2],
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("UGX Distortion Texture"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DISTORTION_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_reduction_levels(device: &wgpu::Device, size: [u32; 2]) -> Vec<ReductionLevel> {
    let mut reduction_levels = Vec::new();
    let mut level_size = [size[0].div_ceil(2), size[1].div_ceil(2)];
    loop {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Tone Map Reduction Texture"),
            size: wgpu::Extent3d {
                width: level_size[0],
                height: level_size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        reduction_levels.push(ReductionLevel {
            _texture: texture,
            view,
            size: level_size,
        });
        if level_size == [1, 1] {
            break;
        }
        level_size = [level_size[0].div_ceil(2), level_size[1].div_ceil(2)];
    }
    reduction_levels
}

fn create_reduction_bindings(
    device: &wgpu::Device,
    scene_size: [u32; 2],
    scene_view: &wgpu::TextureView,
    levels: &[ReductionLevel],
    layout: &wgpu::BindGroupLayout,
) -> (Vec<wgpu::Buffer>, Vec<wgpu::BindGroup>) {
    let mut reduction_buffers = Vec::with_capacity(levels.len());
    let mut reduction_bind_groups = Vec::with_capacity(levels.len());
    for (index, level) in levels.iter().enumerate() {
        let source_size = if index == 0 {
            scene_size
        } else {
            levels[index - 1].size
        };
        let uniform = ReductionUniform {
            source_size,
            source_is_scene: u32::from(index == 0),
            _padding: 0,
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tone Map Reduction Uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let source_view = if index == 0 {
            scene_view
        } else {
            &levels[index - 1].view
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tone Map Reduction Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&level.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        reduction_buffers.push(buffer);
        reduction_bind_groups.push(bind_group);
    }
    (reduction_buffers, reduction_bind_groups)
}

const REDUCTION_SHADER: &str = r"
struct ReductionUniform {
    source_size: vec2<u32>,
    source_is_scene: u32,
    padding: u32,
};

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var output_texture: texture_storage_2d<rg32float, write>;
@group(0) @binding(2) var<uniform> params: ReductionUniform;

@compute @workgroup_size(8, 8)
fn reduce_luminance(@builtin(global_invocation_id) id: vec3<u32>) {
    let output_size = textureDimensions(output_texture);
    if any(id.xy >= output_size) {
        return;
    }

    var log_sum = 0.0;
    var maximum = 0.0;
    let source_origin = id.xy * 2u;
    for (var y = 0u; y < 2u; y += 1u) {
        for (var x = 0u; x < 2u; x += 1u) {
            let source_position = source_origin + vec2<u32>(x, y);
            if all(source_position < params.source_size) {
                let value = textureLoad(source_texture, vec2<i32>(source_position), 0);
                if params.source_is_scene != 0u {
                    let color = max(value.rgb, vec3<f32>(0.0));
                    let luminance = dot(color, vec3<f32>(0.213, 0.715, 0.072));
                    log_sum += log(luminance + 0.00006103515625);
                    maximum = max(maximum, luminance);
                } else {
                    log_sum += value.x;
                    maximum = max(maximum, value.y);
                }
            }
        }
    }
    textureStore(output_texture, vec2<i32>(id.xy), vec4<f32>(log_sum, maximum, 0.0, 1.0));
}
";

const TONE_MAP_SHADER: &str = r"
struct ToneMapUniform {
    output_size: vec2<u32>,
    enabled: u32,
    padding: u32,
    middle_grey: f32,
    log_average_min: f32,
    log_average_max: f32,
    white_point_min: f32,
    white_point_max: f32,
    padding2_x: f32,
    padding2_y: f32,
    padding2_z: f32,
};

@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var luminance_texture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: ToneMapUniform;
@group(0) @binding(3) var distortion_texture: texture_2d<f32>;
@group(0) @binding(4) var scene_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn fullscreen_vertex(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    var output: VertexOutput;
    output.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return output;
}

@fragment
fn tone_map_fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let output_size = vec2<f32>(params.output_size);
    let screen_uv = input.position.xy / output_size;
    let distortion = textureSample(distortion_texture, scene_sampler, screen_uv).xy;
    let scene_uv = clamp(
        screen_uv + distortion,
        vec2<f32>(0.0),
        vec2<f32>(1.0),
    );
    let scene = max(textureSample(scene_texture, scene_sampler, scene_uv).rgb, vec3<f32>(0.0));
    if params.enabled == 0u {
        return vec4<f32>(clamp(scene, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    }

    let reduced = textureLoad(luminance_texture, vec2<i32>(0), 0).xy;
    let pixel_count = f32(params.output_size.x * params.output_size.y);
    let log_average = clamp(
        exp(reduced.x / pixel_count),
        params.log_average_min,
        params.log_average_max,
    );
    let white_point = clamp(
        reduced.y,
        params.white_point_min,
        params.white_point_max,
    );
    let luminance = dot(scene, vec3<f32>(0.213, 0.715, 0.072));
    let scaled_luminance = luminance * params.middle_grey / (log_average + 0.00001);
    let mapped_luminance = scaled_luminance
        * (1.0 + scaled_luminance / (white_point * white_point + 0.00001))
        / (scaled_luminance + 1.0);
    let mapped = mapped_luminance * scene / (luminance + 0.00001);
    return vec4<f32>(clamp(mapped, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
";

#[cfg(test)]
mod tests {
    use super::{ToneMapSettings, ToneMapUniform};

    #[test]
    fn lightset_tone_map_fields_are_preserved() {
        let lightset = pipeline::hw1::LightSetData {
            middle_grey: 0.7,
            log_ave_min: 0.721,
            log_ave_max: 32.0,
            white_point_min: 20.0,
            white_point_max: 32.0,
            ..Default::default()
        };
        let settings = ToneMapSettings::from(&lightset);
        let uniform = ToneMapUniform::new([1024, 512], settings);
        assert_eq!(uniform.output_size, [1024, 512]);
        assert_eq!(uniform.enabled, 1);
        assert_close(uniform.middle_grey, 0.7);
        assert_close(uniform.log_average_min, 0.721);
        assert_close(uniform.log_average_max, 32.0);
        assert_close(uniform.white_point_min, 20.0);
        assert_close(uniform.white_point_max, 32.0);
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }
}
