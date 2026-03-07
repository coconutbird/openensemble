# GPU Terrain Texture Compositing Implementation Plan

## Overview

Replace the current per-pixel texture splatting (done every frame in the fragment shader) with GPU-based render-to-texture compositing that matches retail Halo Wars DE behavior:

- **Current**: Fragment shader samples 4+ tiled textures + alpha masks per pixel, every frame
- **Target**: Pre-composite unique textures per chunk to render targets ONCE (or on LOD change), then sample single composited texture during terrain rendering

### Goals
1. Eliminate redundant per-pixel splatting every frame
2. Match retail quality (512x512 unique texture per chunk for albedo, normal, specular)
3. Implement LOD system for camera distance-based resolution
4. Enable future decal layer support

### Scope Boundaries
- **In scope**: Albedo compositing, normal map compositing, chunk management, LOD system
- **Out of scope**: Decal layers, specular maps, env masks (future work)

---

## Prerequisites

1. **Existing infrastructure** (already present):
   - `ChunkSplatData` struct with per-chunk layer IDs and alpha maps
   - `create_alpha_atlas()` - packs alpha maps into 1024x1024 atlas
   - `create_composited_albedo_atlas()` - CPU compositing (to be replaced)
   - Terrain texture array with mipmaps
   - Normal map texture array

2. **wgpu requirements**:
   - Render target textures with `RENDER_ATTACHMENT` usage
   - Separate compositing render pass
   - Texture atlas for composited chunks

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                     GPU Compositing Pipeline                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────────┐    ┌──────────────────┐    ┌──────────────────┐   │
│  │ Terrain      │    │ Compositing Pass │    │ Terrain Render   │   │
│  │ Textures     │───▶│ (per dirty chunk)│───▶│ Pass             │   │
│  │ + Alpha Maps │    │                  │    │                  │   │
│  └──────────────┘    └──────────────────┘    └──────────────────┘   │
│                              │                        │              │
│                              ▼                        ▼              │
│                    ┌──────────────────┐    ┌──────────────────┐     │
│                    │ Composited Atlas │    │ Final Frame      │     │
│                    │ (8K x 4K atlas)  │    │                  │     │
│                    └──────────────────┘    └──────────────────┘     │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Load Time**: Parse XTT, extract chunk layer IDs + alpha maps
2. **GPU Init**: Create composited texture atlas as render target
3. **First Frame**: Composite all 256 chunks (or visible chunks)
4. **Runtime**:
   - Check camera distance → determine LOD per chunk
   - If LOD changed → re-composite chunk at new resolution
   - Terrain render pass samples composited atlas

---

## Implementation Steps

### Phase 1: Core Compositing Infrastructure (Medium complexity)

#### Step 1.1: Add Compositing Module to render crate
Create `crates/render/src/terrain/compositing.rs`:

```rust
/// Configuration for terrain texture compositing.
pub struct CompositingConfig {
    /// Size of each chunk's composited texture (512 = retail Halo Wars)
    pub chunk_texture_size: u32,
    /// Number of chunks in X direction (typically 16)
    pub chunks_x: u32,
    /// Number of chunks in Z direction (typically 16)
    pub chunks_z: u32,
    /// Atlas width (chunks_x * chunk_texture_size)
    pub atlas_width: u32,
    /// Atlas height (chunks_z * chunk_texture_size)
    pub atlas_height: u32,
}

impl Default for CompositingConfig {
    fn default() -> Self {
        Self {
            chunk_texture_size: 512,
            chunks_x: 16,
            chunks_z: 16,
            atlas_width: 8192,  // 16 * 512
            atlas_height: 8192,
        }
    }
}
```

**Files to create:**
- `crates/render/src/terrain/compositing.rs` (new)

**Files to modify:**
- `crates/render/src/terrain/mod.rs` (add `mod compositing; pub use compositing::*;`)

#### Step 1.2: Create Compositing Shader
Create `crates/render/src/terrain/shaders/composite.wgsl`:

