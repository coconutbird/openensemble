//! Renderer-owned local spot and omnidirectional shadow maps.
//!
//! The retail renderer packs spot lights into four 256-pixel quadrants of an
//! eight-slice 512-pixel atlas. Omnidirectional lights reserve two complete
//! slices and render one dual-paraboloid hemisphere into each. Allocation and
//! projection live here so shadow state never leaks into simulation.

use std::collections::HashMap;
use std::mem::size_of;
use std::num::NonZeroU64;

use glam::{Mat4, Vec3, Vec4};

use crate::lighting::{LocalLight, LocalLightSet, LocalLightShape, LocalShadow};
use crate::wgpu;

/// Width and height of every local-shadow texture-array slice.
pub const LOCAL_SHADOW_MAP_SIZE: u32 = 512;
/// Number of slices in the retail local-shadow texture array.
pub const LOCAL_SHADOW_SLICE_COUNT: u32 = 8;
/// Sampled depth format corresponding to the retail D24S8 atlas.
pub const LOCAL_SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
/// Fixed depth bias closest to retail's `0.0001` in a 24-bit depth buffer.
pub const LOCAL_SHADOW_DEPTH_BIAS: i32 = 1_678;
/// Maximum number of caster passes submitted in one frame.
pub const MAX_LOCAL_SHADOW_PASSES: usize = 32;
/// Byte size of one caster-pass uniform record.
pub const LOCAL_SHADOW_PASS_UNIFORM_SIZE: u64 = size_of::<PassUniform>() as u64;

const LOCAL_SHADOW_MAP_DIM: u16 = 512;
const SPOT_PAGE_SIZE: u16 = LOCAL_SHADOW_MAP_DIM / 2;
const LOCAL_SHADOW_SLICES: u8 = 8;
const SHADOW_FADE_START_PIXELS: f32 = 100.0;
const SHADOW_FADE_END_PIXELS: f32 = 250.0;
const SPOT_NEAR: f32 = 1.0;
const SPOT_FAR: f32 = 128.0;

/// A selected direct light that requests an authored local shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalShadowRequest {
    /// Index in the selected [`LocalLightSet`].
    pub light_index: usize,
    /// Stable renderer identity used to retain atlas placement between frames.
    pub stable_id: u64,
    /// Projected influence radius in pixels.
    pub screen_radius: f32,
    /// Authored spot-right vector. Omnidirectional lights ignore this field.
    pub spot_right: [f32; 3],
}

/// One atlas pass produced by [`LocalShadowMap::update`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalShadowPass {
    layer: u32,
    viewport: [u16; 4],
}

impl LocalShadowPass {
    /// Texture-array layer receiving this pass.
    #[must_use]
    pub const fn layer(self) -> u32 {
        self.layer
    }

