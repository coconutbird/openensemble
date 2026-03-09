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
    // Position decoding: mid (posCompMin) x, y, z, pad
    mid: vec4<f32>,
    // Position decoding: range (posCompRange) x, y, z, pad
    range: vec4<f32>,
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

// Decode packed R10G10B10A2 position (same as terrain_gpu.wesl)
// Formula matches game bytecode: offset = texel * posCompRange - posCompMin
fn unpack_position(packed: u32, mid: vec3<f32>, range: vec3<f32>) -> vec3<f32> {
    let x_bits = packed & 0x3FFu;
    let y_bits = (packed >> 10u) & 0x3FFu;
    let z_bits = (packed >> 20u) & 0x3FFu;

    let norm = vec3<f32>(
        f32(x_bits) / 1023.0,
        f32(y_bits) / 1023.0,
        f32(z_bits) / 1023.0
    );

    return norm * range - mid;
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

    let local_pos = unpack_position(packed, shadow_params.mid.xyz, shadow_params.range.xyz);

    // Reconstruct world position: grid * tileScale + displacement (same as terrain_gpu.wesl)
    let global_u = clamp(round(u * f32(num_verts - 1u)) / f32(num_verts - 1u), 0.0, 1.0);
    let global_v = clamp(round(v * f32(num_verts - 1u)) / f32(num_verts - 1u), 0.0, 1.0);
    let grid_x = global_u * f32(num_verts - 1u);
    let grid_z = global_v * f32(num_verts - 1u);
    let pos = vec3<f32>(
        grid_x * tile_scale + local_pos.x,
        local_pos.y,
        grid_z * tile_scale + local_pos.z,
    );

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
