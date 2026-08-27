// Translation of the Halo Wars PC parametricshader legacy-material path.

const HAS_DIFFUSE: u32 = 1u << 0u;
const HAS_NORMAL: u32 = 1u << 1u;
const HAS_GLOSS: u32 = 1u << 2u;
const HAS_OPACITY: u32 = 1u << 3u;
const HAS_XFORM: u32 = 1u << 4u;
const HAS_EMISSIVE: u32 = 1u << 5u;
const HAS_AO: u32 = 1u << 6u;
const COLOR_GLOSS: u32 = 1u << 7u;
const TWO_SIDED: u32 = 1u << 8u;

struct Scene {
    view_projection: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_position: vec4<f32>,
    dir_light_vector: vec4<f32>,
    dir_light_color: vec4<f32>,
    sh_fill_ar: vec4<f32>,
    sh_fill_ag: vec4<f32>,
    sh_fill_ab: vec4<f32>,
    sh_fill_br: vec4<f32>,
    sh_fill_bg: vec4<f32>,
    sh_fill_bb: vec4<f32>,
    sh_fill_c: vec4<f32>,
    fog_color: vec4<f32>,
    fog_params: vec4<f32>,
    planar_fog_color: vec4<f32>,
    planar_fog_params: vec4<f32>,
    ao_params: vec4<f32>,
};

struct Material {
    tint: vec4<f32>,
    specular: vec4<f32>,
    params: vec4<f32>,
    flags: vec4<u32>,
    channels0: vec4<u32>,
    channels1: vec4<u32>,
};

@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var<storage, read> matrix_palette: array<mat4x4<f32>>;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var diffuse_map: texture_2d<f32>;
@group(1) @binding(2) var normal_map: texture_2d<f32>;
@group(1) @binding(3) var gloss_map: texture_2d<f32>;
@group(1) @binding(4) var opacity_map: texture_2d<f32>;
@group(1) @binding(5) var xform_map: texture_2d<f32>;
@group(1) @binding(6) var emissive_map: texture_2d<f32>;
@group(1) @binding(7) var ao_map: texture_2d<f32>;
@group(1) @binding(8) var material_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) binormal: vec4<f32>,
    @location(4) texcoord0: vec2<f32>,
    @location(5) texcoord1: vec2<f32>,
    @location(6) texcoord2: vec2<f32>,
    @location(7) color: vec4<f32>,
    @location(8) joints: vec4<u32>,
    @location(9) weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_tangent: vec3<f32>,
    @location(3) world_binormal: vec3<f32>,
    @location(4) texcoord0: vec2<f32>,
    @location(5) texcoord1: vec2<f32>,
    @location(6) texcoord2: vec2<f32>,
    @location(7) color: vec4<f32>,
    @location(8) fog_densities: vec2<f32>,
};

fn has_flag(flag: u32) -> bool {
    return (material.flags.x & flag) != 0u;
}

fn skin_matrix(input: VertexInput) -> mat4x4<f32> {
    return matrix_palette[input.joints.x] * input.weights.x
        + matrix_palette[input.joints.y] * input.weights.y
        + matrix_palette[input.joints.z] * input.weights.z
        + matrix_palette[input.joints.w] * input.weights.w;
}

fn radial_fog(world_position: vec3<f32>) -> f32 {
    let camera_vector = scene.camera_position.xyz - world_position;
    let distance_squared = dot(camera_vector, camera_vector);
    return clamp(
        exp2(-scene.fog_params.x * max(0.0, distance_squared - scene.fog_params.y)),
        0.0,
        1.0,
    );
}

fn planar_fog(world_position: vec3<f32>) -> f32 {
    if scene.planar_fog_params.x <= 0.5 {
        return 1.0;
    }
    let camera_vector = scene.camera_position.xyz - world_position;
    let distance = length(camera_vector);
    if distance < 0.001 {
        return 1.0;
    }
    let camera_y_direction = camera_vector.y / distance;
    let fog_height = world_position.y - scene.planar_fog_params.y;
    let inverse_y_direction = clamp(1.0 / -camera_y_direction, -3.4e38, 3.4e38);
    var effective_distance: f32;
    if camera_y_direction < 0.0 {
        effective_distance = distance - fog_height * inverse_y_direction;
    } else {
        effective_distance = fog_height * inverse_y_direction;
    }
    effective_distance = min(distance, max(0.0, effective_distance));
    return clamp(
        exp2(-scene.planar_fog_params.z * effective_distance * effective_distance),
        0.0,
        1.0,
    );
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let skin = skin_matrix(input);
    let world_matrix = scene.model * skin;
    let world_position4 = world_matrix * vec4<f32>(input.position, 1.0);
    let normal = normalize((world_matrix * vec4<f32>(input.normal, 0.0)).xyz);
    var tangent = normalize((world_matrix * vec4<f32>(input.tangent.xyz, 0.0)).xyz);
    tangent = normalize(tangent - normal * dot(tangent, normal));
    let handedness = select(1.0, sign(input.tangent.w), abs(input.tangent.w) > 0.0001);
    let binormal = normalize(cross(tangent, normal)) * handedness;

    var output: VertexOutput;
    output.clip_position = scene.view_projection * world_position4;
    output.world_position = world_position4.xyz;
    output.world_normal = normal;
    output.world_tangent = tangent;
    output.world_binormal = binormal;
    output.texcoord0 = input.texcoord0;
    output.texcoord1 = input.texcoord1;
    output.texcoord2 = input.texcoord2;
    output.color = input.color;
    output.fog_densities = vec2<f32>(radial_fog(world_position4.xyz), planar_fog(world_position4.xyz));
    return output;
}