```wgsl
// Terrain texture compositing shader
// Renders splat layers to a render target for one chunk

// Input: chunk index, layer IDs, alpha maps
// Output: composited RGBA to render target

struct CompositeParams {
    chunk_index: u32,      // Which chunk we're compositing
    chunk_offset: vec2<f32>, // UV offset in output atlas
    chunk_size: vec2<f32>,   // UV size in output atlas
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> params: CompositeParams;

// Terrain texture array (same as main shader)
@group(0) @binding(1)
var t_terrain_array: texture_2d_array<f32>;

// Alpha atlas
@group(0) @binding(2)
var t_alpha_atlas: texture_2d<f32>;

// Per-chunk layer data
@group(0) @binding(3)
var<storage, read> chunk_layers: array<u32>;

// Texture UV scales
@group(0) @binding(4)
var<storage, read> texture_scales: array<vec2<f32>>;

@group(0) @binding(5)
var s_terrain: sampler;

// Vertex shader: fullscreen triangle for the chunk region
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,  // 0-1 within chunk
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle vertices
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = (positions[vertex_index] + 1.0) * 0.5;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let chunk_idx = params.chunk_index;
    let in_chunk_uv = in.uv;

    // Get layer indices
    let layer_base = chunk_idx * 8u;
    let layer0 = chunk_layers[layer_base];
    let layer1 = chunk_layers[layer_base + 1u];
    let layer2 = chunk_layers[layer_base + 2u];
    let layer3 = chunk_layers[layer_base + 3u];

    // Sample alpha (UV within chunk maps to alpha atlas region)
    let chunk_x = chunk_idx % 16u;
    let chunk_y = chunk_idx / 16u;
    let alpha_uv = (vec2<f32>(f32(chunk_x), f32(chunk_y)) + in_chunk_uv) / 16.0;
    let alphas = textureSample(t_alpha_atlas, s_terrain, alpha_uv);

    // Calculate tiled UVs for each layer
    let base_uv = (vec2<f32>(f32(chunk_x), f32(chunk_y)) + in_chunk_uv);
    let uv0 = base_uv * texture_scales[layer0];
    let uv1 = base_uv * texture_scales[layer1];
    let uv2 = base_uv * texture_scales[layer2];
    let uv3 = base_uv * texture_scales[layer3];

    // Blend layers
    var color = textureSample(t_terrain_array, s_terrain, uv0, layer0).rgb;

    if (layer1 > 0u && alphas.r > 0.0) {
        let c1 = textureSample(t_terrain_array, s_terrain, uv1, layer1).rgb;
        color = mix(color, c1, alphas.r);
    }
    if (layer2 > 0u && alphas.g > 0.0) {
        let c2 = textureSample(t_terrain_array, s_terrain, uv2, layer2).rgb;
        color = mix(color, c2, alphas.g);
    }
    if (layer3 > 0u && alphas.b > 0.0) {
        let c3 = textureSample(t_terrain_array, s_terrain, uv3, layer3).rgb;
        color = mix(color, c3, alphas.b);
    }

    return vec4<f32>(color, 1.0);
}
```

**Files to create:**
- `crates/render/src/terrain/shaders/composite.wgsl` (new)

**Files to modify:**
- `crates/render/src/terrain/shaders.rs` (add `pub const COMPOSITE_SHADER: &str = include_str!("shaders/composite.wgsl");`)


#### Step 1.3: Create CompositorResources Struct

Add to `crates/render/src/terrain/compositing.rs`:

```rust
/// GPU resources for terrain texture compositing.
pub struct CompositorResources {
    /// Composited albedo atlas (render target)
    pub albedo_atlas: wgpu::Texture,
    pub albedo_atlas_view: wgpu::TextureView,

    /// Composited normal atlas (render target)
    pub normal_atlas: wgpu::Texture,
    pub normal_atlas_view: wgpu::TextureView,

    /// Compositing render pipeline
    pub pipeline: wgpu::RenderPipeline,

    /// Bind group for compositing shader
    pub bind_group: wgpu::BindGroup,

    /// Per-chunk params buffer (updated for each chunk composite)
    pub params_buffer: wgpu::Buffer,

    /// Configuration
    pub config: CompositingConfig,

    /// Dirty flags per chunk (needs re-composite)
    pub dirty_chunks: Vec<bool>,

    /// Current LOD level per chunk (0 = highest detail)
    pub chunk_lod: Vec<u8>,
}
```

#### Step 1.4: Implement CompositorResources::new()

```rust
impl CompositorResources {
    pub fn new(
        device: &wgpu::Device,
        config: CompositingConfig,
        terrain_array_view: &wgpu::TextureView,
        alpha_atlas_view: &wgpu::TextureView,
        chunk_layers_buffer: &wgpu::Buffer,
        texture_scales_buffer: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
    ) -> Self {
        // Create albedo atlas as render target
        let albedo_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Composited Albedo Atlas"),
            size: wgpu::Extent3d {
                width: config.atlas_width,
                height: config.atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1, // No mipmaps on atlas (chunks have internal detail)
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                 | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        // ... similar for normal_atlas with Rgba8Unorm (linear data)

        // Create compositing pipeline
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });

        // ... bind group layout, pipeline creation

        let total_chunks = (config.chunks_x * config.chunks_z) as usize;

        Self {
            albedo_atlas,
            albedo_atlas_view: albedo_atlas.create_view(&Default::default()),
            normal_atlas: todo!(),
            normal_atlas_view: todo!(),
            pipeline,
            bind_group,
            params_buffer,
            config,
            dirty_chunks: vec![true; total_chunks], // All dirty initially
            chunk_lod: vec![0; total_chunks],
        }
    }
}
```

