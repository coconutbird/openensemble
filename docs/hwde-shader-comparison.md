# PC terrain shader comparison

This document records how the terrain renderer follows the PC terrain shaders of Halo Wars DE.
The reference is HLSL that `d3dasm.exe --emit hlsl` decompiles from the PC shader bundles. The
legacy game source tree is not a reference.

## Shader bundles

| Bundle | Decompiled programs | Renderer shader |
|---|---:|---|
| `gputerrainxbox.bin` | 28 | `crates/render/src/terrain/shaders/terrain_gpu.wesl` |
| `gputerraincomposite.bin` | 9 | `crates/render/src/terrain/shaders/terrain_composite.wesl` |
| `terrainfoliage.bin` | 8 | `crates/render/src/terrain/shaders/foliage.wesl` |
| `terrainheightfield.bin` | 13 | `crates/render/src/terrain/shaders/terrain_heightfield.wesl` |
| `terrainroads.bin` | 3 | `crates/render/src/terrain/shaders/terrain_roads.wesl` |

## Terrain geometry

| Area | Behavior |
|---|---|
| Packed terrain position | Native XTD source `(x, z)` is linear index `x * width + z`. Viewer world `(x, z)` is source `(z, x)`. The upload transposes the texels and swaps packed R/B and the X/Z reconstruction constants. The shader keeps the `.yx` sample and the `.zyx` decode. |
| Terrain Y decode | Subtract the default `g_yOffset = 1/2048` before the multiplication by the Y range. Foliage and roads do not use this bias. |
| Terrain patches | A patch has 16 cells and 17 vertices on each side. Reads at the last edge clamp. Levels 0/1/2/3 select factors 16/8/4/2. Shared edges use the finer neighbor. |
| Tessellation | `wgpu` has no tessellation stages. A fixed indexed carrier mesh, quantized to the fractional-even domain locations, replaces the hull and domain stages. |
| Position LOD | The main domain variant uses normalized global X as the LOD of the position texture. The renderer supplies the generated mip. |
| Terrain alpha | The generated terrain vertices sample the static XTD alpha. The pixel shader combines it with the dynamic-alpha bit and applies the `0.66666` threshold. |
| XTT chunk axes | XTT `(grid_x, grid_z)` is world `(z, x)`. The atlas slot is `grid_x * 16 + grid_z`. |
| Repeating material UV | The shader uses an independent `(Z, X)` convention. This is not the chunk address convention. |

## GPU compositor

`crates/render/src/terrain/compositing.rs` writes the albedo, normal and colored specular of each
chunk into matching atlases. It supports eight splat layers, alpha channels, decal transforms and
all atlas mips. Hardware sRGB sampling and output do the linearization and the encoding.

The chunk alpha and decal masks use 64-pixel tiles, not texture array layers. Thus maps with more
than 256 chunks do not exceed the array layer limit. The atlas uses the largest power-of-two chunk
size, up to 512, that fits the 2D texture limit of the device. For example, Fort Deen (20×20
chunks) uses 256 pixels for each chunk in a 5120×5120 atlas on a device with an 8192 limit.

The compositor rebuilds only dirty chunks. It does not use variable resolution for each chunk; the
mip chain gives the distance falloff.

## Main terrain material

The retail-lit material implements the normal fade, the XTD light texture, the AO order, SH fill,
directional and local lights, specular, cascaded VSM shadows, radial and planar fog, and the
blackmap. The compositor implements the specular pass only. The self-illumination and
environment-mask passes are not implemented. Blood Gulch does not use them: all 256 linkers ask
for the specular pass only.

## Foliage

| Area | Behavior |
|---|---|
| Indices | Big-endian `u32`. The upper 16 bits select the blade type. The lower 16 bits encode `blade_index * 10 + vertex`. `0xffff` is a strip restart. |
| Parent chunk | A QN parent is an index into the XTD visual chunk vector, not the XTT linker vector. Its `(grid_x, grid_z)` is the viewer chunk. For example, parent 96 is chunk `(0, 6)`. |
| Placement | The local grid is `x = index / 64 + 0.5`, `z = index & 63`, then the scalar jitter. Only the local offset, the blade geometry and the normal are transposed to viewer `(x, z)`. The parent chunk origin does not move. |
| Random value | One scalar `fract((fract(index * 0.0012385598) * 257 + 1)^2)` drives both jitter axes, the rotation and the height. |
| UV | The PC vertex shader reads the runtime UV at structured-buffer offset 24. The packer inverts the XML source V. This is the same as `1 - norm0.w` in the Xbox shader. |
| Textures | DXGI formats 98/99 are BC7. The `ensemble-formats` DDX decoder decodes them. The uploads include full mip chains. |
| Lighting | The pixel-lit variant uses two-sided normals, directional, SH and local light, cascaded shadows, blackmap, fog, the `0.6666` alpha test and the 400–500 distance fade. The shadow caster uses its own alpha-tested path. |

## Roads and heightfield patches

The viewer draws roads with the terrain-conforming, TBN, material, light, shadow and fog behavior
of `terrainroads`. `TerrainPatchRenderer` (`crates/render/src/terrain/patch.rs`) implements the
`terrainheightfield` patches. The compositor applies the XTT terrain decals. Blood Gulch has no
road data.

## Viewer check

Run `cargo run --locked --bin terrain_viewer`. The viewer starts in display mode 0 (`Retail Lit`)
with compositor debug mode 0. Mode 12 shows the composited albedo atlas. Mode 13 shows the
orientation: red marks the `(0, 0)` corner, blue `+X`, green `+Z` and yellow the opposite corner.

On Blood Gulch, a correct start creates 256 compositor chunks, 4096 terrain patches and 299
foliage draws with 21,294 blades, without a `wgpu` validation error.

To write a top-down capture, run this command:

```text
cargo run --locked --bin terrain_viewer -- --capture-top-down target/terrain-captures/complete-xz-transform.png --capture-size 2048
```

The capture also writes companion images: `.height.png` and `.alignment.png` show the terrain
against the texture registration, `.foliage-map.png` plots the decoded blade locations, and each
`.foliage-N-view.png` frames a dense foliage region.

To capture the rock in the lower-right corner, run this command:

```text
cargo run --locked --bin terrain_viewer -- --capture-top-down target/terrain-captures/bottom-right-landmark-complete-xz.png --capture-size 768 --capture-center 880 880 --capture-span 224
```
