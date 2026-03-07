// Shadow depth pass - renders terrain from the light's perspective.
//
// This is a minimal vertex shader that transforms terrain vertices
// using the light's view-projection matrix. The fragment shader is
// a no-op since we only need the depth buffer output.
//
// Uses the same vertex input format as gpu_tess.wesl so we can
// reuse the same vertex/instance buffers.

struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: Camera;

struct ShadowParams {
    // terrain_info: num_verts_per_axis, tile_scale, num_patches_x, num_patches_z
    terrain_info: vec4<f32>,
    // world_min: x, y, z, pad
    world_min: vec4<f32>,
    // world_range: x, y, z, pad
    world_range: vec4<f32>,
};
@group(1) @binding(0)
var<uniform> shadow_params: ShadowParams;

// Terrain position data (packed R10G10B10A2)
@group(1) @binding(1)
var t_positions: texture_2d<u32>;

struct VertexInput {
    // Per-vertex: local UV within patch [0, 1]
    @location(0) local_uv: vec2<f32>,
    // Per-instance: patch index (x + z * num_patches_x)
    @location(1) patch_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

// Decode packed R10G10B10A2 position (same as gpu_tess.wesl)
fn unpack_position(packed: u32, mid: vec3<f32>, range: vec3<f32>) -> vec3<f32> {
    let x_bits = packed & 0x3FFu;
    let y_bits = (packed >> 10u) & 0x3FFu;
    let z_bits = (packed >> 20u) & 0x3FFu;

    let x_norm = f32(x_bits) / 1023.0;
    let y_norm = f32(y_bits) / 1023.0;
    let z_norm = f32(z_bits) / 1023.0;

    return mid + (vec3<f32>(x_norm, y_norm, z_norm) - 0.5) * range;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let num_verts = u32(shadow_params.terrain_info.x);
    let tile_scale = shadow_params.terrain_info.y;
    let num_patches_x = u32(shadow_params.terrain_info.z);
    let num_patches_z = u32(shadow_params.terrain_info.w);

    let patch_x = in.patch_index % num_patches_x;
    let patch_z = in.patch_index / num_patches_x;

    let raw_u = (f32(patch_x) + in.local_uv.x) / f32(num_patches_x);
    let raw_v = (f32(patch_z) + in.local_uv.y) / f32(num_patches_z);

    // Clamp to valid range
    let u = clamp(raw_u, 0.0, 1.0);
    let v = clamp(raw_v, 0.0, 1.0);

    // Sample position from texture
    let tex_x = u32(u * f32(num_verts - 1u));
    let tex_y = u32(v * f32(num_verts - 1u));
    let packed = textureLoad(t_positions, vec2<u32>(tex_x, tex_y), 0).r;

    let mid = shadow_params.world_min.xyz + shadow_params.world_range.xyz * 0.5;
    let pos = unpack_position(packed, mid, shadow_params.world_range.xyz);

    // Transform to light clip space
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Output depth as color (R channel) for VSM sampling
    let depth = in.clip_position.z;
    return vec4<f32>(depth, depth * depth, 0.0, 1.0);
}
