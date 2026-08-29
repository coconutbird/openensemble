use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use glam::Mat4;
use wgpu::util::DeviceExt;

use super::model::{BlendMode, Image, Material, MaterialFeature, Model};
use crate::environment::EnvironmentMap;
use crate::gpu::texture_entry;
use crate::lighting::LocalLightBuffer;
use crate::terrain::{LightingParams, TerrainHeightfield};
use crate::{RenderPhase, WorldRenderer};

mod pipelines;
mod texture;
mod uniforms;

#[cfg(test)]
mod tests;

use pipelines::{
    create_distortion_pipelines, create_material_layout, create_pipelines, create_scene_layout,
    create_shadow_layout, create_shadow_pipelines, create_sky_pipelines, pipeline_index,
};
use texture::{
    create_fallback_shadow_view, create_fallback_terrain_heightfield_view,
    create_fallback_volume_view, create_texture_view,
};
pub(super) use uniforms::SelectionOverlay;
use uniforms::{MaterialUniform, SceneUniform};

const SHADER: &str = include_str!("shader.wgsl");

struct GpuMaterial {
    bind_group: wgpu::BindGroup,
    blend: BlendMode,
    pipeline_index: usize,
    two_sided_pipeline_index: usize,
    has_distortion: bool,
    casts_shadows: bool,
}

