# Terrain Shader Parity Audit

This audit uses only fresh HLSL produced from the PC shader bundles with
`d3dasm.exe --emit hlsl` and direct inspection of the XTD/XTT asset bytes. The
legacy game source tree is deliberately not an input.

## Oracle

| Bundle | Decompiled programs | Renderer implementation |
|---|---:|---|
| `gputerrainxbox.bin` | 28 | `terrain_gpu.wesl` |
| `gputerraincomposite.bin` | 9 | `terrain_composite.wesl` |
| `terrainfoliage.bin` | 8 | `foliage.wesl` |
| `terrainheightfield.bin` | 13 | `terrain_heightfield.wesl` |
| `terrainroads.bin` | 3 | `terrain_roads.wesl` |

The captures used for this pass are in `target/shader-oracles/`. They were
regenerated from `../d3dasm/shaders-pc/terrain/` with the local
`../d3dasm/target/debug/d3dasm.exe`; checked-in approximations were not treated
as evidence.

## Terrain

- Geometry stays on the packed GPU path. Positions and normals use the PC
  shader's channel order and range/minimum reconstruction. Terrain height also
  applies the exact `g_yOffset` default, `1/2048`, before range scaling.
- A patch covers 16 cells and therefore reads 17×17 vertices. Boundary reads
  clamp to the last valid XTD vertex.
- XTD position/normal words are serialized in the PC texture's native source
  layout: source grid `(x, z)` is linear index `x * width + z`, or texture
  coordinate `(z, x)`. That source space is diagonally mirrored relative to
  the XTT material world used by the viewer. Before upload, viewer `(x, z)`
  therefore reads source `(z, x)`, and packed R/B plus the X/Z reconstruction
  constants are swapped with the texel axes. The shader still performs the
  oracle's `.yx` sample and `.zyx` decode unchanged. Repeating material UVs
  retain the independent `(Z, X)` convention used by the PC shader.
- XTD tessellation levels 0, 1, 2, and 3 select factors 16, 8, 4, and 2. Their
  patch coordinates use the same source-to-world transpose as the packed
  position atlas. Shared edges use the finer neighboring factor, and the fixed
  carrier mesh is quantized to those fractional-even domain locations. The
  main domain-shader variant's explicit global-X LOD is retained through the
  generated position mip.
- XTT linker axes are texture axes: XTT `grid_x` advances along world Z and
  XTT `grid_z` advances along world X. The compositor therefore places a
  linker at world `(grid_z, grid_x)`, or atlas slot `grid_x * 16 + grid_z`.
- Up to eight splat layers, static/dynamic alpha, decals, albedo, normals,
  colored specular, and the complete compositor mip chain are wired. Static
  alpha is sampled by generated terrain vertices and interpolated before the
  pixel shader's threshold, matching the PC domain/pixel split.
- The lit diagnostic path includes AO, the XTD light texture, SH fill,
  directional and local lighting, cascaded shadows, fog, and blackmap logic.
- Display mode 12 is the canonical viewer path. It samples the GPU-composited
  unique albedo atlas directly. Mode 0 remains an explicitly noncanonical lit
  diagnostic while its presentation is being tuned.

For Blood Gulch, every one of the 256 linkers requests the specular pass and
none requests a self-illumination or environment-mask pass. Those inactive
material variants are represented by the decompiled bundle but are not needed
for this map's canonical comparison.

## Foliage

- XTT foliage indices are decoded as big-endian `u32`. The upper 16 bits select
  the blade type; the lower 16 bits encode `blade_index * 10 + vertex`, with
  `0xffff` strip-restart entries.
- A foliage QN parent is an index into the XTD visual-chunk vector, not the XTT
  material-linker vector. Its stored `(grid_x, grid_z)` already selects the
  viewer's world chunk, so parent 96 remains chunk `(0, 6)`.
- The PC vertex shader derives one scalar random value from the local blade
  index and reuses it for X/Z jitter, rotation, and height variation. The WGSL
  now does the same instead of generating two independent values.
- The PC vertex shader loads runtime UV from structured-buffer offset 24 and
  passes it through. XML source V is therefore inverted while packing blade
  geometry, equivalent to the Xbox source shader's `1 - norm0.w` conversion.
- The shader's packed local 64×64 blade index uses the opposite X/Z order from
  the viewer. Only that local location, rotated blade geometry, and normal are
  transposed; the already-resolved parent chunk origin stays fixed. Terrain
  displacement samples the converted atlas at the oracle's `.yx` coordinate.
- PC foliage albedo and opacity DDS resources use DXGI BC7 (98/99). The local
  `../ensemble-formats` DDX decoder now identifies and decodes BC7 instead of
  treating it as BC3/DXT5.
- The pixel-lit variant supplies two-sided normals, directional/SH/local
  lighting, cascaded shadows, blackmap, fog, the 0.6666 alpha test, and the
  400–500 distance fade. The caster uses its separate alpha-tested path.
- Albedo and opacity textures have complete mip chains so minified blades do
  not sample an undefined mip range.

## Roads and heightfield variants

Road rendering is connected to the viewer and follows the freshly decompiled
terrain-conforming, TBN, material, lighting, shadow, and fog behavior. The
heightfield shader is retained as a reusable oracle-based implementation; XTT
terrain decals used by the canonical viewer are applied in the compositor.

## Runtime acceptance

`terrain_viewer` loading Blood Gulch is the canonical integration test. A valid
startup creates 256 compositor chunks, 4096 terrain patches, and 299 foliage
draws containing 21,294 blades without a wgpu validation error, and begins in
mode 12 with compositor debug mode 0. Blood Gulch contains no road data.

The deterministic acceptance capture is:

```text
cargo run --locked --bin terrain_viewer -- --capture-top-down target/terrain-captures/complete-xz-transform.png --capture-size 2048
```

It writes the canonical mode-12 image plus height, contour/alignment, decoded
XTT, foliage placement/material, and per-set oblique foliage companions.

The lower-right rock can be isolated with:

```text
cargo run --locked --bin terrain_viewer -- --capture-top-down target/terrain-captures/bottom-right-landmark-complete-xz.png --capture-size 768 --capture-center 880 880 --capture-span 224
```
