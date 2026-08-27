// Oracle-oriented terrain compositor based on gpuTerrainComposite shaders #0-#7.
// Multiple source layers are folded into one draw, preserving the original
// SrcAlpha/InvSrcAlpha ordering.

struct CompositeParams {
    chunk_index: u32,
    num_layers: u32,
    debug_mode: u32,
    num_decal_layers: u32,
    lod_level: u32,
    hdr_mode: u32,
    hdr_scale: f32,
    chunks_x: u32,
};

@group(0) @binding(0) var<uniform> params: CompositeParams;
@group(0) @binding(1) var t_terrain_array: texture_2d_array<f32>;
@group(0) @binding(2) var t_alpha_atlas: texture_2d_array<f32>;
@group(0) @binding(3) var<storage, read> chunk_layers: array<u32>;
@group(0) @binding(4) var<storage, read> texture_scales: array<vec2<f32>>;
@group(0) @binding(5) var s_terrain: sampler;
@group(0) @binding(6) var s_alpha: sampler;
@group(0) @binding(7) var t_alpha_atlas_hi: texture_2d_array<f32>;
@group(0) @binding(8) var t_decal_alpha_atlas: texture_2d_array<f32>;
@group(0) @binding(9) var<storage, read> chunk_decal_layers: array<u32>;
// xyz = rotation/center, w contains bitcast<u32>(decal texture index).
@group(0) @binding(10) var<storage, read> decal_instances: array<vec4<f32>>;
@group(0) @binding(11) var<storage, read> decal_uv_scales: array<vec2<f32>>;
@group(0) @binding(12) var t_normal_array: texture_2d_array<f32>;
@group(0) @binding(13) var t_decal_alpha_atlas_hi: texture_2d_array<f32>;
@group(0) @binding(14) var t_decal_diffuse_array: texture_2d_array<f32>;
@group(0) @binding(15) var t_decal_opacity_array: texture_2d_array<f32>;
@group(0) @binding(16) var t_specular_array: texture_2d_array<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct CompositeOutput {
    @location(0) albedo: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) specular: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index >> 1u) * 4 - 1);
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

fn layer_alpha(layer: u32, low: vec4<f32>, high: vec4<f32>) -> f32 {
    if layer == 0u { return 1.0; }
    if layer == 1u { return low.r; }
    if layer == 2u { return low.g; }
    if layer == 3u { return low.b; }
    if layer == 4u { return low.a; }
    if layer == 5u { return high.r; }
    if layer == 6u { return high.g; }
    if layer == 7u { return high.b; }
    return 0.0;
}

fn decode_hdr(sample: vec4<f32>) -> vec3<f32> {
    let linear = pow(abs(sample), vec4<f32>(2.2));
    return linear.rgb * linear.a * 32.0 * params.hdr_scale;
}

fn sample_albedo(uv: vec2<f32>, layer: u32) -> vec3<f32> {
    let sample = textureSampleLevel(
        t_terrain_array,
        s_terrain,
        uv,
        layer,
        f32(params.lod_level),
    );
    if params.hdr_mode != 0u {
        return decode_hdr(sample);
    }
    return sample.rgb;
}

fn decal_coverage(
    decal_layer: u32,
    low: vec4<f32>,
    high: vec4<f32>,
) -> f32 {
    if decal_layer == 0u { return low.r; }
    if decal_layer == 1u { return low.g; }
    if decal_layer == 2u { return low.b; }
    if decal_layer == 3u { return low.a; }
    if decal_layer == 4u { return high.r; }
    if decal_layer == 5u { return high.g; }
    if decal_layer == 6u { return high.b; }
    if decal_layer == 7u { return high.a; }
    return 0.0;
}

fn rotate_decal_uv(
    uv: vec2<f32>,
    rotation: f32,
    center: vec2<f32>,
    uv_scale: vec2<f32>,
) -> vec2<f32> {
    // Exact direct-decal transform from compositor shader #4/#5.
    let negative_center = uv_scale * -center.yx;
    let sine = sin(rotation);
    let cosine = cos(rotation);
    let negative_sine = sin(-rotation);
    let input_uv = uv.yx;
    return vec2<f32>(
        cosine * uv_scale.x * input_uv.x
            + sine * uv_scale.y * input_uv.y
            + dot(vec2<f32>(cosine, sine), negative_center),
        negative_sine * uv_scale.x * input_uv.x
            + cosine * uv_scale.y * input_uv.y
            + dot(vec2<f32>(negative_sine, cosine), negative_center),
    ) + vec2<f32>(0.5);
}