    /// Pixel viewport as `[x, y, width, height]`.
    #[must_use]
    pub const fn viewport(self) -> [u16; 4] {
        self.viewport
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AllocationKind {
    Spot,
    Omni,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Allocation {
    kind: AllocationKind,
    slice: u8,
    page: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct PassUniform {
    transform: [[f32; 4]; 4],
    params: [u32; 4],
}

struct PlannedPass {
    descriptor: LocalShadowPass,
    uniform: PassUniform,
}

struct ShadowPlan {
    passes: Vec<PlannedPass>,
    cache: HashMap<u64, Allocation>,
}

/// Persistent eight-slice local-shadow atlas and per-pass transforms.
pub struct LocalShadowMap {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    layer_views: Vec<wgpu::TextureView>,
    pass_layout: wgpu::BindGroupLayout,
    pass_buffer: wgpu::Buffer,
    pass_bind_group: wgpu::BindGroup,
    pass_binding_size: NonZeroU64,
    pass_stride: u32,
    passes: Vec<LocalShadowPass>,
    cache: HashMap<u64, Allocation>,
}

impl LocalShadowMap {
    /// Creates an empty depth atlas and fixed-size dynamic pass buffer.
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = create_texture(device);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Local Shadow Array View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::DepthOnly,
            ..Default::default()
        });
        let layer_views = (0..LOCAL_SHADOW_SLICE_COUNT)
            .map(|layer| layer_view(&texture, layer))
            .collect();
        let pass_layout = create_pass_layout(device);
        let pass_binding_size =
            NonZeroU64::new(LOCAL_SHADOW_PASS_UNIFORM_SIZE).unwrap_or(NonZeroU64::MIN);
        let pass_stride = aligned_pass_stride(device);
        let pass_buffer = create_pass_buffer(device, pass_stride);
        let pass_bind_group =
            create_pass_bind_group(device, &pass_layout, &pass_buffer, pass_binding_size);
        Self {
            _texture: texture,
            view,
            layer_views,
            pass_layout,
            pass_buffer,
            pass_bind_group,
            pass_binding_size,
            pass_stride,
            passes: Vec::new(),
            cache: HashMap::new(),
        }
    }

    /// Plans this frame, attaches receiver metadata, and uploads caster transforms.
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        lights: &mut LocalLightSet,
        requests: &[LocalShadowRequest],
    ) {
        lights.clear_shadows();
        let plan = build_plan(lights, requests, std::mem::take(&mut self.cache));
        self.cache = plan.cache;
        self.passes = plan
            .passes
            .iter()
            .map(|planned| planned.descriptor)
            .collect();
        lights.shadows_enabled = !self.passes.is_empty();
        upload_passes(queue, &self.pass_buffer, self.pass_stride, &plan.passes);
    }

    /// Clears every slice used by the current plan before caster draws begin.
    pub fn encode_clears(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut used = [false; LOCAL_SHADOW_SLICE_COUNT as usize];
        for pass in &self.passes {
            used[pass.layer as usize] = true;
        }
        for (layer, view) in self.layer_views.iter().enumerate() {
            if used[layer] {
                clear_layer(encoder, view);
            }
        }
    }

    /// Returns all caster passes in submission order.
    #[must_use]
    pub fn passes(&self) -> &[LocalShadowPass] {
        &self.passes
    }

    /// Returns the array view consumed by receiver shaders.
    #[must_use]
    pub const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Returns one renderable depth layer.
    #[must_use]
    pub fn layer_view(&self, layer: u32) -> &wgpu::TextureView {
        &self.layer_views[layer as usize]
    }

    /// Returns the terrain caster-pass layout.
    #[must_use]
    pub const fn pass_layout(&self) -> &wgpu::BindGroupLayout {
        &self.pass_layout
    }

    /// Returns the terrain caster-pass bind group.
    #[must_use]
    pub const fn pass_bind_group(&self) -> &wgpu::BindGroup {
        &self.pass_bind_group
    }

    /// Returns the shared pass buffer for UGX caster pipelines.
    #[must_use]
    pub const fn pass_buffer(&self) -> &wgpu::Buffer {
        &self.pass_buffer
    }

    /// Size of the bound dynamic-uniform record.
    #[must_use]
    pub const fn pass_binding_size(&self) -> NonZeroU64 {
        self.pass_binding_size
    }

    /// Byte stride between dynamic-uniform records.
    #[must_use]
    pub const fn pass_stride(&self) -> u32 {
        self.pass_stride
    }

    /// Dynamic offset for one pass.
    #[must_use]
    pub const fn pass_offset(&self, pass: u32) -> Option<u32> {
        self.pass_stride.checked_mul(pass)
    }
}

fn create_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Local Shadow Atlas"),
        size: wgpu::Extent3d {
            width: LOCAL_SHADOW_MAP_SIZE,
            height: LOCAL_SHADOW_MAP_SIZE,
            depth_or_array_layers: LOCAL_SHADOW_SLICE_COUNT,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: LOCAL_SHADOW_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn layer_view(texture: &wgpu::Texture, layer: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Local Shadow Layer View"),
        dimension: Some(wgpu::TextureViewDimension::D2),
        // Combined depth/stencil views are required for render attachments.
        // The separate array view remains depth-only for shader sampling.
        aspect: wgpu::TextureAspect::All,
        base_array_layer: layer,
        array_layer_count: Some(1),
        ..Default::default()
    })
}

