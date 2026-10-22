//! Shared constructors for common wgpu descriptors and RGBA texture uploads.
//!
//! Renderers keep their own shaders, vertex layouts, and pipeline state, while
//! using these helpers for the descriptor shapes that are identical across
//! terrain, foliage, particles, and model rendering.
//!
//! The RGBA helpers create sampled textures and write tightly packed RGBA8
//! pixels into them, with an optional CPU mip chain.

use crate::terrain::{generate_mipmaps, mip_level_count};
use crate::wgpu;

/// Creates a buffer bind-group-layout entry.
#[must_use]
pub fn buffer_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    buffer_type: wgpu::BufferBindingType,
) -> wgpu::BindGroupLayoutEntry {
    buffer_layout_entry_with_options(binding, visibility, buffer_type, false, None)
}

/// Creates a buffer bind-group-layout entry with dynamic-offset options.
#[must_use]
pub fn buffer_layout_entry_with_options(
    binding: u32,
    visibility: wgpu::ShaderStages,
    buffer_type: wgpu::BufferBindingType,
    has_dynamic_offset: bool,
    min_binding_size: Option<wgpu::BufferSize>,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: buffer_type,
            has_dynamic_offset,
            min_binding_size,
        },
        count: None,
    }
}

/// Creates a sampled-texture bind-group-layout entry.
#[must_use]
pub fn texture_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sample_type: wgpu::TextureSampleType,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

/// Creates a sampler bind-group-layout entry.
#[must_use]
pub fn sampler_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sampler_type: wgpu::SamplerBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(sampler_type),
        count: None,
    }
}

/// Creates a filtering-sampler bind-group-layout entry.
#[must_use]
pub fn filtering_sampler_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    sampler_layout_entry(binding, visibility, wgpu::SamplerBindingType::Filtering)
}

/// Creates a whole-buffer bind-group entry.
#[must_use]
pub fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

/// Creates a texture-view bind-group entry.
#[must_use]
pub fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

/// Creates a sampler bind-group entry.
#[must_use]
pub fn sampler_entry(binding: u32, sampler: &wgpu::Sampler) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Sampler(sampler),
    }
}

/// Creates a sampled 2D RGBA texture that the queue can write.
///
/// `size` is `[width, height]` in pixels. `layers` is the number of array layers.
/// `format` sets the color space, for example `Rgba8UnormSrgb` or `Rgba8Unorm`.
#[must_use]
pub fn create_rgba_texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    layers: u32,
    mip_level_count: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    let [width, height] = size;
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Writes tightly packed RGBA8 pixels into one mip level of one array layer.
///
/// `size` is the `[width, height]` of the mip level, in pixels.
pub fn write_rgba_level(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    pixels: &[u8],
    size: [u32; 2],
    layer: u32,
    mip_level: u32,
) {
    let [width, height] = size;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// Generates the CPU mip chain of an RGBA8 image and writes each level into one array layer.
///
/// `size` is the `[width, height]` of mip level 0. Each next level halves both dimensions,
/// with a minimum of 1 pixel.
pub fn upload_rgba_mip_chain(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    pixels: &[u8],
    size: [u32; 2],
    layer: u32,
) {
    let [mut mip_width, mut mip_height] = size;
    for (mip_level, mip_pixels) in (0_u32..).zip(generate_mipmaps(pixels, size[0], size[1])) {
        write_rgba_level(
            queue,
            texture,
            &mip_pixels,
            [mip_width, mip_height],
            layer,
            mip_level,
        );
        mip_width = (mip_width / 2).max(1);
        mip_height = (mip_height / 2).max(1);
    }
}

/// Creates a one-layer RGBA texture with a full mip chain made from `pixels`.
///
/// `size` is `[width, height]` in pixels. The texture has [`mip_level_count`] levels.
#[must_use]
pub fn create_mipped_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: [u32; 2],
    pixels: &[u8],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    let mip_levels = mip_level_count(size[0], size[1]);
    let texture = create_rgba_texture(device, label, size, 1, mip_levels, format);
    upload_rgba_mip_chain(queue, &texture, pixels, size, 0);
    texture
}