fn choose_uv(channel: u32, input: VertexOutput) -> vec2<f32> {
    switch channel {
        case 1u: { return input.texcoord1; }
        case 2u: { return input.texcoord2; }
        default: { return input.texcoord0; }
    }
}

fn sh_fill(normal: vec3<f32>) -> vec3<f32> {
    let normal4 = vec4<f32>(normal, 1.0);
    let linear = vec3<f32>(
        dot(normal4, scene.sh_fill_ar),
        dot(normal4, scene.sh_fill_ag),
        dot(normal4, scene.sh_fill_ab),
    );
    let quadratic_input = vec4<f32>(
        normal.x * normal.y,
        normal.y * normal.z,
        normal.z * normal.z,
        normal.z * normal.x,
    );
    let quadratic = vec3<f32>(
        dot(quadratic_input, scene.sh_fill_br),
        dot(quadratic_input, scene.sh_fill_bg),
        dot(quadratic_input, scene.sh_fill_bb),
    );
    return linear + quadratic + scene.sh_fill_c.xyz * (normal.x * normal.x - normal.y * normal.y);
}

fn reciprocal_specular(response: f32, power: f32) -> f32 {
    let denominator = power - power * response + response;
    return select(0.0, response / denominator, denominator >= 0.0001);
}

fn fog_color(color: vec3<f32>, densities: vec2<f32>) -> vec3<f32> {
    let planar = mix(scene.planar_fog_color.xyz, color, densities.y);
    return mix(scene.fog_color.xyz, planar, densities.x);
}

@fragment
fn fs_main(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    var diffuse_sample = vec4<f32>(1.0);
    if has_flag(HAS_DIFFUSE) {
        diffuse_sample = textureSample(
            diffuse_map,
            material_sampler,
            choose_uv(material.channels0.x, input),
        );
    }

    var tint_mask = diffuse_sample.a;
    if has_flag(HAS_XFORM) {
        tint_mask = textureSample(
            xform_map,
            material_sampler,
            choose_uv(material.channels1.x, input),
        ).g;
    }
    let tint = mix(vec3<f32>(1.0), material.tint.rgb, tint_mask);
    let albedo = diffuse_sample.rgb * tint * input.color.rgb;

    var opacity = material.params.x * material.tint.a * input.color.a;
    if has_flag(HAS_OPACITY) {
        opacity *= textureSample(
            opacity_map,
            material_sampler,
            choose_uv(material.channels0.w, input),
        ).r;
    }
    if opacity <= max(2.0 / 255.0, material.params.y) {
        discard;
    }

    var vertex_normal = normalize(input.world_normal);
    if has_flag(TWO_SIDED) && !front_facing {
        vertex_normal = -vertex_normal;
    }
    var world_normal = vertex_normal;
    if has_flag(HAS_NORMAL) {
        let encoded = textureSample(
            normal_map,
            material_sampler,
            choose_uv(material.channels0.y, input),
        ).rg * 2.0 - vec2<f32>(1.0);
        var tangent_normal = vec3<f32>(
            encoded,
            sqrt(max(1.0 - dot(encoded, encoded), 0.0)),
        );
        tangent_normal.x *= material.params.w;
        tangent_normal.y *= material.params.w;
        tangent_normal = normalize(tangent_normal);
        world_normal = normalize(
            input.world_tangent * tangent_normal.x
            + input.world_binormal * tangent_normal.y
            + vertex_normal * tangent_normal.z
        );
    }

    var ambient = sh_fill(world_normal);
    if has_flag(HAS_AO) {
        let ao = textureSample(
            ao_map,
            material_sampler,
            choose_uv(material.channels1.z, input),
        ).r;
        ambient *= mix(1.0, ao, scene.ao_params.x);
    }
    let light_direction = normalize(scene.dir_light_vector.xyz);
    let directional_response = max(dot(world_normal, light_direction), 0.0);
    let directional = scene.dir_light_color.rgb * directional_response;

    var specular_color = material.specular.rgb;
    var gloss_strength = 1.0;
    if has_flag(HAS_GLOSS) {
        let gloss = textureSample(
            gloss_map,
            material_sampler,
            choose_uv(material.channels0.z, input),
        );
        if has_flag(COLOR_GLOSS) {
            specular_color *= gloss.rgb;
        } else {
            gloss_strength = gloss.r;
        }
    }
    let view_direction = normalize(scene.camera_position.xyz - input.world_position);
    let reflected_view = reflect(-view_direction, world_normal);
    let reflection_response = max(dot(reflected_view, light_direction), 0.0);
    let specular = directional
        * specular_color
        * gloss_strength
        * reciprocal_specular(reflection_response, material.specular.w);

    var emissive = vec3<f32>(0.0);
    if has_flag(HAS_EMISSIVE) {
        let self_sample = textureSample(
            emissive_map,
            material_sampler,
            choose_uv(material.channels1.y, input),
        );
        emissive = pow(self_sample.rgb, vec3<f32>(2.2))
            * self_sample.a
            * material.params.z
            * 32.0;
    }

    let lit = albedo * max(ambient + directional, vec3<f32>(0.0)) + specular + emissive;
    return vec4<f32>(fog_color(lit, input.fog_densities), opacity);
}