fn create_pass_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Local Shadow Pass Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(LOCAL_SHADOW_PASS_UNIFORM_SIZE),
            },
            count: None,
        }],
    })
}

fn aligned_pass_stride(device: &wgpu::Device) -> u32 {
    let size = u32::try_from(size_of::<PassUniform>()).expect("pass uniform size must fit u32");
    let alignment = device.limits().min_uniform_buffer_offset_alignment.max(1);
    size.div_ceil(alignment) * alignment
}

fn create_pass_buffer(device: &wgpu::Device, stride: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Local Shadow Pass Uniforms"),
        size: u64::from(stride) * MAX_LOCAL_SHADOW_PASSES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_pass_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    size: NonZeroU64,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Local Shadow Pass Bind Group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: Some(size),
            }),
        }],
    })
}

fn clear_layer(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Clear Local Shadow Layer"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        occlusion_query_set: None,
        timestamp_writes: None,
    });
}

fn upload_passes(queue: &wgpu::Queue, buffer: &wgpu::Buffer, stride: u32, passes: &[PlannedPass]) {
    for (index, pass) in passes.iter().enumerate() {
        let offset = u64::from(stride) * index as u64;
        queue.write_buffer(buffer, offset, bytemuck::bytes_of(&pass.uniform));
    }
}

fn build_plan(
    lights: &mut LocalLightSet,
    requests: &[LocalShadowRequest],
    mut previous: HashMap<u64, Allocation>,
) -> ShadowPlan {
    let mut used_pages = [0_u8; LOCAL_SHADOW_SLICE_COUNT as usize];
    let mut retained = retain_allocations(lights, requests, &mut previous, &mut used_pages);
    let mut cache = HashMap::new();
    let mut passes = Vec::new();
    // Retail updates cached lights before newly visible lights. Besides making
    // pass order stable, this keeps the pass cap from evicting an existing
    // shadow merely because a new light sorts earlier in the visible list.
    for retained_only in [true, false] {
        for request in requests {
            if retained.contains_key(&request.stable_id) != retained_only
                || cache.contains_key(&request.stable_id)
            {
                continue;
            }
            let Some(light) = lights.light(request.light_index).copied() else {
                continue;
            };
            let Some(fade) = shadow_fade(request.screen_radius) else {
                continue;
            };
            let kind = allocation_kind(light);
            let pass_count = if kind == AllocationKind::Omni { 2 } else { 1 };
            if passes.len() + pass_count > MAX_LOCAL_SHADOW_PASSES {
                continue;
            }
            let allocation = retained
                .remove(&request.stable_id)
                .or_else(|| allocate(kind, &mut used_pages));
            let Some(allocation) = allocation else {
                continue;
            };
            let (shadow, mut light_passes) = light_plan(light, *request, allocation, fade);
            lights.set_shadow(request.light_index, Some(shadow));
            passes.append(&mut light_passes);
            cache.insert(request.stable_id, allocation);
        }
    }
    ShadowPlan { passes, cache }
}

fn retain_allocations(
    lights: &LocalLightSet,
    requests: &[LocalShadowRequest],
    previous: &mut HashMap<u64, Allocation>,
    used: &mut [u8; LOCAL_SHADOW_SLICE_COUNT as usize],
) -> HashMap<u64, Allocation> {
    let mut retained = HashMap::new();
    for request in requests {
        let Some(light) = lights.light(request.light_index).copied() else {
            continue;
        };
        if shadow_fade(request.screen_radius).is_none() || retained.contains_key(&request.stable_id)
        {
            continue;
        }
        let kind = allocation_kind(light);
        let Some(allocation) = previous.remove(&request.stable_id) else {
            continue;
        };
        if allocation.kind == kind && allocation_is_free(allocation, used) {
            reserve(allocation, used);
            retained.insert(request.stable_id, allocation);
        }
    }
    retained
}

