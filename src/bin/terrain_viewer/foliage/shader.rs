//! Foliage shader - WGSL implementation of terrainFoliage.fx

/// Foliage rendering shader.
///
/// Based on the original Halo Wars terrainFoliage.fx shader.
///
/// Key features from original:
/// - Blade geometry fetched from position/normal textures
/// - Random rotation per blade using deterministic hash
/// - Random height scaling (0.25-1.0)
/// - Terrain height sampling for blade base position
/// - Distance-based alpha fade (400-500 units)
/// - Two-sided lighting (normal flipped toward camera)
pub const FOLIAGE_SHADER: &str = r#"
// Camera uniform (shared with terrain shader - just view_proj matrix)
struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct FoliageParams {
    // Terrain info: num_verts_per_axis, tile_scale, chunk_offset_x, chunk_offset_z
    terrain_info: vec4<f32>,
    // World bounds for terrain height sampling
    world_min: vec3<f32>,
    _pad0: f32,
    world_range: vec3<f32>,
    _pad1: f32,
    // Foliage params: num_verts_per_blade, rcp_num_blades, fade_start, fade_end
    foliage_info: vec4<f32>,
    // Camera position for distance fade and two-sided lighting
    camera_pos: vec3<f32>,
    time: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> params: FoliageParams;

// Blade geometry textures (position.xyz + u, normal.xyz + v)
@group(1) @binding(1)
var t_blade_positions: texture_2d<f32>;

@group(1) @binding(2)
var t_blade_normals: texture_2d<f32>;

@group(1) @binding(3)
var s_blade: sampler;

// Terrain heightmap for positioning blades
@group(1) @binding(4)
var t_heightmap: texture_2d<f32>;

@group(1) @binding(5)
var s_heightmap: sampler;

// Foliage textures
@group(2) @binding(0)
var t_albedo: texture_2d<f32>;

@group(2) @binding(1)
var t_opacity: texture_2d<f32>;

@group(2) @binding(2)
var s_foliage: sampler;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) alpha_fade: f32,
};

// Deterministic pseudo-random function (matching original shader)
const M_PI: f32 = 3.14159265358979323846;

fn our_rand(ij: vec2<f32>) -> vec2<f32> {
    let xy0 = ij / M_PI;
    let xym = (xy0 % 257.0) + 1.0;
    let xym2 = fract(xym * xym);
    return xym2;
}

fn rotate_2d(input: vec2<f32>, theta: f32) -> vec2<f32> {
    let sintheta = sin(theta);
    let costheta = cos(theta);
    return vec2<f32>(
        input.x * costheta + input.y * sintheta,
        input.y * costheta - input.x * sintheta
    );
}

