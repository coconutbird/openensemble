# Terrain Shader Parity Audit

Comparison of original Halo Wars HLSL, disassembled PC DXBC binaries, and our WGSL implementations.

## 1. `gputerraincomposite` — Texture Compositing

| Feature | Original HLSL | PC Binary | Our WGSL (`composite.wgsl`) |
|---|---|---|---|
| Per-layer UV scaling (`g_LayerData[].yz`) | ✅ | ✅ (2 shaders: VS+PS) | ✅ `texture_scales[]` |
| Alpha lookup from 3D texture | ✅ `tex3D(alphasSampler)` | ✅ | ✅ (2D atlas + RGBA channels — equivalent) |
| Multi-pass hardware alpha blend | ✅ (one draw per layer) | ✅ | ✅ (single-pass `mix()` — mathematically identical) |
| sRGB→linear conversion | ✅ `srgbToLinear()` | ✅ | ❌ Missing |
| Self-map / env-mask compositing | ✅ `CompsPixel_self`, `CompsPixel_envMask` | ✅ | ❌ Missing |
| Normal map compositing | ✅ `CompsPixel_normal` | ✅ | ❌ Missing |

**Status**: Core albedo compositing works. Missing sRGB conversion, self-map, env-mask, and normal compositing passes.

## 2. `gputerrainxbox` — Main Terrain Rendering

| Feature | Original HLSL | PC Binary | Our WGSL (`gpu_tess.wgsl` + `terrain.wgsl`) |
|---|---|---|---|
| Instanced patch vertex displacement | ✅ `vfetch` from packed textures | ✅ (28 shaders) | ✅ R10G10B10A2 decode |
| Packed position decode (posCompMin/Range) | ✅ | ✅ | ✅ |
| Packed normal decode | ✅ | ✅ | ✅ |
| Runtime texture splatting | ✅ | ✅ | ✅ (mode 9) |
| GPU-composited atlas sampling | ✅ `UniqueAlbedoSampler` | ✅ | ✅ (mode 12) |
| Normal map splatting (TBN) | ✅ `unpackDXNNormal`, TBN transform | ✅ | ✅ (partial) |
| AO texture | ✅ | ✅ | ✅ |
| Alpha/holes | ✅ | ✅ | ✅ |
| Directional shadow mapping (CSM) | ✅ `calcDirShadowFactor` | ✅ | ❌ Missing |
| SH fill lighting (ambient) | ✅ `computeSHFillLighting` | ✅ | ❌ Missing — hardcoded `0.4` |
| Local omni lights | ✅ `omniIlluminate` loop | ✅ | ❌ Missing |
| Specular lighting | ✅ `computeDirectionalLighting` | ✅ | ❌ Missing |
| Fog (radial + planar) | ✅ `computeFog` | ✅ | ❌ Missing |
| Blackmap | ✅ `computeBlackmap` | ✅ | ❌ Missing |
| Self-map / env-map | ✅ `UniqueEnvMaskSampler`, `EnvSampler` | ✅ | ❌ Missing |
| Bump power scaling | ✅ `gBumpPower` | ✅ | ✅ `bump_power` uniform |

**Status**: Geometry pipeline is solid. Fragment shader has basic splatting + AO + normal mapping but missing the entire lighting pipeline.

## 3. `terrainfoliage` — Grass/Foliage