fn allocation_kind(light: LocalLight) -> AllocationKind {
    match light.shape {
        LocalLightShape::Omni => AllocationKind::Omni,
        LocalLightShape::Spot { .. } => AllocationKind::Spot,
    }
}

fn allocation_is_free(allocation: Allocation, used: &[u8]) -> bool {
    let slice = usize::from(allocation.slice);
    match allocation.kind {
        AllocationKind::Spot => used[slice] & (1 << allocation.page) == 0,
        AllocationKind::Omni => {
            allocation.slice + 1 < LOCAL_SHADOW_SLICES && used[slice] == 0 && used[slice + 1] == 0
        }
    }
}

fn reserve(allocation: Allocation, used: &mut [u8; LOCAL_SHADOW_SLICE_COUNT as usize]) {
    let slice = usize::from(allocation.slice);
    match allocation.kind {
        AllocationKind::Spot => used[slice] |= 1 << allocation.page,
        AllocationKind::Omni => {
            used[slice] = 0xF;
            used[slice + 1] = 0xF;
        }
    }
}

fn allocate(
    kind: AllocationKind,
    used: &mut [u8; LOCAL_SHADOW_SLICE_COUNT as usize],
) -> Option<Allocation> {
    let allocation = match kind {
        AllocationKind::Spot => allocate_spot(used),
        AllocationKind::Omni => allocate_omni(used),
    }?;
    reserve(allocation, used);
    Some(allocation)
}

fn allocate_spot(used: &[u8]) -> Option<Allocation> {
    for slice in (0..LOCAL_SHADOW_SLICES).rev() {
        for page in 0..4_u8 {
            if used[usize::from(slice)] & (1 << page) == 0 {
                return Some(Allocation {
                    kind: AllocationKind::Spot,
                    slice,
                    page,
                });
            }
        }
    }
    None
}

fn allocate_omni(used: &[u8]) -> Option<Allocation> {
    for slice in 0..LOCAL_SHADOW_SLICES - 1 {
        if used[usize::from(slice)] == 0 && used[usize::from(slice) + 1] == 0 {
            return Some(Allocation {
                kind: AllocationKind::Omni,
                slice,
                page: 0,
            });
        }
    }
    None
}

fn shadow_fade(screen_radius: f32) -> Option<f32> {
    let byte = (255.0 * (screen_radius - SHADOW_FADE_START_PIXELS)
        / (SHADOW_FADE_END_PIXELS - SHADOW_FADE_START_PIXELS))
        .trunc()
        .clamp(0.0, 255.0);
    (byte >= 1.0).then_some(1.0 - byte / 255.0)
}

fn light_plan(
    light: LocalLight,
    request: LocalShadowRequest,
    allocation: Allocation,
    fade: f32,
) -> (LocalShadow, Vec<PlannedPass>) {
    match light.shape {
        LocalLightShape::Omni => omni_plan(light, allocation, fade),
        LocalLightShape::Spot {
            direction,
            outer_cos,
            ..
        } => spot_plan(
            light,
            request.spot_right,
            direction,
            outer_cos,
            allocation,
            fade,
        ),
    }
}

fn omni_plan(
    light: LocalLight,
    allocation: Allocation,
    fade: f32,
) -> (LocalShadow, Vec<PlannedPass>) {
    let position = Vec3::from_array(light.position);
    let view = world_to_view(position, Vec3::X, -Vec3::Y);
    let texture_index = -((f32::from(allocation.slice) + 0.5) / 8.0) - 1.0;
    let shadow = LocalShadow {
        texture_index,
        transform_rows: [
            matrix_row(view, 0),
            matrix_row(view, 1),
            matrix_row(view, 2),
        ],
        bounds_preset: 0,
        fade,
    };
    let viewport = [0, 0, LOCAL_SHADOW_MAP_DIM, LOCAL_SHADOW_MAP_DIM];
    let back_view = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)) * view;
    let passes = vec![
        planned_pass(allocation.slice, viewport, view, true),
        planned_pass(allocation.slice + 1, viewport, back_view, true),
    ];
    (shadow, passes)
}

