# HWDE Shader Comparison — Differences & Status

Comprehensive comparison of Halo Wars: Definitive Edition (HWDE) disassembled
D3D11 shaders (`gputerrainxbox.bin`, `gputerraincomposite.bin`) against our
WGSL reimplementation.

Source disassembly: `d3dasm` on the PC `.bin` shader bundles from HWDE.

## ✅ Already Correct

| Feature | Notes |
|---------|-------|
| Fog base (exp vs exp2) | DXBC `exp` = 2^x, matches our `exp2()` |
| TBN construction | Matches `GiveTBNFromNormal` exactly |
| SH fill lighting | Matches exactly |
| VSM shadows | Same 3×3 kernel, Chebyshev, smoothstep(0.43,1.0) |
| Position/Normal decode | `.zyxw`/`.zyx` swizzles are float-texture channel order; we decode from packed R32Uint — different path, same result |
| Tiling UV swap | Fixed: now `(Z, X)` matching HWDE's `gPos = (Z, X)` |

## ✅ Implemented

### 1. Normal Map Distance Fadeout
**Status:** DONE
**Files:** `terrain_gpu.wesl` (VS + PS), `uniforms.rs`

HWDE VS (lines 208-215) computes a per-vertex fadeout:
```
dist = length(world_pos - camera)
fadeout = 1.0 - saturate((dist - min) / (max - min + bias))
```
PS blends bumped normal → flat `(0,0,1)` via `lerp(bump, (0,0,1), 1-fadeout)`.

Added `fadeout_params` to `LightingParams` uniform. VS computes fadeout factor,
PS applies it to blend the bump-mapped normal toward flat up-vector at distance.

### 2. Light Texture
**Status:** DONE
**Files:** `terrain_gpu.wesl` (VS + PS), `pipelines.rs` (binding 21)

HWDE VS samples `gVertSampler_light_Texture` (t2) and passes RGB as `v8.xyz`.
PS adds `lightmap * 2.0` to the diffuse lighting sum.

Added R8Unorm light texture from XTD `decode_lighting()` at binding 21 (vertex
stage). VS samples it and passes to PS as `light_color`. PS integrates as
`in.light_color * 2.0` in the lighting combination. Falls back to a mid-gray
RGBA placeholder when lighting data is unavailable.

### 3. AO Integration Order
**Status:** DONE
**Files:** `terrain_gpu.wesl` (PS)

HWDE formula (PS lines 493-507, after shadow + lightmap):
```
light = dir_diffuse * shadow + lightmap * 2.0
ao_blend = mix(light, light * ao, ao_intensity)
final = (sh_ambient * ao + ao_blend) * albedo
```

Changed from applying AO only to ambient/specular to the HWDE approach:
AO is blended into the full diffuse lighting result using `ao_intensity`.

### 6. Planar Fog (Volumetric)
**Status:** DONE
**Files:** `shared/fog.wesl`, all calling shaders

HWDE VS (lines 180-199) traces the view ray through the fog volume:
1. Computes vertical direction from vertex to camera
2. Finds effective distance of view ray through fog region (ray-plane intersection)
3. Applies squared exponential decay: `2^(-(eff_dist² × density²))`

Replaced simple height-based fog with volumetric ray-plane intersection.
Updated `compute_planar_fog_density()` signature to take `world_pos` and
`camera_pos` (was just `world_y`). Updated all 4 callers: `terrain_gpu.wesl`,
`foliage.wesl`, `terrain_heightfield.wesl`, `terrain_roads.wesl`.

### 7. Blackmap UV Coordinates
**Status:** DONE
**Files:** `terrain_gpu.wesl` (PS), `uniforms.rs`

HWDE samples blackmap at world-position-derived coords with separate X/Z scales
(cb4[33-34]):
```
blackmap_u = world_x * scale_x
blackmap_v = world_z * scale_z
```

Added `blackmap_uv_scales` to `LightingParams`. PS now computes blackmap UVs
from world position × per-axis scales instead of using terrain UV directly.

### 8. Alpha Gutter Offset
**Status:** DONE (no change needed)

HWDE uses `0.00003` for alpha texture gutter inset — calibrated for their
specific 3D atlas layout. Our `0.5/64.0 = 0.0078` is the correct half-pixel
offset for our 2D texture array with 64×64 per-layer resolution. Both serve
the same purpose (half-pixel inset to avoid bleeding).

## ✅ Previously Deferred (Now Implemented)

### 4. Compositor HDR / Self-Illumination (Shader #2)
**Status:** DONE (structural parity — HDR scale data is null/zero in PC release)

The HWDE shader supports a gamma-correct self-illumination path:
```
linear = pow(abs(srgb_color), 2.2)
output = linear.rgb * linear.a * 32.0 * hdr_scale_per_layer
```

IDA analysis confirmed `g_pHDRScalePerLayerType` is null in the PC release,
making this path effectively dead code. The `hdr_scale` field is carried in the
`DecalLayerInfo` struct for structural parity but defaults to 1.0.

**Files:** `crates/render/src/terrain/shaders/terrain_composite.wesl`

### 5. Compositor Decals (Shaders #4-#7)
**Status:** DONE

Full decal compositing with rotation/transform, matching HWDE Shader #6/#7:
- `sincos(angle)` rotation matrix applied to swapped `.yx` UVs
- Per-decal UV scale (reciprocal), center offset, alpha channel selection
- Decal alpha atlas (64×64 per chunk, RGBA for up to 4 decal layers)
- Decal instance data buffer (rotation, center_u, center_v, hdr_scale)
- Decal UV scales buffer (per-instance u_scale, v_scale)
- Chunk decal layers buffer (per-chunk decal instance IDs)
- Bounds checking (UV clamped to [0,1] to avoid bleed)

**Files:**
- `crates/render/src/terrain/shaders/terrain_composite.wesl` — decal rotation & compositing
- `crates/render/src/terrain/compositing.rs` — bind group layout entries 8-11, `num_decal_layers`
- `src/bin/terrain_viewer/resources/buffers.rs` — decal instance/scale/layer buffers
- `src/bin/terrain_viewer/resources/textures.rs` — decal alpha atlas creation
- `src/bin/terrain_viewer/resources/pipelines.rs` — wiring decal resources to compositor
- `src/bin/terrain_viewer/viewer/rendering.rs` — passing decal layer counts at composite time
