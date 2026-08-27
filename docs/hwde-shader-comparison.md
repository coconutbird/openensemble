# PC Terrain Shader Comparison

The comparison source is the HLSL freshly decompiled from the PC shader
bundles by `d3dasm.exe --emit hlsl`. Legacy source-derived descriptions are not
used because they do not describe the binaries being rendered here.

## Corrections made from the fresh decompile

| Area | Correct behavior |
|---|---|
| Packed terrain position | Native XTD source `(x, z)` is linear index `x * width + z`, but viewer/material world `(x, z)` corresponds to source `(z, x)`. The upload transposes texels, swaps packed R/B and the X/Z reconstruction constants, then leaves the shader's `.yx` sample and `.zyx` decode unchanged. |
| Terrain Y decode | Subtract the DXBC default `g_yOffset = 0.00048828125` before multiplying by the Y range. Foliage and roads intentionally do not use that terrain-only bias. |
| Terrain patches | 16 cells per patch and 17 vertices per side, with final-edge clamping. Levels 0/1/2/3 select factors 16/8/4/2, patch metadata follows the same source-to-world transpose, and shared edges take the finer neighbor. |
| Terrain position LOD | The main domain variant uses normalized global X as its explicit position-texture LOD; the renderer supplies the required generated mip. |
| XTT chunk axes | XTT `(grid_x, grid_z)` maps to world `(z, x)`; the atlas slot is `grid_x * 16 + grid_z`. |
| Repeating material UV | Uses the shader's independent `(Z, X)` convention; this is not the chunk-address convention. |
| Foliage random | One scalar `fract((fract(index * 0.0012385598) * 257 + 1)^2)` drives both jitter axes, rotation, and height. |
| Foliage UV | Stored U and V pass through directly; V is not inverted. |
| Foliage placement | In XTD source axes, `x = index / 64 + 0.5`, `z = index & 63`, followed by scalar jitter and the directly indexed XTD visual-parent origin. The complete source placement and blade geometry are then converted to viewer `(x, z) = (source z, source x)`. |
| Foliage indices | Big-endian packed `u32`: blade type in the upper half, blade/vertex index in the lower half, with strip restarts. |
| Foliage DDS | DXGI 98/99 is BC7, decoded through the local `../ensemble-formats` crate rather than the previous BC3 approximation. |

## GPU compositor

The compositor writes per-chunk albedo, normal, and colored-specular results
into matching atlases. It supports eight splat layers, alpha channels, decal
transforms, and every atlas mip. Hardware sRGB sampling/output performs the
linearization and encoding around the blend.

Blood Gulch's XTT bytes identify 256 linkers. Linker order increments XTT
`grid_x` first, which is world Z; compositor placement transposes that into the
world-facing atlas. All 256 request the specular pass; none requests a self or
environment-mask pass. Mode 12 reads the resulting unique albedo atlas and is
the canonical viewer comparison.

## Main terrain material

The lit diagnostic material implements the decompiled bundle's normal fade,
XTD light texture, AO ordering, SH fill, directional/local illumination,
specular response, cascaded VSM, radial/planar fog, and blackmap calculations.
It is retained for inspecting those channels, but mode 0 is not the canonical
visual target because its final presentation does not yet match mode 12.

## Foliage material

The viewer reconstructs active blades from the XTT strip data rather than
inventing placements. The pixel-lit path uses two-sided normals, directional
and local diffuse light, SH fill, cascaded shadows, blackmap, fog, the 0.6666
opacity threshold, and the 400–500 distance fade found in the fresh shaders.
Albedo and opacity uploads include mip chains to keep the alpha-tested sprites
stable under minification.

QN parent IDs index the XTD visual-chunk vector directly; they must never be
looked up in the XTT linker vector. The resulting chunk coordinates remain in
XTD source axes while the oracle placement and RNG are evaluated. The complete
placement is then mirrored into the viewer/material world together with the
terrain position field. For example, XTD source chunk `(0, 6)` becomes viewer
chunk `(6, 0)`.

## Canonical check

Run `cargo run --locked --bin terrain_viewer`. The viewer must start in mode 12
with compositor debug mode 0. Mode 13 is the orientation diagnostic: red marks
the `(0,0)` corner, blue `+X`, green `+Z`, and yellow the opposite corner.

For an automated oracle image, run:

```text
cargo run --locked --bin terrain_viewer -- --capture-top-down target/terrain-captures/complete-xz-transform.png --capture-size 2048
```

The companion `.height.png` and `.alignment.png` images expose terrain versus
texture registration, while `.foliage-map.png` plots the exact decoded blade
locations and each `.foliage-N-view.png` frames a deterministic dense region.

Use this focused capture for the lower-right rock diagnostic:

```text
cargo run --locked --bin terrain_viewer -- --capture-top-down target/terrain-captures/bottom-right-landmark-complete-xz.png --capture-size 768 --capture-center 880 880 --capture-span 224
```