| Feature | Original HLSL | PC Binary | Our WGSL (`foliage/shader.rs`) |
|---|---|---|---|
| Blade geometry from texture fetch | ✅ `tfetch2D` | ✅ (8 shaders) | ✅ |
| Deterministic random (`our_rand`) | ✅ `fmodp(xy/PI, 257)+1` | ✅ | ✅ |
| Random rotation per blade | ✅ `rotate_2d(rnd.y * 360)` | ✅ | ✅ |
| Random height scaling (0.25-1.0) | ✅ | ✅ | ✅ |
| Grid positioning + jitter | ✅ | ✅ | ✅ |
| Terrain height sampling | ✅ `tex2Dlod(vertSampler_pos)` | ✅ | ✅ |
| Gerstner wave animation | ✅ `wavePos()` — sin/cos wind | ✅ | ❌ Missing |
| Directional shadow mapping | ✅ `calcDirShadowFactorNoFilter` | ✅ | ❌ Missing |
| SH fill lighting | ✅ | ✅ | ❌ Missing |
| Local omni lights | ✅ | ✅ | ❌ Missing |
| Backside shadow scalar | ✅ `gBacksideShadowScalar` | ✅ | ❌ Missing |
| Two-sided normal flip | ✅ | ✅ | ✅ |
| Distance alpha fade | ✅ (fog-based) | ✅ | ✅ |
| Fog (radial + planar) | ✅ `computeFog` | ✅ | ❌ Missing |

**Status**: Core blade geometry, positioning, and randomization match well. Missing wind animation and full lighting pipeline.

## 4. `terrainheightfield` — Decal/Heightfield Rendering

| Feature | Original HLSL | PC Binary | Our WGSL |
|---|---|---|---|
| Debug heightfield visualization | ✅ | ✅ (13 shaders) | ❌ No shader |
| Heightfield occlusion pass | ✅ | ✅ | ❌ No shader |
| Quad-based patch rendering | ✅ `calcWeights`, `interpolate` | ✅ | ❌ No shader |
| Conform-to-terrain (heightfield sampling) | ✅ | ✅ | ❌ No shader |
| Dynamic alpha (terrain holes) | ✅ | ✅ | ❌ No shader |
| Full lit pass (diffuse+normal+spec+opacity) | ✅ | ✅ | ❌ No shader |
| Ribbon rendering | ✅ `vsRenderRibbonLit` | ✅ | ❌ No shader |

**Status**: Entirely unimplemented. Data types (`DecalTexture`, `DecalInstance`, `ChunkDecalData`) exist in Rust but no rendering shader.

## 5. `terrainroads` — Road Rendering

| Feature | Original HLSL | PC Binary | Our WGSL |
|---|---|---|---|
| Road vertex shader (terrain conform) | ✅ | ✅ (3 shaders) | ❌ No shader |
| TBN from terrain normal | ✅ `GiveTBNFromNormal` | ✅ | ❌ No shader |
| Road albedo/normal/specular sampling | ✅ | ✅ | ❌ No shader |
| Full directional + local lighting | ✅ | ✅ | ❌ No shader |
| Shadow mapping | ✅ | ✅ | ❌ No shader |
| Fog | ✅ | ✅ | ❌ No shader |

**Status**: Entirely unimplemented. Roads are terrain-conforming geometry with own textures and full lighting.

## Summary

| Shader | Geometry | Texturing | Lighting | Shadows | Fog | Overall |
|---|---|---|---|---|---|---|
| **gputerraincomposite** | ✅ | 🟡 | N/A | N/A | N/A | ~70% |
| **gputerrainxbox** | ✅ | ✅ | ❌ | ❌ | ❌ | ~50% |
| **terrainfoliage** | ✅ | ✅ | ❌ | ❌ | ❌ | ~45% |
| **terrainheightfield** | ❌ | ❌ | ❌ | ❌ | ❌ | 0% |
| **terrainroads** | ❌ | ❌ | ❌ | ❌ | ❌ | 0% |

## Priority Gaps

1. **Shared lighting pipeline** — SH fill lighting, directional shadows (CSM), local omni lights, specular, fog, blackmap. Needed by ALL shaders.
2. **terrainheightfield** — Decal/patch rendering with terrain conformance. Data structures exist but no shader.
3. **terrainroads** — Road rendering with terrain conformance. Similar to heightfield but simpler geometry.
4. **Wind animation** — Gerstner wave for foliage.
5. **Compositor completeness** — sRGB, self-map, env-mask, normal compositing.