// Sample terrain height at given world XZ position
fn get_terrain_height(world_xz: vec2<f32>) -> f32 {
    let num_verts = params.terrain_info.x;
    let tile_scale = params.terrain_info.y;
    let terrain_size = (num_verts - 1.0) * tile_scale;

    // Convert world position to UV coordinates
    let uv = clamp(world_xz / terrain_size, vec2<f32>(0.0), vec2<f32>(1.0));

    // Sample height (Y) from heightmap
    let height_sample = textureSampleLevel(t_heightmap, s_heightmap, uv, 0.0);
    return height_sample.r; // R32Float texture stores height directly
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    let num_verts_per_blade = u32(params.foliage_info.x);
    let num_blades_per_node: u32 = 64u;
    
    // Decode blade and vertex index from instance/vertex
    // instance_index encodes: blade_type (high 16 bits) + blade_index (low 16 bits)
    let blade_type = in.instance_index >> 16u;
    let blade_index = in.instance_index & 0xFFFFu;
    let vertex_in_blade = in.vertex_index % num_verts_per_blade;
    
    // Fetch blade vertex from geometry texture
    let pixel_coord = vec2<f32>(
        f32(vertex_in_blade) + f32(blade_type * num_verts_per_blade),
        0.5
    );
    let tex_size = vec2<f32>(textureDimensions(t_blade_positions, 0));
    let tex_uv = pixel_coord / tex_size;
    
    let pos_sample = textureSampleLevel(t_blade_positions, s_blade, tex_uv, 0.0);
    let norm_sample = textureSampleLevel(t_blade_normals, s_blade, tex_uv, 0.0);

    // Blade position from texture - scale up for visibility
    // Original blade geometry is in model units, scale to world units
    var pos = pos_sample.xyz * 5.0; // Scale blade size
    let blade_uv = vec2<f32>(pos_sample.w, 1.0 - norm_sample.w);
    var normal = norm_sample.xyz;

    // Random values per blade (deterministic based on blade index)
    let rnd = our_rand(vec2<f32>(f32(blade_index), f32(blade_index)));

    // Grid position - spread blades across terrain
    // With 4096 blades in 64x64 grid, scale by ~16 to cover full terrain (1024 verts)
    let num_verts = params.terrain_info.x;
    let grid_scale = (num_verts - 1.0) / 64.0; // Scale factor to cover full terrain

    var grid_pos = vec2<f32>(
        f32(blade_index / num_blades_per_node) + 0.5,
        f32(blade_index % num_blades_per_node)
    );
    // Scale to cover more terrain
    grid_pos *= grid_scale;
    // Add chunk offset (for per-chunk rendering later)
    grid_pos += vec2<f32>(params.terrain_info.z, params.terrain_info.w);
    // Add random jitter within cell (scaled)
    grid_pos += (rnd * 2.0 - 1.0) * 0.9 * grid_scale;

    // Rotate blade around Y axis
    let rotation_angle = rnd.y * 360.0;
    pos.x = rotate_2d(pos.xz, rotation_angle).x;
    pos.z = rotate_2d(pos.xz, rotation_angle).y;
    normal.x = rotate_2d(normal.xz, rotation_angle).x;
    normal.z = rotate_2d(normal.xz, rotation_angle).y;
    normal = normalize(normal);

    // Scale position by tile scale and add grid offset to get world XZ
    let tile_scale = params.terrain_info.y;
    let world_x = grid_pos.x * tile_scale;
    let world_z = grid_pos.y * tile_scale;

    // Random height scaling (0.25 to 1.0)
    pos.y *= max(0.25, rnd.x);

    // Get terrain height at blade base world position
    let terrain_height = get_terrain_height(vec2<f32>(world_x, world_z));

    // Position blade in world space
    pos.x += world_x;
    pos.y += terrain_height;
    pos.z += world_z;

    // Transform to clip space
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);
    out.world_pos = pos;
    out.uv = blade_uv;

    // Flip normal toward camera for two-sided lighting
    let to_camera = normalize(params.camera_pos - pos);
    if (dot(to_camera, normal) < 0.0) {
        normal = -normal;
    }
    out.normal = normal;

    // Distance-based alpha fade
    let dist_to_camera = length(params.camera_pos - pos);
    let fade_start = params.foliage_info.z;
    let fade_end = params.foliage_info.w;
    out.alpha_fade = 1.0 - clamp((dist_to_camera - fade_start) / (fade_end - fade_start), 0.0, 1.0);

    return out;
}

// Debug mode: 0 = normal, 1 = bright green, 2 = UV colors, 3 = height as color
const DEBUG_MODE: i32 = 3;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // DEBUG: Return bright green to verify rendering
    if (DEBUG_MODE == 1) {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0);
    }

    // Debug: show UV coordinates as color
    if (DEBUG_MODE == 2) {
        return vec4<f32>(in.uv.x, in.uv.y, 0.0, 1.0);
    }

    // Debug: show world Y position as color (normalized to 0-500 range)
    if (DEBUG_MODE == 3) {
        let h = clamp(in.world_pos.y / 500.0, 0.0, 1.0);
        return vec4<f32>(h, 1.0 - h, 0.0, 1.0);
    }

    // Sample textures
    let albedo = textureSample(t_albedo, s_foliage, in.uv);
    let opacity = textureSample(t_opacity, s_foliage, in.uv).r;

    // Discard fully transparent pixels
    let final_alpha = opacity * in.alpha_fade;
    if (final_alpha < 0.1) {
        discard;
    }

    // Simple diffuse lighting
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let normal = normalize(in.normal);
    let n_dot_l = max(dot(normal, light_dir), 0.0);

    // Ambient + diffuse
    let ambient = 0.4;
    let diffuse = n_dot_l * 0.6;
    let lighting = ambient + diffuse;

    let result = albedo.rgb * lighting;

    return vec4<f32>(result, final_alpha);
}
"#;
