// Hit-point bars: translation of the `renderSquadHPBar` technique of `HPBar\HPBar.bin`.
// Each bar is a camera-facing world quad, drawn into the tone-mapped output without a depth
// test. Retail expands a point in a geometry shader; wgpu has no geometry stage, so the vertex
// shader expands six vertices for each instance.

struct HpBarCamera {
    view_projection: mat4x4<f32>,
    // Camera right direction in world space (`gViewToWorld` column 0).
    right: vec4<f32>,
    // Camera up direction in world space (`gViewToWorld` column 1).
    up: vec4<f32>,
    // x = 1 when the output format is sRGB, so the shader writes linear color.
    output: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: HpBarCamera;

@group(1) @binding(0)
var t_frame: texture_2d<f32>;
@group(1) @binding(1)
var t_mask: texture_2d<f32>;
@group(1) @binding(2)
var s_bar: sampler;

struct InstanceInput {
    // xyz = bar center, w = width.
    @location(0) position_width: vec4<f32>,
    // xyz = fill end U of the hit-point, shield and ammunition fills, w = height.
    @location(1) fill_height: vec4<f32>,
    @location(2) player_color: vec4<f32>,
    @location(3) hitpoint_color: vec4<f32>,
    @location(4) shield_color: vec4<f32>,
    @location(5) ammo_color: vec4<f32>,
    // [u1, v1, u2, v2] fill rectangles. An absent fill has a rectangle outside 0..1.
    @location(6) hitpoint_uv: vec4<f32>,
    @location(7) shield_uv: vec4<f32>,
    @location(8) ammo_uv: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) fill_end: vec3<f32>,
    @location(2) @interpolate(flat) player_color: vec4<f32>,
    @location(3) @interpolate(flat) hitpoint_color: vec4<f32>,
    @location(4) @interpolate(flat) shield_color: vec4<f32>,
    @location(5) @interpolate(flat) ammo_color: vec4<f32>,
    @location(6) @interpolate(flat) hitpoint_uv: vec4<f32>,
    @location(7) @interpolate(flat) shield_uv: vec4<f32>,
    @location(8) @interpolate(flat) ammo_uv: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: InstanceInput) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let width = instance.position_width.w;
    let height = instance.fill_height.w;
    let world_position = instance.position_width.xyz
        + camera.right.xyz * ((corner.x - 0.5) * width)
        + camera.up.xyz * ((0.5 - corner.y) * height);

    var out: VertexOutput;
    out.clip_position = camera.view_projection * vec4<f32>(world_position, 1.0);
    out.uv = corner;
    out.fill_end = instance.fill_height.xyz;
    out.player_color = instance.player_color;
    out.hitpoint_color = instance.hitpoint_color;
    out.shield_color = instance.shield_color;
    out.ammo_color = instance.ammo_color;
    out.hitpoint_uv = instance.hitpoint_uv;
    out.shield_uv = instance.shield_uv;
    out.ammo_uv = instance.ammo_uv;
    return out;
}

// Retail color of the empty part of a fill.
const EMPTY_FILL: vec4<f32> = vec4<f32>(0.1, 0.1, 0.1, 0.5);

// Blends `base` by the mask alpha to the fill color inside the fill rectangle.
fn apply_fill(
    base: vec4<f32>,
    uv: vec2<f32>,
    mask: f32,
    rect: vec4<f32>,
    fill_end: f32,
    color: vec4<f32>,
) -> vec4<f32> {
    let inside = all(uv >= rect.xy) && all(uv <= rect.zw);
    let fill = select(color, EMPTY_FILL, uv.x > fill_end);
    return select(base, base + mask * (fill - base), inside);
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    let low = color / 12.92;
    let high = pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, color <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let mask = textureSample(t_mask, s_bar, in.uv).a;
    var color = textureSample(t_frame, s_bar, in.uv) * in.player_color;
    color = apply_fill(color, in.uv, mask, in.hitpoint_uv, in.fill_end.x, in.hitpoint_color);
    color = apply_fill(color, in.uv, mask, in.shield_uv, in.fill_end.y, in.shield_color);
    color = apply_fill(color, in.uv, mask, in.ammo_uv, in.fill_end.z, in.ammo_color);
    if color.a <= 0.001 {
        discard;
    }
    if camera.output.x > 0.5 {
        color = vec4<f32>(srgb_to_linear(clamp(color.rgb, vec3<f32>(0.0), vec3<f32>(1.0))), color.a);
    }
    return color;
}
