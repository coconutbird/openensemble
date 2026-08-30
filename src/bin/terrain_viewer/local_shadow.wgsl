// Terrain caster path for local spot and dual-paraboloid shadows.

struct ShadowParams {
    terrain_info: vec4<f32>,
    mid: vec4<f32>,
    range: vec4<f32>,
};

struct LocalShadowPass {
    transform: mat4x4<f32>,
    params: vec4<u32>,
};

@group(0) @binding(0) var<uniform> shadow_params: ShadowParams;
@group(0) @binding(1) var t_positions: texture_2d<f32>;
@group(0) @binding(2) var t_alpha: texture_2d<f32>;
@group(0) @binding(3) var s_alpha: sampler;
@group(0) @binding(4) var t_dynamic_alpha: texture_2d<u32>;
@group(0) @binding(5) var s_position: sampler;
@group(1) @binding(0) var<uniform> local_shadow_pass: LocalShadowPass;

struct VertexInput {
    @location(0) local_uv: vec2<f32>,
    @location(1) patch_index: u32,
    @location(2) edge_factors: vec4<u32>,
    @location(3) inside_factors: vec2<u32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) alpha_uv: vec2<f32>,
    @location(1) static_alpha: f32,
    @location(2) hemisphere: f32,
};

fn dynamic_alpha_enabled(uv: vec2<f32>) -> bool {
    let dimensions = textureDimensions(t_dynamic_alpha);
    let maximum_pixel = dimensions.y - 1u;
    let pixel = vec2<u32>(
        min(u32(clamp(uv.x, 0.0, 1.0) * f32(maximum_pixel)), maximum_pixel),
        min(u32(clamp(uv.y, 0.0, 1.0) * f32(maximum_pixel)), maximum_pixel),
    );
    let word_x = min(pixel.x >> 5u, dimensions.x - 1u);
    let word = textureLoad(t_dynamic_alpha, vec2<u32>(word_x, pixel.y), 0).r;
    return (word & (1u << (pixel.x & 31u))) != 0u;
}

fn quantize_domain(value: f32, factor: u32) -> f32 {
    let divisions = f32(max(factor, 1u));
    return round(value * divisions) / divisions;
}

fn tessellated_domain(input: VertexInput) -> vec2<f32> {
    var domain = vec2<f32>(
        quantize_domain(input.local_uv.x, input.inside_factors.x),
        quantize_domain(input.local_uv.y, input.inside_factors.y),
    );
    if input.local_uv.y <= 0.0 {
        domain.x = quantize_domain(input.local_uv.x, input.edge_factors.y);
    } else if input.local_uv.y >= 1.0 {
        domain.x = quantize_domain(input.local_uv.x, input.edge_factors.w);
    }
    if input.local_uv.x <= 0.0 {
        domain.y = quantize_domain(input.local_uv.y, input.edge_factors.x);
    } else if input.local_uv.x >= 1.0 {
        domain.y = quantize_domain(input.local_uv.y, input.edge_factors.z);
    }
    return domain;
}

fn terrain_grid(input: VertexInput) -> vec2<u32> {
    let num_verts = u32(shadow_params.terrain_info.x);
    let num_patches_x = u32(shadow_params.terrain_info.z);
    let patch_x = input.patch_index / num_patches_x;
    let patch_z = input.patch_index % num_patches_x;
    let domain = tessellated_domain(input);
    let cell_x = min(patch_x * 16u + u32(round(domain.x * 16.0)), num_verts);
    let cell_z = min(patch_z * 16u + u32(round(domain.y * 16.0)), num_verts);
    return vec2<u32>(cell_x, cell_z);
}

fn terrain_position(grid: vec2<u32>) -> vec4<f32> {
    let num_verts = u32(shadow_params.terrain_info.x);
    let uv = vec2<f32>(grid) / f32(num_verts);
    let sample = textureSampleLevel(t_positions, s_position, uv.yx, uv.x).bgr;
    let local_position = vec3<f32>(
        sample.x,
        sample.y - shadow_params.mid.w,
        sample.z,
    ) * shadow_params.range.xyz - shadow_params.mid.xyz;
    let tile_scale = shadow_params.terrain_info.y;
    return vec4<f32>(
        f32(grid.x) * tile_scale + local_position.x,
        local_position.y,
        f32(grid.y) * tile_scale + local_position.z,
        1.0,
    );
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let num_verts = shadow_params.terrain_info.x;
    let grid = terrain_grid(input);
    let world_position = terrain_position(grid);
    let transformed = local_shadow_pass.transform * world_position;
    var output: VertexOutput;
    if local_shadow_pass.params.x != 0u {
        let radial_distance = max(length(transformed.xyz), 0.0001);
        let direction = transformed.xyz / radial_distance;
        let denominator = max(direction.z + 1.0, 0.0001);
        output.clip_position = vec4<f32>(
            direction.xy / denominator,
            clamp((radial_distance - 1.0) / 100.0, 0.0, 1.0),
            1.0,
        );
        output.hemisphere = direction.z;
    } else {
        output.clip_position = transformed;
        output.hemisphere = 1.0;
    }
    output.alpha_uv = vec2<f32>(grid) / num_verts;
    // Static XTD alpha remains in authored SCN axes, transposed from world X/Z.
    output.static_alpha = textureSampleLevel(t_alpha, s_alpha, output.alpha_uv.yx, 0.0).r;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) {
    if input.hemisphere < 0.0 {
        discard;
    }
    let alpha = select(0.0, input.static_alpha, dynamic_alpha_enabled(input.alpha_uv));
    if alpha < 0.5 {
        discard;
    }
}