fn spot_plan(
    light: LocalLight,
    authored_right: [f32; 3],
    direction: [f32; 3],
    outer_cos: f32,
    allocation: Allocation,
    fade: f32,
) -> (LocalShadow, Vec<PlannedPass>) {
    let at = Vec3::from_array(direction).normalize_or_zero();
    let right = spot_right(at, Vec3::from_array(authored_right));
    let view = world_to_view(Vec3::from_array(light.position), right, at);
    let outer_angle = (outer_cos.clamp(-1.0, 1.0).acos() * 2.0)
        .clamp(1.0_f32.to_radians(), 179.0_f32.to_radians());
    let projection = Mat4::perspective_lh(outer_angle, 1.0, SPOT_NEAR, SPOT_FAR);
    let world_to_clip = projection * view;
    let page = u16::from(allocation.page);
    let u = (page & 1) * SPOT_PAGE_SIZE;
    let v = ((page >> 1) & 1) * SPOT_PAGE_SIZE;
    let viewport = [u, v, SPOT_PAGE_SIZE, SPOT_PAGE_SIZE];
    let shadow = spot_shadow(world_to_clip, allocation, viewport, fade);
    let passes = vec![planned_pass(
        allocation.slice,
        viewport,
        world_to_clip,
        false,
    )];
    (shadow, passes)
}

fn spot_shadow(
    world_to_clip: Mat4,
    allocation: Allocation,
    viewport: [u16; 4],
    fade: f32,
) -> LocalShadow {
    let [u, v, size, _] = viewport;
    let size = f32::from(size);
    let u = f32::from(u);
    let v = f32::from(v);
    let scale = size * 0.5 / 512.0;
    let offset_u = (size * 0.5 + u + 0.5) / 512.0;
    let offset_v = (size * 0.5 + v + 0.5) / 512.0;
    let clip_x = Vec4::from_array(matrix_row(world_to_clip, 0));
    let clip_y = Vec4::from_array(matrix_row(world_to_clip, 1));
    let clip_w = Vec4::from_array(matrix_row(world_to_clip, 3));
    LocalShadow {
        texture_index: (f32::from(allocation.slice) + 0.5) / 8.0,
        transform_rows: [
            (clip_x * scale + clip_w * offset_u).to_array(),
            (clip_y * -scale + clip_w * offset_v).to_array(),
            clip_w.to_array(),
        ],
        bounds_preset: u32::from(allocation.page) + 1,
        fade,
    }
}

fn planned_pass(
    layer: u8,
    viewport: [u16; 4],
    transform: Mat4,
    dual_paraboloid: bool,
) -> PlannedPass {
    PlannedPass {
        descriptor: LocalShadowPass {
            layer: u32::from(layer),
            viewport,
        },
        uniform: PassUniform {
            transform: transform.to_cols_array_2d(),
            params: [u32::from(dual_paraboloid), 0, 0, 0],
        },
    }
}

fn world_to_view(position: Vec3, right: Vec3, at: Vec3) -> Mat4 {
    let right = right.normalize_or_zero();
    let at = at.normalize_or_zero();
    let up = at.cross(right).normalize_or_zero();
    Mat4::from_cols(
        Vec4::new(right.x, up.x, at.x, 0.0),
        Vec4::new(right.y, up.y, at.y, 0.0),
        Vec4::new(right.z, up.z, at.z, 0.0),
        Vec4::new(
            -position.dot(right),
            -position.dot(up),
            -position.dot(at),
            1.0,
        ),
    )
}

fn spot_right(at: Vec3, authored: Vec3) -> Vec3 {
    let projected = authored - at * authored.dot(at);
    if projected.is_finite() && projected.length_squared() > f32::EPSILON {
        return projected.normalize();
    }
    let reference = if at.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
    reference.cross(at).normalize_or_zero()
}