---

### Phase 2: Compositing Pass Integration (Medium complexity)

#### Step 2.1: Add composite_chunk() Method

```rust
impl CompositorResources {
    /// Composite a single chunk to the atlas at its assigned region.
    pub fn composite_chunk(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        chunk_index: u32,
    ) {
        let chunk_x = chunk_index % self.config.chunks_x;
        let chunk_z = chunk_index / self.config.chunks_x;

        // Calculate viewport for this chunk within the atlas
        let chunk_size = self.config.chunk_texture_size;
        let viewport_x = chunk_x * chunk_size;
        let viewport_z = chunk_z * chunk_size;

        // Update params buffer with chunk index
        // encoder.copy_buffer_to_buffer(...) or queue.write_buffer()

        // Create render pass targeting the chunk region
        let chunk_view = self.albedo_atlas.create_view(&wgpu::TextureViewDescriptor {
            // Use base_mip_level and mip_level_count for single slice
            ..Default::default()
        });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Composite Chunk Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &chunk_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        render_pass.set_viewport(
            viewport_x as f32, viewport_z as f32,
            chunk_size as f32, chunk_size as f32,
            0.0, 1.0
        );
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1); // Fullscreen triangle
    }

    /// Composite all dirty chunks.
    pub fn composite_dirty_chunks(&mut self, encoder: &mut wgpu::CommandEncoder) {
        for chunk_idx in 0..(self.config.chunks_x * self.config.chunks_z) {
            if self.dirty_chunks[chunk_idx as usize] {
                self.composite_chunk(encoder, chunk_idx);
                self.dirty_chunks[chunk_idx as usize] = false;
            }
        }
    }
}
```

#### Step 2.2: Integrate into TerrainViewer

Modify `src/bin/terrain_viewer.rs`:

1. Add `compositor: Option<CompositorResources>` to `TerrainViewer` struct
2. Initialize compositor in `create_gpu_resources_from_data()` and `create_gpu_tessellation_resources()`
3. Call `compositor.composite_dirty_chunks()` before terrain render pass
4. Modify terrain shader to sample from composited atlas instead of per-pixel splatting

```rust
// In Application3D::render_3d():
fn render_3d(&mut self, ctx: &mut RenderContext) {
    // ... existing setup ...

    // Composite dirty chunks (typically only on first frame or LOD change)
    if let Some(compositor) = &mut self.compositor {
        compositor.composite_dirty_chunks(ctx.encoder);
    }

    // Render terrain using composited atlas
    // ... existing terrain render pass ...
}
```

---

### Phase 3: LOD System (High complexity)

#### Step 3.1: Add LOD Configuration

```rust
/// LOD level configuration.
pub struct LodConfig {
    /// Distance thresholds for each LOD level (in world units)
    /// LOD 0 = closest (highest detail), LOD 3 = farthest (lowest detail)
    pub distance_thresholds: [f32; 4],

    /// Texture size multiplier for each LOD level
    /// LOD 0 = 1.0 (512), LOD 1 = 0.5 (256), LOD 2 = 0.25 (128), LOD 3 = 0.125 (64)
    pub size_multipliers: [f32; 4],
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            distance_thresholds: [100.0, 300.0, 600.0, f32::MAX],
            size_multipliers: [1.0, 0.5, 0.25, 0.125],
        }
    }
}
```

#### Step 3.2: Calculate LOD Per Chunk

```rust
impl CompositorResources {
    /// Calculate LOD level for each chunk based on camera distance.
    /// Returns true if any chunk's LOD changed (needs re-composite).
    pub fn update_lod(
        &mut self,
        camera_pos: Vec3,
        chunk_centers: &[[f32; 3]; 256], // Pre-calculated chunk centers
        lod_config: &LodConfig,
    ) -> bool {
        let mut any_changed = false;

        for chunk_idx in 0..256 {
            let chunk_center = Vec3::from(chunk_centers[chunk_idx]);
            let distance = (camera_pos - chunk_center).length();

            let new_lod = lod_config.distance_thresholds
                .iter()
                .position(|&threshold| distance < threshold)
                .unwrap_or(3) as u8;

            if self.chunk_lod[chunk_idx] != new_lod {
                self.chunk_lod[chunk_idx] = new_lod;
                self.dirty_chunks[chunk_idx] = true;
                any_changed = true;
            }
        }

        any_changed
    }
}
```

