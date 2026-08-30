struct SceneUniform {
    view_projection: mat4x4<f32>,
    world_to_view: mat4x4<f32>,
    view_to_world: mat4x4<f32>,
    camera_position: vec4<f32>,
    viewport_depth: vec4<f32>,
    light_volume_row0: vec4<f32>,
    light_volume_row1: vec4<f32>,
    light_volume_row2: vec4<f32>,
    light_volume_params: vec4<f32>,
};

struct MaterialUniform {
    flags: vec4<u32>,
    hdr_scales: vec4<f32>,
    light_params: vec4<f32>,
    corner_color_1: vec4<f32>,
    corner_color_2: vec4<f32>,
    corner_color_3: vec4<f32>,
    corner_color_4: vec4<f32>,
};

struct VertexInput {
    @location(0) position_rotation: vec4<f32>,
    @location(1) axis_half_length: vec4<f32>,
    @location(2) up_axis: vec4<f32>,
    @location(3) half_size_softness: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) intensity: vec4<f32>,
    @location(6) uv_rect0: vec4<f32>,
    @location(7) uv_rect1: vec4<f32>,
    @location(8) uv_rect2: vec4<f32>,
    @location(9) uv_rect_intensity: vec4<f32>,
    @location(10) texture_layers: vec4<u32>,
    @location(11) geometry: vec4<u32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) intensity: vec4<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) intensity_uv: vec2<f32>,
    @location(6) world_position: vec3<f32>,
    @location(7) view_depth: f32,
    @location(8) soft_fade_scale: f32,
    @location(9) valid: f32,
    @location(10) @interpolate(flat) texture_layers: vec4<u32>,
};

@group(0) @binding(0) var<uniform> scene: SceneUniform;
@group(1) @binding(0) var diffuse0: texture_2d_array<f32>;
@group(1) @binding(1) var diffuse1: texture_2d_array<f32>;
@group(1) @binding(2) var diffuse2: texture_2d_array<f32>;
@group(1) @binding(3) var intensity_map: texture_2d_array<f32>;
@group(1) @binding(4) var material_sampler: sampler;
@group(1) @binding(5) var scene_depth: texture_depth_2d;
@group(1) @binding(6) var light_volume: texture_3d<f32>;
@group(1) @binding(7) var light_volume_sampler: sampler;
@group(1) @binding(8) var<uniform> material: MaterialUniform;

const HAS_INTENSITY: u32 = 1u;
const USE_LIGHT_VOLUME: u32 = 2u;
const SOFT_PARTICLES: u32 = 4u;
const SOFT_FADE_RGB: u32 = 8u;
const PREMULTIPLY_COLOR_ALPHA: u32 = 16u;
const ALPHA_TEST: u32 = 32u;

const BILLBOARD: u32 = 0u;
const UP_FACING: u32 = 1u;
const ORIENTED_AXIAL: u32 = 2u;
const VELOCITY_ALIGNED: u32 = 3u;
const BEAM: u32 = 4u;
const TRAIL: u32 = 5u;
const TRAIL_CROSS: u32 = 6u;
const TERRAIN_PATCH: u32 = 7u;
const BEAM_VERTICAL: u32 = 8u;
const BEAM_HORIZONTAL: u32 = 9u;

const QUAD_CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
);

fn safe_normalize(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let length_squared = dot(value, value);
    return select(fallback, value * inverseSqrt(length_squared), length_squared > 0.000001);
}

fn rotate_basis(right: vec3<f32>, up: vec3<f32>, angle: f32) -> mat2x3<f32> {
    let sine = sin(angle);
    let cosine = cos(angle);
    return mat2x3<f32>(
        right * cosine + up * sine,
        up * cosine - right * sine,
    );
}

fn rotate_corner(corner: vec2<f32>, angle: f32) -> vec2<f32> {
    let sine = sin(angle);
    let cosine = cos(angle);
    return vec2<f32>(
        corner.x * cosine - corner.y * sine,
        corner.x * sine + corner.y * cosine,
    );
}

fn corner_light(geometry: u32, corner: vec2<f32>, angle: f32) -> vec4<f32> {
    if geometry != BILLBOARD
        && geometry != UP_FACING
        && geometry != ORIENTED_AXIAL
        && geometry != VELOCITY_ALIGNED
        && geometry != TERRAIN_PATCH
    {
        return vec4<f32>(1.0);
    }
    let selector = rotate_corner(corner, angle) * 0.5 + vec2<f32>(0.5);
    let upper = mix(material.corner_color_1, material.corner_color_2, selector.x);
    let lower = mix(material.corner_color_3, material.corner_color_4, selector.x);
    return mix(lower, upper, selector.y);
}

fn rect_uv(rect: vec4<f32>, corner: vec2<f32>) -> vec2<f32> {
    let unit = vec2<f32>(corner.x * 0.5 + 0.5, 0.5 - corner.y * 0.5);
    return mix(rect.xy, rect.zw, unit);
}

