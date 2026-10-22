//! GPU pass of the hit-point bars.

use std::collections::HashMap;
use std::mem;
use std::ops::Range;

use pipeline::source::{AssetSource, StdFileProvider};
use zerocopy::IntoBytes;

use super::{
    HpBarColorStages, HpBarInstance, HpBarStyle, HpBarStyleId, load_hp_bar_images, stage_color,
    texture_key,
};
use crate::gpu;
use crate::terrain::TerrainPatchImage;
use crate::wgpu;
use crate::world_view::WorldCamera;

const SHADER: &str = include_str!("hp_bars.wgsl");

/// Fill rectangle of an absent fill. It is outside the 0..1 UV range of the quad.
const ABSENT_RECT: [f32; 4] = [-1.0; 4];

/// Uniforms of the bar pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, zerocopy::IntoBytes, zerocopy::Immutable)]
struct PackedCamera {
    view_projection: [[f32; 4]; 4],
    right: [f32; 4],
    up: [f32; 4],
    output: [f32; 4],
}

/// One bar in the instance stream. The fields match `InstanceInput` of the shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, zerocopy::IntoBytes, zerocopy::Immutable)]
struct PackedHpBar {
    position_width: [f32; 4],
    fill_height: [f32; 4],
    player_color: [f32; 4],
    hitpoint_color: [f32; 4],
    shield_color: [f32; 4],
    ammo_color: [f32; 4],
    hitpoint_uv: [f32; 4],
    shield_uv: [f32; 4],
    ammo_uv: [f32; 4],
}

/// The fill rectangles of a style and the bind group of its textures.
#[derive(Clone, Copy, Debug)]
struct StyleBinding {
    hitpoint_uv: Option<[f32; 4]>,
    shield_uv: Option<[f32; 4]>,
    ammo_uv: Option<[f32; 4]>,
    bind_group: usize,
}

/// GPU state that draws the hit-point bars of each frame.
pub struct HpBarRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    texture_bind_groups: Vec<wgpu::BindGroup>,
    styles: Vec<Option<StyleBinding>>,
    names: HashMap<String, HpBarStyleId>,
    color_stages: HpBarColorStages,
    output_srgb: bool,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    sorted: Vec<(usize, PackedHpBar)>,
    packed: Vec<PackedHpBar>,
    draws: Vec<(usize, Range<u32>)>,
}