#### Step 3.3: Variable Resolution Compositing

For LOD, we need per-chunk render targets or a more sophisticated atlas packing:

**Option A (Simpler)**: Fixed atlas, sample with implicit LOD
- Composite at full resolution always
- Let GPU mipmaps handle distance falloff
- Cheaper but less optimal

**Option B (Halo Wars approach)**: Variable resolution per chunk
- Maintain separate render targets per LOD level
- Atlas packing becomes more complex
- Better quality/performance

**Recommendation**: Start with Option A, iterate to Option B if needed.

---

### Phase 4: Shader Modifications (Medium complexity)

#### Step 4.1: Create Composited Terrain Shader

Create `crates/render/src/terrain/shaders/terrain_composited.wgsl`:

This is a simplified version of the current terrain shader that samples the composited atlas instead of doing per-pixel splatting:

```wgsl
// Simplified terrain shader using pre-composited textures

@group(1) @binding(0)
var t_composited_albedo: texture_2d<f32>;

@group(1) @binding(1)
var t_composited_normal: texture_2d<f32>;

@group(1) @binding(2)
var s_terrain: sampler;

// ... camera uniform, params ...

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Simple UV-based sampling - no per-pixel splatting!
    let albedo = textureSample(t_composited_albedo, s_terrain, in.uv).rgb;
    let normal_sample = textureSample(t_composited_normal, s_terrain, in.uv).rg;

    // Reconstruct normal from RG
    let tangent_normal = vec3<f32>(
        normal_sample.r * 2.0 - 1.0,
        normal_sample.g * 2.0 - 1.0,
        sqrt(max(0.0, 1.0 - dot(normal_sample, normal_sample)))
    );

    // ... TBN, lighting ...

    return vec4<f32>(albedo * lighting, 1.0);
}
```

#### Step 4.2: Add Shader Selection Toggle

Allow runtime switching between splatting and composited modes for comparison:

```rust
enum TexturingMode {
    /// Per-pixel splatting (current implementation)
    RuntimeSplatting,
    /// GPU-composited unique textures (new)
    GpuComposited,
}
```

---

## File Changes Summary

### New Files

| File | Purpose |
|------|---------|
| `crates/render/src/terrain/compositing.rs` | CompositorResources, LOD logic, compositing pipeline |
| `crates/render/src/terrain/shaders/composite.wgsl` | Compositing shader (splat → render target) |
| `crates/render/src/terrain/shaders/terrain_composited.wgsl` | Simplified terrain shader using composited atlas |

### Modified Files

| File | Changes |
|------|---------|
| `crates/render/src/terrain/mod.rs` | Add `mod compositing; pub use compositing::*;` |
| `crates/render/src/terrain/shaders.rs` | Add `COMPOSITE_SHADER`, `TERRAIN_COMPOSITED_SHADER` constants |
| `src/bin/terrain_viewer.rs` | Add `compositor: Option<CompositorResources>`, integrate compositing pass, add UI toggle |

---

## Testing Strategy

### Unit Tests

1. **Compositing config**: Verify atlas dimensions calculated correctly
2. **LOD calculation**: Test distance thresholds return correct LOD levels
3. **Dirty flag management**: Verify chunks marked dirty on LOD change

### Integration Tests

1. **Single chunk compositing**: Composite one chunk, verify output matches CPU reference
2. **Full terrain compositing**: Composite all 256 chunks, compare with current CPU implementation
3. **LOD transitions**: Move camera, verify chunks re-composite at correct distances

### Manual Testing

1. Load blood_gulch.xtd, enable GPU compositing mode
2. Compare visual output with current runtime splatting (debug mode toggle)
3. Profile frame time improvement
4. Test with different terrain files to verify generalization

---

## Rollback Plan

1. **Code revert**: Git revert the compositing commits
2. **Feature flag**: Keep `TexturingMode` toggle to switch back to runtime splatting
3. **No data migration**: No persistent data changes, purely runtime

---

## Estimated Effort

| Phase | Complexity | Estimate |
|-------|------------|----------|
| Phase 1: Core Infrastructure | Medium | 4-6 hours |
| Phase 2: Pass Integration | Medium | 3-4 hours |
| Phase 3: LOD System | High | 4-6 hours |
| Phase 4: Shader Modifications | Medium | 2-3 hours |
| Testing & Debugging | Medium | 4-6 hours |
| **Total** | | **17-25 hours** |

---

## Future Enhancements (Out of Scope)

1. **Decal compositing**: Add decal layer pass after splat compositing
2. **Specular atlas**: Composite specular/roughness maps
3. **Streaming**: Load/unload chunk textures based on visibility
4. **Async compositing**: Composite chunks over multiple frames to avoid hitches