struct GpuModel {
    materials: Vec<GpuMaterial>,
    sections: Vec<GpuSection>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TextureIdentity {
    Asset(String),
    Fallback([u8; 4]),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextureCacheKey {
    identity: TextureIdentity,
    format: wgpu::TextureFormat,
}

#[derive(Clone, Copy)]
struct MaterialTextureSlot {
    name: &'static str,
    fallback: [u8; 4],
    format: wgpu::TextureFormat,
}

struct MaterialTextureViews {
    diffuse: wgpu::TextureView,
    normal: wgpu::TextureView,
    gloss: wgpu::TextureView,
    opacity: wgpu::TextureView,
    xform: wgpu::TextureView,
    emissive: wgpu::TextureView,
    ao: wgpu::TextureView,
    environment_mask: wgpu::TextureView,
    emissive_xform: wgpu::TextureView,
    distortion: wgpu::TextureView,
    highlight: wgpu::TextureView,
    modulate: wgpu::TextureView,
}

impl MaterialTextureViews {
    fn new(
        shared: &SharedResources,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        material: &Material,
    ) -> Self {
        let texture =
            |image, slot| shared.material_texture_view(device, queue, material, image, slot);
        Self {
            diffuse: texture(
                material.diffuse.as_ref(),
                MaterialTextureSlot {
                    name: "Diffuse",
                    fallback: [255; 4],
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                },
            ),
            normal: texture(
                material.normal.as_ref(),
                MaterialTextureSlot {
                    name: "Normal",
                    fallback: [128, 128, 255, 255],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            gloss: texture(
                material.gloss.as_ref(),
                MaterialTextureSlot {
                    name: "Gloss",
                    fallback: [255; 4],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            opacity: texture(
                material.opacity_map.as_ref(),
                MaterialTextureSlot {
                    name: "Opacity",
                    fallback: [255; 4],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            xform: texture(
                material.xform.as_ref(),
                MaterialTextureSlot {
                    name: "XForm",
                    fallback: [0, 0, 0, 255],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            emissive: texture(
                material.emissive.as_ref(),
                MaterialTextureSlot {
                    name: "Emissive",
                    fallback: [0; 4],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            ao: texture(
                material.ao.as_ref(),
                MaterialTextureSlot {
                    name: "AO",
                    fallback: [255; 4],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            environment_mask: texture(
                material.environment_mask.as_ref(),
                MaterialTextureSlot {
                    name: "Environment Mask",
                    fallback: [255; 4],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            emissive_xform: texture(
                material.emissive_xform.as_ref(),
                MaterialTextureSlot {
                    name: "Emissive XForm",
                    fallback: [0; 4],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            distortion: texture(
                material.distortion.as_ref(),
                MaterialTextureSlot {
                    name: "Distortion",
                    fallback: [128, 128, 0, 0],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            highlight: texture(
                material.highlight.as_ref(),
                MaterialTextureSlot {
                    name: "Highlight",
                    fallback: [0; 4],
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            ),
            modulate: texture(
                material.modulate.as_ref(),
                MaterialTextureSlot {
                    name: "Modulate",
                    fallback: [255; 4],
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                },
            ),
        }
    }
}

struct GpuSection {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    material_index: usize,
}

pub(super) struct SharedResources {
    scene_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    shadow_bind_group: wgpu::BindGroup,
    terrain_heightfield_view: wgpu::TextureView,
    terrain_heightfield_info: TerrainHeightfieldInfo,
    sampler: wgpu::Sampler,
    environment_sampler: wgpu::Sampler,
    environment_view: wgpu::TextureView,
    environment_hdr_scale: f32,
    environment_available: bool,
    environment_views: Mutex<HashMap<String, wgpu::TextureView>>,
    texture_views: Mutex<HashMap<TextureCacheKey, wgpu::TextureView>>,
    models: Mutex<HashMap<String, Arc<GpuModel>>>,
    pipelines: Vec<wgpu::RenderPipeline>,
    sky_pipelines: Vec<wgpu::RenderPipeline>,
    distortion_pipelines: [wgpu::RenderPipeline; 2],
    shadow_pipelines: Vec<wgpu::RenderPipeline>,
}

/// Scenario-global resources shared by all UGX models in one renderer.
#[derive(Clone, Copy, Default)]
pub struct WorldBindings<'a> {
    /// Scenario HDR environment cubemap.
    pub environment: Option<&'a EnvironmentMap>,
    /// Directional variance-shadow array.
    pub directional_shadow: Option<&'a wgpu::TextureView>,
    /// Accepted-axis terrain position texture for conforming materials.
    pub terrain_heightfield: Option<TerrainHeightfield<'a>>,
    /// Oracle-packed local-light storage shared with terrain and foliage.
    pub local_lights: Option<&'a LocalLightBuffer>,
    /// Local spot/omni shadow texture array.
    pub local_shadow: Option<&'a wgpu::TextureView>,
    /// Optional light-volume color field.
    pub light_volume_color: Option<&'a wgpu::TextureView>,
    /// Optional light-volume direction field.
    pub light_volume_vector: Option<&'a wgpu::TextureView>,
}

/// Scenario-global UGX pipelines and bindings shared by model and unit renderers.
///
/// Constructing the legacy material pipeline family is expensive. Create one
/// resource set per device, target format, and world binding set, then use it to
/// upload every model, unit, and scenario placement that shares those inputs.
pub struct RendererResources {
    pub(super) shared: Arc<SharedResources>,
}

impl RendererResources {
    /// Creates the complete UGX pipeline family and scenario-global bindings.
    #[must_use]
    pub fn new_with_world(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        world: WorldBindings<'_>,
    ) -> Self {
        Self {
            shared: Arc::new(SharedResources::new(device, queue, surface_format, world)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TerrainHeightfieldInfo {
    dimension: u32,
    tile_scale: f32,
    world_min_xz: [f32; 2],
    y_range: f32,
    y_mid: f32,
    normalized_y_bias: f32,
}

impl Default for TerrainHeightfieldInfo {
    fn default() -> Self {
        Self {
            dimension: 1,
            tile_scale: 1.0,
            world_min_xz: [0.0; 2],
            y_range: 0.0,
            y_mid: 0.0,
            normalized_y_bias: 0.0,
        }
    }
}

impl From<TerrainHeightfield<'_>> for TerrainHeightfieldInfo {
    fn from(heightfield: TerrainHeightfield<'_>) -> Self {
        Self {
            dimension: heightfield.dimension.max(1),
            tile_scale: heightfield.tile_scale.abs().max(f32::EPSILON),
            world_min_xz: heightfield.world_min_xz,
            y_range: heightfield.y_range,
            y_mid: heightfield.y_mid,
            normalized_y_bias: heightfield.normalized_y_bias,
        }
    }
}

struct EnvironmentResources {
    sampler: wgpu::Sampler,
    view: wgpu::TextureView,
    hdr_scale: f32,
    available: bool,
}

fn create_environment_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    environment: Option<&EnvironmentMap>,
) -> EnvironmentResources {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("UGX Environment Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let available = environment.is_some();
    let fallback = EnvironmentMap::black();
    let environment = environment.unwrap_or(&fallback);
    EnvironmentResources {
        sampler,
        view: environment.create_texture_view(device, queue, "UGX Global Environment Cubemap"),
        hdr_scale: environment.hdr_scale(),
        available,
    }
}

fn create_world_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    world: WorldBindings<'_>,
) -> wgpu::BindGroup {
    let fallback_directional = create_fallback_shadow_view(device, queue, 4, "UGX Fallback CSM");
    let directional_shadow = world.directional_shadow.unwrap_or(&fallback_directional);
    let fallback_local_lights = LocalLightBuffer::empty(device);
    let local_lights = world.local_lights.unwrap_or(&fallback_local_lights);
    let fallback_local_shadow =
        create_fallback_shadow_view(device, queue, 8, "UGX Fallback Local Shadows");
    let local_shadow = world.local_shadow.unwrap_or(&fallback_local_shadow);
    let fallback_volume_color = create_fallback_volume_view(
        device,
        queue,
        "UGX Fallback Light Volume Color",
        [0, 0, 0, 0],
    );
    let fallback_volume_vector = create_fallback_volume_view(
        device,
        queue,
        "UGX Fallback Light Volume Vector",
        [128, 128, 128, 255],
    );
    let volume_color = world.light_volume_color.unwrap_or(&fallback_volume_color);
    let volume_vector = world.light_volume_vector.unwrap_or(&fallback_volume_vector);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("UGX World Lighting Sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("UGX World Lighting Bind Group"),
        layout,
        entries: &[
            texture_entry(0, directional_shadow),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: local_lights.buffer().as_entire_binding(),
            },
            texture_entry(3, local_shadow),
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            texture_entry(5, volume_color),
            texture_entry(6, volume_vector),
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

impl SharedResources {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        world: WorldBindings<'_>,
    ) -> Self {
        let scene_layout = create_scene_layout(device);
        let material_layout = create_material_layout(device);
        let shadow_layout = create_shadow_layout(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("UGX Material Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let environment = create_environment_resources(device, queue, world.environment);
        let shadow_bind_group = create_world_bind_group(device, queue, &shadow_layout, world);
        let terrain_heightfield_view = world.terrain_heightfield.map_or_else(
            || create_fallback_terrain_heightfield_view(device, queue),
            |heightfield| heightfield.view.clone(),
        );
        let terrain_heightfield_info = world.terrain_heightfield.map_or_else(
            TerrainHeightfieldInfo::default,
            TerrainHeightfieldInfo::from,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UGX Parametric Shader Translation"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipelines = create_pipelines(
            device,
            surface_format,
            &shader,
            &scene_layout,
            &material_layout,
            &shadow_layout,
        );
        let sky_pipelines = create_sky_pipelines(
            device,
            surface_format,
            &shader,
            &scene_layout,
            &material_layout,
            &shadow_layout,
        );
        let distortion_pipelines =
            create_distortion_pipelines(device, &shader, &scene_layout, &material_layout);
        let shadow_pipelines =
            create_shadow_pipelines(device, &shader, &scene_layout, &material_layout);
        Self {
            scene_layout,
            material_layout,
            shadow_bind_group,
            terrain_heightfield_view,
            terrain_heightfield_info,
            sampler,
            environment_sampler: environment.sampler,
            environment_view: environment.view,
            environment_hdr_scale: environment.hdr_scale,
            environment_available: environment.available,
            environment_views: Mutex::new(HashMap::new()),
            texture_views: Mutex::new(HashMap::new()),
            models: Mutex::new(HashMap::new()),
            pipelines,
            sky_pipelines,
            distortion_pipelines,
            shadow_pipelines,
        }
    }

    fn environment_view_for(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        environment: Option<&EnvironmentMap>,
    ) -> wgpu::TextureView {
        environment.map_or_else(
            || self.environment_view.clone(),
            |environment| {
                let mut views = self
                    .environment_views
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                views
                    .entry(environment.path().to_owned())
                    .or_insert_with(|| {
                        environment.create_texture_view(
                            device,
                            queue,
                            &format!("UGX Environment: {}", environment.path()),
                        )
                    })
                    .clone()
            },
        )
    }

    fn create_material_bind_group(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        material: &Material,
    ) -> wgpu::BindGroup {
        let explicit_environment = material.environment.as_ref();
        let use_global_environment = explicit_environment.is_none()
            && self.environment_available
            && (material.has_feature(MaterialFeature::GLOBAL_ENVIRONMENT)
                || material.environment_mask.is_some());
        let environment_available = explicit_environment.is_some() || use_global_environment;
        let environment_hdr_scale =
            explicit_environment.map_or(self.environment_hdr_scale, EnvironmentMap::hdr_scale);
        let uniform =
            MaterialUniform::from_material(material, environment_available, environment_hdr_scale);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("UGX Material Uniform: {}", material.name)),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let textures = MaterialTextureViews::new(self, device, queue, material);
        let environment_view = self.environment_view_for(device, queue, explicit_environment);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("UGX Material Bind Group: {}", material.name)),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                texture_entry(1, &textures.diffuse),
                texture_entry(2, &textures.normal),
                texture_entry(3, &textures.gloss),
                texture_entry(4, &textures.opacity),
                texture_entry(5, &textures.xform),
                texture_entry(6, &textures.emissive),
                texture_entry(7, &textures.ao),
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                texture_entry(9, &textures.environment_mask),
                texture_entry(10, &environment_view),
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::Sampler(&self.environment_sampler),
                },
                texture_entry(12, &textures.emissive_xform),
                texture_entry(13, &textures.distortion),
                texture_entry(14, &textures.highlight),
                texture_entry(15, &textures.modulate),
            ],
        })
    }

    fn material_texture_view(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        material: &Material,
        image: Option<&Image>,
        slot: MaterialTextureSlot,
    ) -> wgpu::TextureView {
        let identity = image.map_or(TextureIdentity::Fallback(slot.fallback), |image| {
            TextureIdentity::Asset(image.asset_path.to_ascii_lowercase())
        });
        let key = TextureCacheKey {
            identity,
            format: slot.format,
        };
        let mut views = self
            .texture_views
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(view) = views.get(&key) {
            return view.clone();
        }
        let view = create_texture_view(
            device,
            queue,
            &format!("UGX {}: {}", slot.name, material.name),
            image,
            slot.fallback,
            slot.format,
        );
        views.insert(key, view.clone());
        view
    }

    fn gpu_model(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &Model,
    ) -> Arc<GpuModel> {
        let mut models = self.models.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(model) = models.get(&model.asset_path) {
            return Arc::clone(model);
        }
        let materials = model
            .materials
            .iter()
            .map(|material| {
                let bind_group = self.create_material_bind_group(device, queue, material);
                GpuMaterial {
                    bind_group,
                    blend: material.blend,
                    pipeline_index: pipeline_index(
                        material.blend,
                        material.has_feature(MaterialFeature::TWO_SIDED),
                    ),
                    two_sided_pipeline_index: usize::from(
                        material.has_feature(MaterialFeature::TWO_SIDED),
                    ),
                    has_distortion: material.distortion.is_some(),
                    casts_shadows: material.has_feature(MaterialFeature::CASTS_SHADOWS),
                }
            })
            .collect::<Vec<_>>();
        let mut sections = model
            .sections
            .iter()
            .filter(|section| !section.vertices.is_empty() && !section.indices.is_empty())
            .map(|section| GpuSection {
                vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("UGX Section Vertices"),
                    contents: bytemuck::cast_slice(&section.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("UGX Section Indices"),
                    contents: bytemuck::cast_slice(&section.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                index_count: section.index_count,
                material_index: section.material_index,
            })
            .collect::<Vec<_>>();
        sections.sort_by_key(|section| materials[section.material_index].blend.rank());
        let gpu_model = Arc::new(GpuModel {
            materials,
            sections,
        });
        models.insert(model.asset_path.clone(), Arc::clone(&gpu_model));
        gpu_model
    }
}

/// GPU resources used to draw one decoded [`Model`].
pub struct Renderer {
    shared: Arc<SharedResources>,
    scene_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    joint_buffer: wgpu::Buffer,
    gpu_model: Arc<GpuModel>,
    model_transform: Mat4,
    joint_count: usize,
}

impl Renderer {
    /// Uploads a decoded model and creates all legacy material pipelines.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        model: &Model,
        model_transform: Mat4,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            model,
            model_transform,
            WorldBindings::default(),
        )
    }

    /// Uploads a decoded model with a scenario-global environment fallback.
    #[must_use]
    pub fn new_with_environment(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        model: &Model,
        model_transform: Mat4,
        environment: Option<&EnvironmentMap>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            model,
            model_transform,
            WorldBindings {
                environment,
                ..WorldBindings::default()
            },
        )
    }

    /// Uploads a decoded model with environment and directional-shadow inputs.
    #[must_use]
    pub fn new_with_environment_and_shadow(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        model: &Model,
        model_transform: Mat4,
        environment: Option<&EnvironmentMap>,
        shadow_view: Option<&wgpu::TextureView>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            model,
            model_transform,
            WorldBindings {
                environment,
                directional_shadow: shadow_view,
                ..WorldBindings::default()
            },
        )
    }

    /// Uploads a model with every scenario-global rendering input.
    #[must_use]
    pub fn new_with_world(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        model: &Model,
        model_transform: Mat4,
        world: WorldBindings<'_>,
    ) -> Self {
        let resources = RendererResources::new_with_world(device, queue, surface_format, world);
        Self::new_with_resources(device, queue, model, model_transform, &resources)
    }

    /// Uploads a model using an existing scenario-global pipeline set.
    #[must_use]
    pub fn new_with_resources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &Model,
        model_transform: Mat4,
        resources: &RendererResources,
    ) -> Self {
        Self::new_with_shared(
            device,
            queue,
            model,
            model_transform,
            Arc::clone(&resources.shared),
        )
    }

    pub(super) fn new_with_shared(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        model: &Model,
        model_transform: Mat4,
        shared: Arc<SharedResources>,
    ) -> Self {
        let scene_uniform = SceneUniform::new(model_transform, shared.terrain_heightfield_info);
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UGX Scene Uniform"),
            contents: bytemuck::bytes_of(&scene_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let identity_joints = vec![Mat4::IDENTITY.to_cols_array_2d(); model.joint_count];
        let joint_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("UGX Matrix Palette"),
            contents: bytemuck::cast_slice(&identity_joints),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("UGX Scene Bind Group"),
            layout: &shared.scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: joint_buffer.as_entire_binding(),
                },
                texture_entry(2, &shared.terrain_heightfield_view),
            ],
        });

        let gpu_model = shared.gpu_model(device, queue, model);

        Self {
            shared,
            scene_buffer,
            scene_bind_group,
            joint_buffer,
            gpu_model,
            model_transform,
            joint_count: model.joint_count,
        }
    }

    /// Updates the camera, transform, lighting, and fog constants for a frame.
    pub fn update_frame(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        model_transform: Mat4,
        lighting: &LightingParams,
    ) {
        self.update_frame_at_time(queue, view_projection, model_transform, lighting, 0.0);
    }

    /// Updates frame constants and advances legacy material UV animation.
    pub fn update_frame_at_time(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        model_transform: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
    ) {
        self.update_frame_with_selection_at_time(
            queue,
            view_projection,
            model_transform,
            lighting,
            time_seconds,
            SelectionOverlay::default(),
        );
    }

    pub(super) fn update_frame_with_selection_at_time(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        model_transform: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
        selection: SelectionOverlay,
    ) {
        self.model_transform = model_transform;
        let uniform = SceneUniform::from_frame(
            view_projection,
            model_transform,
            lighting,
            time_seconds,
            self.shared.terrain_heightfield_info,
            selection,
        );
        queue.write_buffer(&self.scene_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Replaces the skinning palette used by the vertex shader.
    ///
    /// The default palette is identity, which is the correct bind-pose matrix
    /// (`current_world * inverse_bind`) for UGX vertices stored in model space.
    ///
    /// # Panics
    ///
    /// Panics if the supplied palette does not contain exactly the model's
    /// allocated joint count.
    pub fn update_joints(&self, queue: &wgpu::Queue, matrices: &[Mat4]) {
        assert_eq!(
            matrices.len(),
            self.joint_count,
            "UGX joint palette length must remain constant"
        );
        let columns = matrices
            .iter()
            .map(Mat4::to_cols_array_2d)
            .collect::<Vec<_>>();
        queue.write_buffer(&self.joint_buffer, 0, bytemuck::cast_slice(&columns));
    }

    /// Draws all sections using the oracle blend order.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        pass.set_bind_group(2, &self.shared.shadow_bind_group, &[]);
        for blend in BlendMode::DRAW_ORDER {
            for section in &self.gpu_model.sections {
                let material = &self.gpu_model.materials[section.material_index];
                if material.blend != blend {
                    continue;
                }
                pass.set_pipeline(&self.shared.pipelines[material.pipeline_index]);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
                pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..section.index_count, 0, 0..1);
            }
        }
    }

    /// Draws this model as a camera-relative sky background.
    ///
    /// Sky visuals retain their authored UGX materials, but use a far-plane,
    /// depth-read-only pipeline so they cannot occlude world geometry.
    pub fn render_sky<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        pass.set_bind_group(2, &self.shared.shadow_bind_group, &[]);
        for blend in BlendMode::DRAW_ORDER {
            for section in &self.gpu_model.sections {
                let material = &self.gpu_model.materials[section.material_index];
                if material.blend != blend {
                    continue;
                }
                pass.set_pipeline(&self.shared.sky_pipelines[material.pipeline_index]);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
                pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..section.index_count, 0, 0..1);
            }
        }
    }

    /// Draws sections with authored distortion maps into the signed
    /// screen-space offset target.
    pub fn render_distortion<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        for section in &self.gpu_model.sections {
            let material = &self.gpu_model.materials[section.material_index];
            if !material.has_distortion {
                continue;
            }
            pass.set_pipeline(&self.shared.distortion_pipelines[material.two_sided_pipeline_index]);
            pass.set_bind_group(1, &material.bind_group, &[]);
            pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
            pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..section.index_count, 0, 0..1);
        }
    }

    /// Draws shadow-casting sections into one directional cascade.
    ///
    /// The cascade index follows the oracle's 8x, 4x, 2x, and 1x projection
    /// scale order. Out-of-range indices are ignored.
    pub fn render_shadow<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>, cascade: usize) {
        let Some(pipeline_base) = cascade.checked_mul(2) else {
            return;
        };
        if pipeline_base + 1 >= self.shared.shadow_pipelines.len() {
            return;
        }

        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        for section in &self.gpu_model.sections {
            let material = &self.gpu_model.materials[section.material_index];
            if !material.casts_shadows {
                continue;
            }
            pass.set_pipeline(
                &self.shared.shadow_pipelines[pipeline_base + material.two_sided_pipeline_index],
            );
            pass.set_bind_group(1, &material.bind_group, &[]);
            pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
            pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..section.index_count, 0, 0..1);
        }
    }

    /// Returns the current model-to-world transform.
    #[must_use]
    pub fn model_transform(&self) -> Mat4 {
        self.model_transform
    }
}

impl WorldRenderer for Renderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        match phase {
            RenderPhase::Sky => self.render_sky(pass),
            RenderPhase::World => self.render(pass),
            RenderPhase::Distortion => self.render_distortion(pass),
            RenderPhase::Shadow { cascade } => self.render_shadow(pass, cascade),
        }
    }
}