@vertex
fn vs_main(input: VertexInput, @builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let second_plane = vertex_index >= 6u;
    let corner = QUAD_CORNERS[vertex_index % 6u];
    let center = input.position_rotation.xyz;
    let camera_forward = safe_normalize(scene.camera_position.xyz - center, vec3<f32>(0.0, 0.0, 1.0));
    var right = safe_normalize(scene.view_to_world[0].xyz, vec3<f32>(1.0, 0.0, 0.0));
    var up = safe_normalize(scene.view_to_world[1].xyz, vec3<f32>(0.0, 1.0, 0.0));
    var half_width = input.half_size_softness.x;
    var half_height = input.half_size_softness.y;
    let geometry = input.geometry.x;
    let axis = safe_normalize(input.axis_half_length.xyz, vec3<f32>(0.0, 1.0, 0.0));
    let authored_up = safe_normalize(input.up_axis.xyz, vec3<f32>(0.0, 1.0, 0.0));

    if geometry == UP_FACING || geometry == TERRAIN_PATCH {
        right = vec3<f32>(1.0, 0.0, 0.0);
        up = vec3<f32>(0.0, 0.0, 1.0);
    } else if geometry == VELOCITY_ALIGNED {
        if abs(axis.y) >= 0.99999 {
            right = vec3<f32>(1.0, 0.0, 0.0);
            up = vec3<f32>(0.0, 0.0, 1.0);
        } else {
            right = safe_normalize(cross(vec3<f32>(0.0, 1.0, 0.0), axis), vec3<f32>(1.0, 0.0, 0.0));
            up = safe_normalize(cross(axis, right), vec3<f32>(0.0, 0.0, 1.0));
        }
    } else if geometry == ORIENTED_AXIAL {
        right = axis;
        up = safe_normalize(cross(camera_forward, axis), scene.view_to_world[0].xyz);
    } else if geometry == BEAM
        || geometry == BEAM_VERTICAL
        || geometry == BEAM_HORIZONTAL
        || geometry == TRAIL
        || geometry == TRAIL_CROSS
    {
        up = axis;
        half_height = input.axis_half_length.w;
        if geometry == BEAM_VERTICAL {
            right = vec3<f32>(0.0, 1.0, 0.0);
            half_width = input.half_size_softness.y;
        } else if geometry == BEAM_HORIZONTAL {
            right = safe_normalize(cross(axis, vec3<f32>(0.0, 1.0, 0.0)), scene.view_to_world[0].xyz);
        } else if geometry == TRAIL_CROSS {
            if second_plane {
                right = safe_normalize(cross(axis, authored_up), scene.view_to_world[0].xyz);
            } else {
                right = authored_up;
                half_width = input.half_size_softness.y;
            }
        } else {
            right = safe_normalize(cross(camera_forward, axis), scene.view_to_world[0].xyz);
        }
    }

    if geometry == BILLBOARD
        || geometry == UP_FACING
        || geometry == ORIENTED_AXIAL
        || geometry == VELOCITY_ALIGNED
        || geometry == TERRAIN_PATCH
    {
        let rotated = rotate_basis(right, up, input.position_rotation.w);
        right = rotated[0];
        up = rotated[1];
    }
    var valid = 1.0;
    if second_plane && geometry != TRAIL_CROSS {
        valid = 0.0;
        half_width = 0.0;
        half_height = 0.0;
    }
    let world_position = center
        + right * (corner.x * half_width)
        + up * (corner.y * half_height);
    let view_position = scene.world_to_view * vec4<f32>(world_position, 1.0);

    var output: VertexOutput;
    output.clip_position = scene.view_projection * vec4<f32>(world_position, 1.0);
    output.color = input.color * corner_light(geometry, corner, input.position_rotation.w);
    output.intensity = input.intensity;
    output.uv0 = rect_uv(input.uv_rect0, corner);
    output.uv1 = rect_uv(input.uv_rect1, corner);
    output.uv2 = rect_uv(input.uv_rect2, corner);
    output.intensity_uv = rect_uv(input.uv_rect_intensity, corner);
    output.world_position = world_position;
    output.view_depth = abs(view_position.z);
    output.soft_fade_scale = input.half_size_softness.z;
    output.valid = valid;
    output.texture_layers = input.texture_layers;
    return output;
}

fn clamped_layer(texture: texture_2d_array<f32>, requested: u32) -> u32 {
    return min(requested, textureNumLayers(texture) - 1u);
}

