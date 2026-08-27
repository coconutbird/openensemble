use super::{PackedPatchInstance, TerrainPatchImage};
use crate::wgpu;
use std::mem;
use wgpu::util::DeviceExt;

pub(super) fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    let byte_size = capacity
        .checked_mul(mem::size_of::<PackedPatchInstance>())
        .and_then(|size| u64::try_from(size).ok())
        .unwrap_or(u64::MAX);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Terrain Patch Instance Buffer"),
        size: byte_size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(super) fn create_image_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    image: &TerrainPatchImage,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &image.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width * 4),
            rows_per_image: Some(image.height),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn create_2d_fallback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    color: [u8; 4],
) -> wgpu::TextureView {
    create_image_view(device, queue, label, &TerrainPatchImage::solid(color))
}

pub(super) fn create_array_fallback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    layers: u32,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let byte_count = usize::try_from(layers).unwrap_or(1).saturating_mul(4);
    queue.write_texture(
        texture.as_image_copy(),
        &vec![255; byte_count],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

pub(super) fn create_volume_fallback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    color: [u8; 4],
) -> wgpu::TextureView {
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
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
        &color,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn uniform_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    crate::gpu::buffer_layout_entry(binding, visibility, wgpu::BufferBindingType::Uniform)
}

pub(super) fn storage_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    crate::gpu::buffer_layout_entry(
        binding,
        visibility,
        wgpu::BufferBindingType::Storage { read_only: true },
    )
}

pub(super) fn texture_entry_layout(
    binding: u32,
    view_dimension: wgpu::TextureViewDimension,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    crate::gpu::texture_layout_entry(
        binding,
        visibility,
        wgpu::TextureSampleType::Float { filterable: true },
        view_dimension,
    )
}

pub(super) fn sampler_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    crate::gpu::filtering_sampler_layout_entry(binding, visibility)
}

pub(super) fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    crate::gpu::buffer_entry(binding, buffer)
}

pub(super) fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    crate::gpu::texture_entry(binding, view)
}
