//! Headless validation for renderer pipelines that are not always instantiated
//! by the terrain-viewer scenario under test.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use pipeline::database::hw1::pfx;
use pipeline::xmb::Document;
use render::hp_bars::{ColorStage, HpBarColorStages, HpBarInstance, HpBarRenderer, HpBarStyle};
use render::lighting::{LocalLight, LocalLightBuffer, LocalLightSet};
use render::local_shadow::{
    LOCAL_SHADOW_DEPTH_BIAS, LOCAL_SHADOW_FORMAT, LocalShadowMap, LocalShadowRequest,
};
use render::particle::{
    ParticleBlendMode, ParticleEffectRuntime, ParticleInstance, ParticleMaterial,
    ParticleRenderContext, ParticleRenderer, ParticleScene, ParticleSceneTextures,
};
use render::postprocess::{DISTORTION_FORMAT, HDR_COLOR_FORMAT};
use render::selection_decals::{SelectionDecal, SelectionDecalRenderer};
use render::terrain::{
    LightingParams, TerrainPatchImage, TerrainPatchInstance, TerrainPatchMaterial,
    TerrainPatchRenderer, TerrainPatchRendererDescriptor, TerrainPatchWorldBindings,
};
use render::wgpu;
use render::world_view::WorldCamera;

fn main() -> anyhow::Result<()> {
    pollster::block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .ok_or_else(|| anyhow::anyhow!("no headless GPU adapter is available"))?;
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Renderer GPU Smoke Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
            None,
        )
        .await?;

    let [color, distortion, depth] = [
        create_target(&device, "Smoke HDR", HDR_COLOR_FORMAT),
        create_target(&device, "Smoke Distortion", DISTORTION_FORMAT),
        create_target(&device, "Smoke Depth", wgpu::TextureFormat::Depth32Float),
    ];
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let distortion_view = distortion.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let scene_textures = ParticleSceneTextures {
        depth: &depth_view,
        light_volume: None,
    };

    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let color_material = ParticleMaterial::default();
    let mut color_renderer = ParticleRenderer::new(
        &device,
        &queue,
        HDR_COLOR_FORMAT,
        &color_material,
        scene_textures,
    )?;
    let distortion_material = ParticleMaterial {
        blend: ParticleBlendMode::Distortion,
        ..ParticleMaterial::default()
    };
    let mut distortion_renderer = ParticleRenderer::new(
        &device,
        &queue,
        HDR_COLOR_FORMAT,
        &distortion_material,
        scene_textures,
    )?;
    let scene = ParticleScene::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        [0.0, 0.0, 2.0],
        [8, 8],
        [0.0, 1.0],
    );
    let particles = [ParticleInstance::billboard(
        [0.0, 0.0, 0.0],
        [0.5, 0.5],
        [1.0; 4],
    )];
    color_renderer.update_scene(&queue, &scene);
    update_runtime_particles(&device, &queue, &mut color_renderer, &color_material)?;
    distortion_renderer.update_scene(&queue, &scene);
    distortion_renderer.update_instances(&device, &queue, &particles)?;

    let (local_shadows, local_lights, local_light_buffer) =
        create_local_shadow_smoke(&device, &queue)?;
    let local_shadow_caster = SmokeLocalShadowCaster::new(&device, local_shadows.pass_layout());
    let patch_renderer = create_patch_renderer(
        &device,
        &queue,
        &local_shadows,
        &local_lights,
        &local_light_buffer,
    )?;

    let command_buffer = encode_renderer_passes(
        &device,
        [&color_view, &distortion_view, &depth_view],
        &color_renderer,
        &distortion_renderer,
        &patch_renderer,
        &local_shadows,
        &local_shadow_caster,
    );
    queue.submit(Some(command_buffer));
    let _poll_result = device.poll(wgpu::Maintain::Wait);
    run_overlay_smoke(&device, &queue)?;
    if let Some(error) = device.pop_error_scope().await {
        anyhow::bail!("renderer GPU validation failed: {error}");
    }
    println!("renderer GPU smoke passed on {}", adapter.get_info().name);
    Ok(())
}

