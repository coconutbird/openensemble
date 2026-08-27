//! Headless validation for renderer pipelines that are not always instantiated
//! by the terrain-viewer scenario under test.

use glam::{Mat4, Vec3};
use render::particle::{
    ParticleBlendMode, ParticleInstance, ParticleMaterial, ParticleRenderer, ParticleScene,
    ParticleSceneTextures,
};
use render::postprocess::{DISTORTION_FORMAT, HDR_COLOR_FORMAT};
use render::terrain::{
    LightingParams, TerrainPatchImage, TerrainPatchInstance, TerrainPatchMaterial,
    TerrainPatchRenderer, TerrainPatchRendererDescriptor, TerrainPatchWorldBindings,
};
use render::wgpu;

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
    let mut color_renderer = ParticleRenderer::new(
        &device,
        &queue,
        HDR_COLOR_FORMAT,
        &ParticleMaterial::default(),
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
    color_renderer.update_instances(&device, &queue, &particles)?;
    distortion_renderer.update_scene(&queue, &scene);
    distortion_renderer.update_instances(&device, &queue, &particles)?;

    let patch_renderer = create_patch_renderer(&device, &queue)?;

    let command_buffer = encode_renderer_passes(
        &device,
        &color_view,
        &distortion_view,
        &depth_view,
        &color_renderer,
        &distortion_renderer,
        &patch_renderer,
    );
    queue.submit(Some(command_buffer));
    let _poll_result = device.poll(wgpu::Maintain::Wait);
    if let Some(error) = device.pop_error_scope().await {
        anyhow::bail!("renderer GPU validation failed: {error}");
    }
    println!("renderer GPU smoke passed on {}", adapter.get_info().name);
    Ok(())
}

fn create_patch_renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
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
            world: TerrainPatchWorldBindings::default(),
        },
    );
    let view = Mat4::look_at_rh(Vec3::new(0.0, 3.0, 0.0), Vec3::ZERO, Vec3::Z);
    let projection = Mat4::orthographic_rh(-2.0, 2.0, -2.0, 2.0, 0.1, 10.0);
    let lighting = LightingParams {
        world_camera_pos: [0.0, 3.0, 0.0, 0.0],
        ..LightingParams::default()
    };
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
    color_view: &wgpu::TextureView,
    distortion_view: &wgpu::TextureView,
    depth_view: &wgpu::TextureView,
    color_renderer: &ParticleRenderer,
    distortion_renderer: &ParticleRenderer,
    patch_renderer: &TerrainPatchRenderer,
) -> wgpu::CommandBuffer {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Renderer GPU Smoke Encoder"),
    });
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
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}