@fragment
fn fs_main(in: VertexOutput) -> CompositeOutput {
    let chunk_index = params.chunk_index;
    let chunks_x = max(params.chunks_x, 1u);
    let chunks_z = max(textureNumLayers(t_alpha_atlas) / chunks_x, 1u);
    let grid_z = chunk_index / chunks_x;
    let grid_x = chunk_index % chunks_x;
    let half_pixel = 0.5 / 64.0;
    let alpha_uv = mix(vec2<f32>(half_pixel), vec2<f32>(1.0 - half_pixel), in.uv);
    let alpha_low = textureSampleLevel(t_alpha_atlas, s_alpha, alpha_uv, chunk_index, 0.0);
    let alpha_high = textureSampleLevel(t_alpha_atlas_hi, s_alpha, alpha_uv, chunk_index, 0.0);
    let tiling_uv = vec2<f32>(f32(grid_z) + in.uv.y, f32(grid_x) + in.uv.x);
    let layer_base = chunk_index * 8u;
    let base_layer = chunk_layers[layer_base];
    let base_uv = tiling_uv * texture_scales[base_layer];

    var albedo = sample_albedo(base_uv, base_layer);
    var encoded_normal = textureSampleLevel(
        t_normal_array,
        s_terrain,
        base_uv,
        base_layer,
        f32(params.lod_level),
    );
    var specular = textureSampleLevel(
        t_specular_array,
        s_terrain,
        base_uv,
        base_layer,
        f32(params.lod_level),
    );

    let layer_count = min(max(params.num_layers, 1u), 8u);
    for (var layer_slot = 1u; layer_slot < layer_count; layer_slot = layer_slot + 1u) {
        let alpha = layer_alpha(layer_slot, alpha_low, alpha_high);
        if alpha > 0.0 {
            let texture_layer = chunk_layers[layer_base + layer_slot];
            let layer_uv = tiling_uv * texture_scales[texture_layer];
            albedo = mix(albedo, sample_albedo(layer_uv, texture_layer), alpha);
            let normal_sample = textureSampleLevel(
                t_normal_array,
                s_terrain,
                layer_uv,
                texture_layer,
                f32(params.lod_level),
            );
            encoded_normal = mix(encoded_normal, normal_sample, alpha);
            let specular_sample = textureSampleLevel(
                t_specular_array,
                s_terrain,
                layer_uv,
                texture_layer,
                f32(params.lod_level),
            );
            specular = mix(specular, specular_sample, alpha);
        }
    }

    if params.num_decal_layers > 0u {
        let decal_alpha_low = textureSampleLevel(
            t_decal_alpha_atlas,
            s_alpha,
            alpha_uv,
            chunk_index,
            0.0,
        );
        let decal_alpha_high = textureSampleLevel(
            t_decal_alpha_atlas_hi,
            s_alpha,
            alpha_uv,
            chunk_index,
            0.0,
        );
        let decal_base = chunk_index * 8u;
        let decal_count = min(params.num_decal_layers, 8u);
        for (var decal_layer = 0u; decal_layer < decal_count; decal_layer = decal_layer + 1u) {
            let instance_index = chunk_decal_layers[decal_base + decal_layer];
            let instance = decal_instances[instance_index];
            let texture_index = bitcast<u32>(instance.w);
            let decal_uv = rotate_decal_uv(
                in.uv,
                instance.x,
                instance.yz,
                decal_uv_scales[instance_index],
            );
            let opacity = textureSampleLevel(
                t_decal_opacity_array,
                s_alpha,
                decal_uv,
                texture_index,
                f32(params.lod_level),
            ).r;
            let alpha = decal_coverage(decal_layer, decal_alpha_low, decal_alpha_high) * opacity;
            if alpha > 0.0 {
                let decal_sample = textureSampleLevel(
                    t_decal_diffuse_array,
                    s_alpha,
                    decal_uv,
                    texture_index,
                    f32(params.lod_level),
                );
                let decal_color = select(decal_sample.rgb, decode_hdr(decal_sample), params.hdr_mode != 0u);
                albedo = mix(albedo, decal_color, alpha);
            }
        }
    }

    if params.debug_mode == 1u {
        albedo = vec3<f32>(in.uv, 0.0);
    } else if params.debug_mode == 2u {
        albedo = vec3<f32>(f32(grid_x) / f32(chunks_x), f32(grid_z) / f32(chunks_z), 0.0);
    } else if params.debug_mode == 3u {
        albedo = alpha_low.rgb;
    } else if params.debug_mode == 4u {
        albedo = sample_albedo(base_uv, base_layer);
    } else if params.debug_mode == 5u {
        albedo = vec3<f32>(tiling_uv / vec2<f32>(f32(chunks_z), f32(chunks_x)), 0.0);
    }

    var out: CompositeOutput;
    out.albedo = vec4<f32>(albedo, 1.0);
    out.normal = encoded_normal;
    out.specular = specular;
    return out;
}