fn create_local_shadow_smoke(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> anyhow::Result<(LocalShadowMap, LocalLightSet, LocalLightBuffer)> {
    let mut lights = LocalLightSet::new(vec![
        LocalLight::spot([0.0, 3.0, 0.0], [1.0; 3], 10.0, [0.0, -1.0, 0.0], 0.9, 0.8),
        LocalLight::omni([1.0, 2.0, 3.0], [0.5; 3], 8.0),
    ])?;
    let requests = [
        LocalShadowRequest {
            light_index: 0,
            stable_id: 1,
            screen_radius: 250.0,
            spot_right: [1.0, 0.0, 0.0],
        },
        LocalShadowRequest {
            light_index: 1,
            stable_id: 2,
            screen_radius: 250.0,
            spot_right: [1.0, 0.0, 0.0],
        },
    ];
    let mut shadows = LocalShadowMap::new(device);
    shadows.update(queue, &mut lights, &requests);
    anyhow::ensure!(
        shadows.passes().len() == 3 && lights.lights().iter().all(|light| light.shadow.is_some()),
        "local-shadow smoke plan did not produce one spot and two omni passes"
    );
    let buffer = LocalLightBuffer::new(device, &lights);
    Ok((shadows, lights, buffer))
}

struct SmokeLocalShadowCaster {
    pipeline: wgpu::RenderPipeline,
}

impl SmokeLocalShadowCaster {
    fn new(device: &wgpu::Device, pass_layout: &wgpu::BindGroupLayout) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Local Shadow Smoke Caster Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r"
struct LocalShadowPass {
    transform: mat4x4<f32>,
    params: vec4<u32>,
};

@group(0) @binding(0) var<uniform> shadow_pass: LocalShadowPass;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>(0.5, -0.5),
        vec2<f32>(0.0, 0.5),
    );
    let scale = select(0.25, 0.2, shadow_pass.params.x != 0u);
    return vec4<f32>(positions[vertex_index] * scale, 0.5, 1.0);
}
"
                .into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Local Shadow Smoke Caster Layout"),
            bind_group_layouts: &[pass_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Local Shadow Smoke Caster Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: LOCAL_SHADOW_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: LOCAL_SHADOW_DEPTH_BIAS,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Self { pipeline }
    }
}

fn update_runtime_particles(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut ParticleRenderer,
    material: &ParticleMaterial,
) -> anyhow::Result<()> {
    let mut runtime = create_particle_runtime()?;
    runtime.update(0.25, Mat4::IDENTITY);
    let emitter = runtime
        .emitters()
        .first()
        .ok_or_else(|| anyhow::anyhow!("particle runtime smoke effect has no emitter"))?;
    renderer.update_emitter_runtime(
        device,
        queue,
        emitter,
        material,
        ParticleRenderContext::default(),
    )?;
    Ok(())
}

fn create_particle_runtime() -> anyhow::Result<ParticleEffectRuntime> {
    let document = Document::from_xml(
        r#"<ParticleEffect Name="gpu-smoke">
            <ParticleEmitter Name="runtime">
                <EmitterData>
                    <ParticleType>eBillBoard</ParticleType>
                    <MaxParticles>8</MaxParticles>
                    <EmissionRate>10</EmissionRate>
                    <EmissionTime>1</EmissionTime>
                    <ParticleLife>2</ParticleLife>
                </EmitterData>
            </ParticleEmitter>
        </ParticleEffect>"#,
    )?;
    let effect = pfx::parse(&document)?;
    Ok(ParticleEffectRuntime::new(&effect, 7, Mat4::IDENTITY))
}