impl HpBarRenderer {
    /// Loads the textures of all styles from `source`, then makes the renderer.
    ///
    /// A style with a missing or bad texture does not draw. See [`load_hp_bar_images`].
    #[must_use]
    pub fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_format: wgpu::TextureFormat,
        source: &mut AssetSource<StdFileProvider>,
        styles: &[HpBarStyle],
        color_stages: &HpBarColorStages,
    ) -> Self {
        let images = load_hp_bar_images(source, styles);
        Self::new(device, queue, output_format, styles, color_stages, &images)
    }

    /// Uploads the style textures and makes the bar pipeline.
    ///
    /// `images` maps the texture paths of the styles (as in [`HpBarStyle::texture`] and
    /// [`HpBarStyle::mask_texture`]) to decoded images. The path match ignores the case and
    /// the slash direction. A style without both images does not draw.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_format: wgpu::TextureFormat,
        styles: &[HpBarStyle],
        color_stages: &HpBarColorStages,
        images: &HashMap<String, TerrainPatchImage>,
    ) -> Self {
        let images = images
            .iter()
            .map(|(path, image)| (texture_key(path), image))
            .collect::<HashMap<_, _>>();
        let texture_layout = create_texture_layout(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("HP Bar Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut views = HashMap::<String, wgpu::TextureView>::new();
        let mut pairs = HashMap::<(String, String), usize>::new();
        let mut texture_bind_groups = Vec::new();
        let mut names = HashMap::new();
        let mut bindings = Vec::with_capacity(styles.len());
        for (index, style) in styles.iter().enumerate() {
            let frame_key = texture_key(&style.texture);
            let mask_key = texture_key(&style.mask_texture);
            let (Some(frame), Some(mask)) = (images.get(&frame_key), images.get(&mask_key)) else {
                bindings.push(None);
                continue;
            };
            for (key, image) in [(&frame_key, frame), (&mask_key, mask)] {
                views
                    .entry(key.clone())
                    .or_insert_with(|| create_image_view(device, queue, key, image));
            }
            let pair = (frame_key, mask_key);
            let bind_group = if let Some(&bind_group) = pairs.get(&pair) {
                bind_group
            } else {
                let bind_group = texture_bind_groups.len();
                texture_bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("HP Bar Textures"),
                    layout: &texture_layout,
                    entries: &[
                        gpu::texture_entry(0, &views[&pair.0]),
                        gpu::texture_entry(1, &views[&pair.1]),
                        gpu::sampler_entry(2, &sampler),
                    ],
                }));
                pairs.insert(pair, bind_group);
                bind_group
            };
            names.insert(style.name.to_ascii_lowercase(), HpBarStyleId(index));
            bindings.push(Some(StyleBinding {
                hitpoint_uv: style.hitpoint_uv,
                shield_uv: style.shield_uv,
                ammo_uv: style.ammo_uv,
                bind_group,
            }));
        }

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HP Bar Camera Layout"),
            entries: &[gpu::buffer_layout_entry(
                0,
                wgpu::ShaderStages::VERTEX_FRAGMENT,
                wgpu::BufferBindingType::Uniform,
            )],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HP Bar Camera"),
            size: mem::size_of::<PackedCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HP Bar Camera"),
            layout: &camera_layout,
            entries: &[gpu::buffer_entry(0, &camera_buffer)],
        });
        let pipeline = create_pipeline(device, output_format, &camera_layout, &texture_layout);
        let instance_capacity = 1;
        Self {
            pipeline,
            camera_buffer,
            camera_bind_group,
            texture_bind_groups,
            styles: bindings,
            names,
            color_stages: color_stages.clone(),
            output_srgb: output_format.is_srgb(),
            instance_buffer: create_instance_buffer(device, instance_capacity),
            instance_capacity,
            sorted: Vec::new(),
            packed: Vec::new(),
            draws: Vec::new(),
        }
    }

    /// Returns the style with `name` (the match ignores the case), if its textures loaded.
    #[must_use]
    pub fn style(&self, name: &str) -> Option<HpBarStyleId> {
        self.names.get(&name.to_ascii_lowercase()).copied()
    }

    /// Returns true if the current frame has bars to draw.
    #[must_use]
    pub fn has_bars(&self) -> bool {
        !self.draws.is_empty()
    }

    /// Replaces the bars of the frame and updates the camera.
    ///
    /// A bar with an unknown style, a style that did not load, or a position or size that is
    /// not finite does not draw.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &WorldCamera,
        bars: &[HpBarInstance],
    ) {
        self.sorted.clear();
        self.packed.clear();
        self.draws.clear();
        for bar in bars {
            let Some(Some(style)) = self.styles.get(bar.style.0) else {
                continue;
            };
            if let Some(packed) = pack_bar(bar, style, &self.color_stages) {
                self.sorted.push((style.bind_group, packed));
            }
        }
        if self.sorted.is_empty() {
            return;
        }
        // A stable sort keeps the caller order inside each texture batch.
        self.sorted.sort_by_key(|(bind_group, _)| *bind_group);
        for (index, (bind_group, packed)) in self.sorted.iter().enumerate() {
            let (Ok(start), Ok(end)) = (u32::try_from(index), u32::try_from(index + 1)) else {
                break;
            };
            match self.draws.last_mut() {
                Some((last, range)) if last == bind_group => range.end = end,
                _ => self.draws.push((*bind_group, start..end)),
            }
            self.packed.push(*packed);
        }
        if self.packed.len() > self.instance_capacity {
            self.instance_capacity = self.packed.len().next_power_of_two();
            self.instance_buffer = create_instance_buffer(device, self.instance_capacity);
        }
        queue.write_buffer(&self.instance_buffer, 0, self.packed.as_bytes());
        let right = camera.world_to_view.row(0).truncate().normalize_or_zero();
        let up = camera.world_to_view.row(1).truncate().normalize_or_zero();
        let uniforms = PackedCamera {
            view_projection: camera.view_projection.to_cols_array_2d(),
            right: right.extend(0.0).to_array(),
            up: up.extend(0.0).to_array(),
            output: [f32::from(u8::from(self.output_srgb)), 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.camera_buffer, 0, uniforms.as_bytes());
    }

    /// Draws the bars of the frame into a pass without a depth attachment.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.draws.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        for (bind_group, range) in &self.draws {
            pass.set_bind_group(1, &self.texture_bind_groups[*bind_group], &[]);
            pass.draw(0..6, range.clone());
        }
    }
}

/// Packs one bar. Returns `None` for a bar that cannot draw.
fn pack_bar(
    bar: &HpBarInstance,
    style: &StyleBinding,
    stages: &HpBarColorStages,
) -> Option<PackedHpBar> {
    let [width, height] = bar.size;
    let valid = bar.position.is_finite()
        && width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0;
    if !valid {
        return None;
    }
    // Retail sends the fill rectangle of the style even if the object has no shield or
    // ammunition; the fill then shows empty.
    let hitpoints = clamp_ratio(bar.hitpoints);
    let shield = clamp_ratio(bar.shield.unwrap_or(0.0));
    let ammo = clamp_ratio(bar.ammo.unwrap_or(0.0));
    let player_color = bar.player_color.map(|channel| f32::from(channel) / 255.0);
    Some(PackedHpBar {
        position_width: bar.position.extend(width).to_array(),
        fill_height: [
            fill_end(style.hitpoint_uv, hitpoints),
            fill_end(style.shield_uv, shield),
            fill_end(style.ammo_uv, ammo),
            height,
        ],
        player_color: opaque(player_color),
        hitpoint_color: opaque(stage_color(&stages.hitpoint, hitpoints)),
        shield_color: opaque(stage_color(&stages.shield, shield)),
        ammo_color: opaque(stage_color(&stages.ammo, ammo)),
        hitpoint_uv: style.hitpoint_uv.unwrap_or(ABSENT_RECT),
        shield_uv: style.shield_uv.unwrap_or(ABSENT_RECT),
        ammo_uv: style.ammo_uv.unwrap_or(ABSENT_RECT),
    })
}

fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The U coordinate where a fill ends: `u1 + (u2 - u1) * ratio`, as retail computes it.
fn fill_end(rect: Option<[f32; 4]>, ratio: f32) -> f32 {
    rect.map_or(ABSENT_RECT[0], |[u1, _, u2, _]| u1 + (u2 - u1) * ratio)
}

fn opaque([red, green, blue]: [f32; 3]) -> [f32; 4] {
    [red, green, blue, 1.0]
}

fn create_image_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    image: &TerrainPatchImage,
) -> wgpu::TextureView {
    gpu::create_mipped_rgba_texture(
        device,
        queue,
        label,
        [image.width, image.height],
        &image.pixels,
        wgpu::TextureFormat::Rgba8Unorm,
    )
    .create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture = |binding| {
        gpu::texture_layout_entry(
            binding,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Float { filterable: true },
            wgpu::TextureViewDimension::D2,
        )
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("HP Bar Texture Layout"),
        entries: &[
            texture(0),
            texture(1),
            gpu::filtering_sampler_layout_entry(2, wgpu::ShaderStages::FRAGMENT),
        ],
    })
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let size = capacity
        .checked_mul(mem::size_of::<PackedHpBar>())
        .and_then(|size| u64::try_from(size).ok())
        .unwrap_or(u64::MAX);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("HP Bar Instances"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    output_format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    const ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
    ];
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("HP Bar Shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("HP Bar Pipeline Layout"),
        bind_group_layouts: &[camera_layout, texture_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("HP Bar Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<PackedHpBar>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &ATTRIBUTES,
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            // Retail: SrcAlpha / InvSrcAlpha for the color. The alpha keeps the output
            // opaque where it was opaque.
            targets: &[Some(wgpu::ColorTargetState {
                format: output_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        // Retail: depth test off, so the bars draw over all geometry.
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::hp_bars::ColorStage;

    #[test]
    fn packed_bar_follows_the_retail_fill_rules() {
        let style = StyleBinding {
            hitpoint_uv: Some([0.05, 0.23, 0.95, 0.43]),
            shield_uv: Some([0.05, 0.49, 0.8, 0.69]),
            ammo_uv: None,
            bind_group: 0,
        };
        let stages = HpBarColorStages {
            hitpoint: vec![
                ColorStage {
                    value: 0.38,
                    intensity: 1.0,
                    color: [208, 16, 16],
                },
                ColorStage {
                    value: 1.0,
                    intensity: 1.0,
                    color: [146, 228, 65],
                },
            ],
            ..HpBarColorStages::default()
        };
        let bar = HpBarInstance {
            position: Vec3::new(1.0, 2.0, 3.0),
            size: [8.0, 2.0],
            style: HpBarStyleId(0),
            hitpoints: 0.25,
            shield: None,
            ammo: Some(0.5),
            player_color: [255, 0, 0],
        };
        let packed = pack_bar(&bar, &style, &stages).expect("bar is valid");
        assert_eq!(packed.position_width, [1.0, 2.0, 3.0, 8.0]);
        // 0.05 + 0.9 * 0.25; the shield without a ratio shows empty at its left edge.
        assert!((packed.fill_height[0] - 0.275).abs() < 1e-6);
        assert!((packed.fill_height[1] - 0.05).abs() < 1e-6);
        assert!((packed.fill_height[3] - 2.0).abs() < f32::EPSILON);
        assert_eq!(packed.ammo_uv, ABSENT_RECT);
        assert_eq!(packed.player_color, [1.0, 0.0, 0.0, 1.0]);
        let red = [208.0 / 255.0, 16.0 / 255.0, 16.0 / 255.0, 1.0];
        assert_eq!(packed.hitpoint_color, red);
        // The shield has no stages, so its color is the retail default black.
        assert_eq!(packed.shield_color, [0.0, 0.0, 0.0, 1.0]);

        let flat = HpBarInstance {
            size: [8.0, 0.0],
            ..bar
        };
        assert_eq!(pack_bar(&flat, &style, &stages), None);
    }
}