fn matrix_row(matrix: Mat4, row: usize) -> [f32; 4] {
    [
        matrix.x_axis[row],
        matrix.y_axis[row],
        matrix.z_axis[row],
        matrix.w_axis[row],
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{AllocationKind, LOCAL_SHADOW_SLICE_COUNT, LocalShadowRequest, build_plan};
    use crate::lighting::{LocalLight, LocalLightSet};

    fn request(light_index: usize, stable_id: u64) -> LocalShadowRequest {
        LocalShadowRequest {
            light_index,
            stable_id,
            screen_radius: 250.0,
            spot_right: [1.0, 0.0, 0.0],
        }
    }

    #[test]
    fn spots_pack_from_last_slice_and_use_retail_quadrant_indices() {
        let mut lights = LocalLightSet::new(vec![
            LocalLight::spot([0.0; 3], [1.0; 3], 10.0, [0.0, 0.0, 1.0], 0.9, 0.8),
            LocalLight::spot([0.0; 3], [1.0; 3], 10.0, [0.0, 0.0, 1.0], 0.9, 0.8),
        ])
        .unwrap();
        let plan = build_plan(&mut lights, &[request(0, 1), request(1, 2)], HashMap::new());

        assert_eq!(
            plan.passes[0].descriptor.layer,
            LOCAL_SHADOW_SLICE_COUNT - 1
        );
        assert_eq!(plan.passes[0].descriptor.viewport, [0, 0, 256, 256]);
        assert_eq!(plan.passes[1].descriptor.viewport, [256, 0, 256, 256]);
        assert_eq!(lights.lights()[0].shadow.unwrap().bounds_preset, 1);
        assert_eq!(lights.lights()[1].shadow.unwrap().bounds_preset, 2);
    }

    #[test]
    fn omni_uses_two_low_slices_and_normalized_slice_centers() {
        let mut lights =
            LocalLightSet::new(vec![LocalLight::omni([4.0, 5.0, 6.0], [1.0; 3], 20.0)]).unwrap();
        let plan = build_plan(&mut lights, &[request(0, 7)], HashMap::new());
        let shadow = lights.lights()[0].shadow.unwrap();

        assert_eq!(plan.cache[&7].kind, AllocationKind::Omni);
        assert_eq!(plan.passes.len(), 2);
        assert_eq!(plan.passes[0].descriptor.layer, 0);
        assert_eq!(plan.passes[1].descriptor.layer, 1);
        assert_eq!(shadow.texture_index.to_bits(), (-1.0625_f32).to_bits());
    }

    #[test]
    fn new_lights_cannot_steal_a_retained_spot_page() {
        let spot = || LocalLight::spot([0.0; 3], [1.0; 3], 10.0, [0.0, 0.0, 1.0], 0.9, 0.8);
        let mut first_lights = LocalLightSet::new(vec![spot()]).unwrap();
        let first = build_plan(&mut first_lights, &[request(0, 1)], HashMap::new());
        let mut next_lights = LocalLightSet::new(vec![spot(), spot()]).unwrap();
        let next = build_plan(
            &mut next_lights,
            &[request(0, 2), request(1, 1)],
            first.cache,
        );

        assert_eq!(next.passes[0].descriptor.viewport, [0, 0, 256, 256]);
        assert_eq!(next.passes[1].descriptor.viewport, [256, 0, 256, 256]);
    }

    #[test]
    fn retail_shadow_fade_discards_subpixel_byte_results() {
        let mut lights =
            LocalLightSet::new(vec![LocalLight::omni([0.0; 3], [1.0; 3], 10.0)]).unwrap();
        let mut tiny = request(0, 1);
        tiny.screen_radius = 100.1;
        let plan = build_plan(&mut lights, &[tiny], HashMap::new());

        assert!(plan.passes.is_empty());
        assert!(lights.lights()[0].shadow.is_none());
    }
}