fn particle_diffuse(input: VertexOutput) -> vec4<f32> {
    let first = textureSample(
        diffuse0,
        material_sampler,
        input.uv0,
        clamped_layer(diffuse0, input.texture_layers.x),
    );
    let first_rgb = first.rgb * material.hdr_scales.x;
    if material.flags.y <= 1u {
        return vec4<f32>(first_rgb, first.a);
    }

    let second = textureSample(
        diffuse1,
        material_sampler,
        input.uv1,
        clamped_layer(diffuse1, input.texture_layers.y),
    );
    let second_rgb = second.rgb * material.hdr_scales.y;
    var accumulated: vec3<f32>;
    if material.flags.z == 0u {
        accumulated = (first.a * first_rgb) * (second.a * second_rgb);
    } else {
        accumulated = mix(first_rgb, second_rgb, second.a);
    }

    if material.flags.y >= 3u {
        let third = textureSample(
            diffuse2,
            material_sampler,
            input.uv2,
            clamped_layer(diffuse2, input.texture_layers.z),
        );
        let third_rgb = third.rgb * material.hdr_scales.z;
        if material.flags.w == 0u {
            accumulated += accumulated * (third.a * third_rgb);
        } else {
            accumulated = mix(accumulated, third_rgb, third.a);
        }
    }
    return vec4<f32>(accumulated, 1.0);
}

fn soft_fade(input: VertexOutput) -> f32 {
    if (material.flags.x & SOFT_PARTICLES) == 0u {
        return 1.0;
    }
    let pixel = clamp(
        vec2<i32>(input.clip_position.xy),
        vec2<i32>(0),
        vec2<i32>(scene.viewport_depth.xy) - vec2<i32>(1),
    );
    let device_depth = textureLoad(scene_depth, pixel, 0);
    let denominator = device_depth * scene.viewport_depth.z + scene.viewport_depth.w;
    let scene_eye_depth = select(
        3.4e38,
        1.0 / denominator,
        abs(denominator) > 0.000001,
    );
    return clamp(
        (scene_eye_depth - input.view_depth) * input.soft_fade_scale,
        0.0,
        1.0,
    );
}

fn light_volume_factor(world_position: vec3<f32>) -> vec3<f32> {
    if (material.flags.x & USE_LIGHT_VOLUME) == 0u {
        return vec3<f32>(1.0);
    }
    let position = vec4<f32>(world_position, 1.0);
    let uvw = vec3<f32>(
        dot(position, scene.light_volume_row0),
        dot(position, scene.light_volume_row1),
        dot(position, scene.light_volume_row2),
    );
    let sampled = textureSample(light_volume, light_volume_sampler, uvw).rgb
        * material.light_params.x
        * scene.light_volume_params.x;
    return sampled + vec3<f32>(1.0);
}

@fragment
fn fs_color(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.valid < 0.5 {
        discard;
    }
    let sampled = particle_diffuse(input);
    var alpha = sampled.a * input.color.a;
    var vertex_rgb = input.color.rgb;
    if (material.flags.x & PREMULTIPLY_COLOR_ALPHA) != 0u {
        vertex_rgb *= input.color.a;
    }
    var rgb = pow(
        clamp(sampled.rgb * vertex_rgb, vec3<f32>(0.0), vec3<f32>(1.0)),
        vec3<f32>(2.0),
    );
    var intensity_rgb = input.intensity.rgb;
    if (material.flags.x & HAS_INTENSITY) != 0u {
        let intensity_sample = textureSample(
            intensity_map,
            material_sampler,
            input.intensity_uv,
            clamped_layer(intensity_map, input.texture_layers.w),
        );
        intensity_rgb *= intensity_sample.rgb * (16.0 * material.hdr_scales.w);
        if material.flags.y > 1u {
            alpha *= intensity_sample.a;
            if (material.flags.x & SOFT_FADE_RGB) != 0u {
                rgb *= intensity_sample.a;
            }
        }
    }
    rgb *= intensity_rgb
        * light_volume_factor(input.world_position);
    let fade = soft_fade(input);
    if (material.flags.x & SOFT_FADE_RGB) != 0u {
        rgb *= fade;
    } else {
        alpha *= fade;
    }
    if (material.flags.x & ALPHA_TEST) != 0u && alpha <= 0.0 {
        discard;
    }
    return vec4<f32>(rgb, alpha);
}

@fragment
fn fs_distortion(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.valid < 0.5 {
        discard;
    }
    let sampled = textureSample(
        diffuse0,
        material_sampler,
        input.uv0,
        clamped_layer(diffuse0, input.texture_layers.x),
    ).rgb * 2.0 - vec3<f32>(1.0);
    var intensity_xy = input.intensity.xy;
    if (material.flags.x & HAS_INTENSITY) != 0u {
        let intensity_sample = textureSample(
            intensity_map,
            material_sampler,
            input.intensity_uv,
            clamped_layer(intensity_map, input.texture_layers.w),
        );
        intensity_xy *= intensity_sample.xy * (16.0 * material.hdr_scales.w);
    }
    let distortion = vec3<f32>(sampled.xy * intensity_xy, sampled.z) * input.color.a;
    let coverage = abs(distortion.x) + abs(distortion.y);
    if coverage <= 0.0 {
        discard;
    }
    return vec4<f32>(distortion, coverage);
}