fn create_patch_renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    local_shadows: &LocalShadowMap,
    local_lights: &LocalLightSet,
    local_light_buffer: &LocalLightBuffer,
) -> anyhow::Result<TerrainPatchRenderer> {
    let material = TerrainPatchMaterial::from_images(
        "gpu-smoke",
        TerrainPatchImage::solid([255; 4]),
        TerrainPatchImage::solid([128, 128, 255, 255]),
        TerrainPatchImage::solid([255; 4]),
    );
    let mut renderer = TerrainPatchRenderer::new(
        device,
        queue,
        TerrainPatchRendererDescriptor {
            color_format: HDR_COLOR_FORMAT,
            depth_format: Some(wgpu::TextureFormat::Depth32Float),
            material: &material,
            world: TerrainPatchWorldBindings {
                local_lights: Some(local_light_buffer),
                local_shadow: Some(local_shadows.view()),
                ..TerrainPatchWorldBindings::default()
            },
        },
    );
    let view = Mat4::look_at_rh(Vec3::new(0.0, 3.0, 0.0), Vec3::ZERO, Vec3::Z);
    let projection = Mat4::orthographic_rh(-2.0, 2.0, -2.0, 2.0, 0.1, 10.0);
    let mut lighting = LightingParams {
        world_camera_pos: [0.0, 3.0, 0.0, 0.0],
        ..LightingParams::default()
    };
    local_lights.apply_to_lighting(&mut lighting);
    renderer.update_frame(queue, projection * view, &lighting);
    renderer.update_instances(
        device,
        queue,
        &[TerrainPatchInstance::axis_aligned(
            [0.0, 0.0, 0.0],
            [0.75, 0.75],
        )],
    )?;
    Ok(renderer)
}

fn encode_renderer_passes(
    device: &wgpu::Device,
    views: [&wgpu::TextureView; 3],
    color_renderer: &ParticleRenderer,
    distortion_renderer: &ParticleRenderer,
    patch_renderer: &TerrainPatchRenderer,
    local_shadows: &LocalShadowMap,
    local_shadow_caster: &SmokeLocalShadowCaster,
) -> wgpu::CommandBuffer {
    let [color_view, distortion_view, depth_view] = views;
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Renderer GPU Smoke Encoder"),
    });
    encode_local_shadow_smoke(&mut encoder, local_shadows, local_shadow_caster);
    {
        let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Particle Smoke Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Particle Color Smoke Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        color_renderer.render_color(&mut pass);
        patch_renderer.render(&mut pass);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Particle Distortion Smoke Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: distortion_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        distortion_renderer.render_distortion(&mut pass);
    }
    encoder.finish()
}

