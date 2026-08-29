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
    // Position decoding: xyz = posCompMin, w = normalized Y offset
    mid: vec4<f32>,
    // Position decoding: range (posCompRange) x, y, z, pad
    range: vec4<f32>,
};
@group(1) @binding(0)
var<uniform> shadow_params: ShadowParams;

// Terrain position data (filterable R10G10B10A2_UNORM)
@group(1) @binding(1)
var t_positions: texture_2d<f32>;

@group(1) @binding(2)
var t_alpha: texture_2d<f32>;

@group(1) @binding(3)
var s_alpha: sampler;

@group(1) @binding(4)
var t_dynamic_alpha: texture_2d<u32>;

@group(1) @binding(5)
var s_position: sampler;

struct VertexInput {
    // Per-vertex: local UV within patch [0, 1]
    @location(0) local_uv: vec2<f32>,
    // Per-instance: patch index (x + z * num_patches_x)
    @location(1) patch_index: u32,
    @location(2) edge_factors: vec4<u32>,
    @location(3) inside_factors: vec2<u32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) alpha_uv: vec2<f32>,
    @location(1) static_alpha: f32,
};

fn dynamic_alpha_enabled(grid_coord: vec2<u32>) -> bool {
    let dimensions = textureDimensions(t_dynamic_alpha);
    let word_x = min(grid_coord.x >> 5u, dimensions.x - 1u);
    let row = min(grid_coord.y, dimensions.y - 1u);
    let word = textureLoad(t_dynamic_alpha, vec2<u32>(word_x, row), 0).r;
    return (word & (1u << (grid_coord.x & 31u))) != 0u;
}

fn quantize_domain(value: f32, factor: u32) -> f32 {
    let divisions = f32(max(factor, 1u));
    return round(value * divisions) / divisions;
}

fn tessellated_domain(in: VertexInput) -> vec2<f32> {
    var domain = vec2<f32>(
        quantize_domain(in.local_uv.x, in.inside_factors.x),
        quantize_domain(in.local_uv.y, in.inside_factors.y),
    );
    if in.local_uv.y <= 0.0 {
        domain.x = quantize_domain(in.local_uv.x, in.edge_factors.y);
    } else if in.local_uv.y >= 1.0 {
        domain.x = quantize_domain(in.local_uv.x, in.edge_factors.w);
    }
    if in.local_uv.x <= 0.0 {
        domain.y = quantize_domain(in.local_uv.y, in.edge_factors.x);
    } else if in.local_uv.x >= 1.0 {
        domain.y = quantize_domain(in.local_uv.y, in.edge_factors.z);
    }
    return domain;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let num_verts = u32(shadow_params.terrain_info.x);
    let tile_scale = shadow_params.terrain_info.y;
    let num_patches_x = u32(shadow_params.terrain_info.z);

    let patch_x = in.patch_index / num_patches_x;
    let patch_z = in.patch_index % num_patches_x;
    let domain = tessellated_domain(in);

    let cell_x = min(patch_x * 16u + u32(round(domain.x * 16.0)), num_verts);
    let cell_z = min(patch_z * 16u + u32(round(domain.y * 16.0)), num_verts);
    let u = f32(cell_x) / f32(num_verts);
    let v = f32(cell_z) / f32(num_verts);

    let position_sample = textureSampleLevel(
        t_positions,
        s_position,
        vec2<f32>(v, u),
        u,
    ).bgr;
    let local_pos = vec3<f32>(
        position_sample.x,
        position_sample.y - shadow_params.mid.w,
        position_sample.z,
    ) * shadow_params.range.xyz - shadow_params.mid.xyz;

    // Reconstruct world position: grid * tileScale + displacement (same as terrain_gpu.wesl)
    let grid_x = f32(cell_x);
    let grid_z = f32(cell_z);
    let pos = vec3<f32>(
        grid_x * tile_scale + local_pos.x,
        local_pos.y,
        grid_z * tile_scale + local_pos.z,
    );

    // Transform to light clip space
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);
    out.alpha_uv = vec2<f32>(u, v);
    out.static_alpha = textureSampleLevel(t_alpha, s_alpha, out.alpha_uv, 0.0).r;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let num_verts = u32(shadow_params.terrain_info.x);
    let grid_coord = vec2<u32>(
        min(u32(saturate(in.alpha_uv.x) * f32(num_verts - 1u)), num_verts - 1u),
        min(u32(saturate(in.alpha_uv.y) * f32(num_verts - 1u)), num_verts - 1u),
    );
    let alpha = select(0.0, in.static_alpha, dynamic_alpha_enabled(grid_coord));
    if alpha < 0.5 {
        discard;
    }

    // The oracle applies a receiver-plane derivative bias in the caster PS.
    let raw_depth = in.clip_position.z;
    let slope = clamp(max(dpdxCoarse(raw_depth), dpdyCoarse(raw_depth)), 0.0, 0.004);
    let depth = raw_depth + slope * 1.2 + 0.0005;
    return vec4<f32>(depth, 0.0, 0.0, alpha);
}
