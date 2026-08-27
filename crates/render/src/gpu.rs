//! Shared constructors for common wgpu binding descriptors.
//!
//! Renderers keep their own shaders, vertex layouts, and pipeline state, while
//! using these helpers for the descriptor shapes that are identical across
//! terrain, foliage, particles, and model rendering.

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