fn encode_local_shadow_smoke(
    encoder: &mut wgpu::CommandEncoder,
    local_shadows: &LocalShadowMap,
    caster: &SmokeLocalShadowCaster,
) {
    local_shadows.encode_clears(encoder);
    for (pass_index, descriptor) in local_shadows.passes().iter().copied().enumerate() {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Local Shadow Smoke Pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: local_shadows.layer_view(descriptor.layer()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let [x, y, width, height] = descriptor.viewport();
        pass.set_viewport(
            f32::from(x),
            f32::from(y),
            f32::from(width),
            f32::from(height),
            0.0,
            1.0,
        );
        pass.set_scissor_rect(
            u32::from(x),
            u32::from(y),
            u32::from(width),
            u32::from(height),
        );
        let pass_index = u32::try_from(pass_index).expect("smoke pass index must fit u32");
        let offset = local_shadows
            .pass_offset(pass_index)
            .expect("smoke pass offset must fit u32");
        pass.set_pipeline(&caster.pipeline);
        pass.set_bind_group(0, local_shadows.pass_bind_group(), &[offset]);
        pass.draw(0..3, 0..1);
    }
}

/// Draws a selection decal into an HDR target and a hit-point bar into an sRGB target, then
/// reads the bar pixels back.
fn run_overlay_smoke(device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<()> {
    let [scene, depth, output] = [
        create_target(device, "Overlay Smoke HDR", HDR_COLOR_FORMAT),
        create_target(
            device,
            "Overlay Smoke Depth",
            wgpu::TextureFormat::Depth32Float,
        ),
        create_target(
            device,
            "Overlay Smoke Output",
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ),
    ];
    let scene_view = scene.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let world_to_view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    let camera = WorldCamera {
        view_projection: Mat4::orthographic_rh(-2.0, 2.0, -2.0, 2.0, 0.1, 10.0) * world_to_view,
        world_to_view,
        position: Vec3::new(0.0, 0.0, 5.0),
        depth_unproject: [0.0, 1.0],
        viewport: [8, 8],
    };

    let mut decals = SelectionDecalRenderer::new(
        device,
        queue,
        HDR_COLOR_FORMAT,
        Some(wgpu::TextureFormat::Depth32Float),
        TerrainPatchWorldBindings::default(),
    );
    let top_down = Mat4::orthographic_rh(-2.0, 2.0, -2.0, 2.0, 0.1, 10.0)
        * Mat4::look_at_rh(Vec3::new(0.0, 3.0, 0.0), Vec3::ZERO, Vec3::Z);
    decals.update(
        device,
        queue,
        top_down,
        &LightingParams::default(),
        &[SelectionDecal {
            center: Vec3::ZERO,
            forward: Vec3::Z,
            radius_x: 1.5,
            radius_z: 1.0,
            color: [0.27, 0.43, 0.09, 1.0],
            intensity: 6.0,
        }],
    )?;

    let style = HpBarStyle {
        name: "Smoke_Health".to_owned(),
        texture: r"ui\smoke\frame".to_owned(),
        mask_texture: r"ui\smoke\frame_mask".to_owned(),
        size: [4.0, 4.0],
        hitpoint_uv: Some([0.0, 0.25, 1.0, 0.75]),
        shield_uv: None,
        ammo_uv: None,
    };
    let stages = HpBarColorStages {
        hitpoint: vec![ColorStage {
            value: 1.0,
            intensity: 1.0,
            color: [0, 255, 0],
        }],
        ..HpBarColorStages::default()
    };
    let images = HashMap::from([
        (style.texture.clone(), TerrainPatchImage::solid([255; 4])),
        (
            style.mask_texture.clone(),
            TerrainPatchImage::solid([255; 4]),
        ),
    ]);
    let mut bars = HpBarRenderer::new(
        device,
        queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        std::slice::from_ref(&style),
        &stages,
        &images,
    );
    let style_id = bars
        .style("smoke_health")
        .ok_or_else(|| anyhow::anyhow!("the smoke HP bar style did not load"))?;
    bars.update(
        device,
        queue,
        &camera,
        &[HpBarInstance {
            position: Vec3::ZERO,
            size: style.size,
            style: style_id,
            hitpoints: 0.5,
            shield: None,
            ammo: None,
            player_color: [255, 0, 0],
        }],
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Overlay Smoke Encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Selection Decal Smoke Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &scene_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        decals.render(&mut pass);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("HP Bar Smoke Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        bars.render(&mut pass);
    }
    let pixels = read_target(device, queue, encoder, &output)?;
    let pixel = |x: usize, y: usize| &pixels[y * READBACK_ROW_BYTES + x * 4..][..4];
    // Filled half of the hit-point rectangle, empty half, and the frame above the rectangle.
    let filled = pixel(2, 4);
    let empty = pixel(6, 4);
    let frame = pixel(4, 1);
    let checks = filled[1] > 200
        && filled[0] < 30
        && empty[1] < 60
        && empty[0] < 60
        && frame[0] > 200
        && frame[1] < 30;
    anyhow::ensure!(
        checks,
        "HP bar pixels are wrong: filled {filled:?}, empty {empty:?}, frame {frame:?}"
    );
    Ok(())
}

/// Bytes in one row of a readback buffer: the copy alignment of wgpu.
const READBACK_ROW_BYTES: usize = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;

/// Copies an 8x8 RGBA8 target into a buffer, submits `encoder` and returns the padded rows.
fn read_target(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    target: &wgpu::Texture,
) -> anyhow::Result<Vec<u8>> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Overlay Smoke Readback"),
        size: u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * 8,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
                rows_per_image: Some(8),
            },
        },
        target.size(),
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _send_result = sender.send(result);
    });
    let _poll_result = device.poll(wgpu::Maintain::Wait);
    receiver.recv()??;
    let pixels = slice.get_mapped_range().to_vec();
    buffer.unmap();
    Ok(pixels)
}

fn create_target(device: &wgpu::Device, label: &str, format: wgpu::TextureFormat) -> wgpu::Texture {
    let is_depth = format == wgpu::TextureFormat::Depth32Float;
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if is_depth {
                wgpu::TextureUsages::TEXTURE_BINDING
            } else {
                wgpu::TextureUsages::COPY_SRC
            },
        view_formats: &[],
    })
}
