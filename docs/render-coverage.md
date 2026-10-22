# Presentation and application coverage

This document compares the presentation subsystems of the retail engine (`xgameFinal.exe`, Halo
Wars DE) with the crates `render`, `net` and `motion` and the root executables. The engine
evidence is a function or a string address in the shared IDA database. "Missing" means that no code
in this repository implements the item.

For the simulation, see [simulation-coverage.md](simulation-coverage.md). For the terrain shaders,
see [hwde-shader-comparison.md](hwde-shader-comparison.md).

## Implemented

| Area | Engine evidence | Our code |
|---|---|---|
| UGX models: skinning, material maps, blend modes, SH fill | `BUGXGeomRender*`, `BUGXGeomUberSectionRenderer` | `crates/render/src/ugx.rs`, `crates/render/src/ugx/renderer/` |
| Animation sampling, blends, attachments | Granny runtime (`granny_model_instance.cpp` @ `0x140f1c058`), `BGrannyAnimation` | `crates/motion/src/`, `crates/render/src/ugx/unit/` |
| Inverse kinematics (CCD, ground IK) | Granny IK in the Granny runtime | `crates/render/src/ugx/model/ik.rs`, `crates/render/src/ugx/unit/ik.rs` |
| Particles (PFX) with beams and trails | `*.pfx` @ `0x1411e218c` (`sub_1407955A0`) | `crates/render/src/particle.rs`, `crates/render/src/particle/runtime.rs` |
| Terrain (XTD/XTT), GPU compositor, foliage, roads | `BTerrainIOLoader*`, `BTerrainVisual*` | `crates/render/src/terrain/`, `crates/render/src/world_view/` |
| Terrain effects (`.tfx`) and `impacteffects.xml` | `impacteffects.xml` @ `0x1411a3388` | `crates/render/src/terrain_effect.rs`, `crates/render/src/terrain_effect/` |
| Impact decals on terrain | `BTerrainImpactDecalManager_createImpactDecal` @ `0x14041AB80` | `crates/render/src/ugx/scene/impact_decals.rs` |
| Light effects (`.lgt`) and local lights | `uiGlobalLights.gls` @ `0x1411b7a60` | `crates/render/src/light_effect.rs`, `crates/render/src/lighting.rs` |
| Buffered light volume | `mLightBuffered` light path | `crates/render/src/light_volume.rs` |
| Shadows: cascaded VSM, local spot and omni maps | terrain shader bundles | `crates/render/src/local_shadow.rs`, `crates/render/src/world_view/shadow.rs` |
| Scenario light sets (GLS/FLS) | `uiGlobalLights.fls` @ `0x1411b7af8` | `pipeline::hw1::LightSetData` in `ensemble-assets`, used by `crates/render/src/world_view/frame.rs` |
| HDR target, tone map, distortion | `postprocess` pass of the PC renderer | `crates/render/src/postprocess.rs` |
| HDR environment cubemap, sky render phase | `BUGXGeomUberSectionRenderer__initEffect` @ `0x1406c93e0` ("Found sky material") | `crates/render/src/environment.rs`, `RenderPhase::Sky` in `crates/render/src/world.rs` |
| Fog of war on terrain (blackmap) | `gBlackmapParams*` @ `0x1411d64e0`, `BWorldVisibility_loadConfig` @ `0x1406D4D10` | `crates/render/src/terrain/uniforms.rs` |
| Screen projection of callouts, objective pointers, icons | in-game HUD | `crates/render/src/terrain/callouts.rs`, `objective_pointers.rs`, `icons.rs` |
| Screen fades and game timers | trigger effects | `crates/render/src/ui/fades.rs`, `crates/render/src/ui/timers.rs` |
| Frame of a loaded scenario: shadow, scene, particle, distortion and tone-map passes | PC renderer frame | `crates/render/src/world_view.rs`. `terrain_viewer` and a future game client use the same `WorldView`. |

## Gaps

| Area | Engine evidence | Our status | Notes |
|---|---|---|---|
| Scaleform UI screens (`.gfx`) | `scaleform.gfx` @ `0x140f7f448` (`sub_140B85400`); 26 screens, for example `hud_reticle.gfx` @ `0x1411a7b40` (`sub_14022D3D0`) | Missing | `src/main.rs` shows only an `egui` placeholder menu. No `.gfx` or `FlashFonts.xml` (`0x1411ab110`) loader exists. |
| HUD and minimap | `Minimap` @ `0x141181580` (`sub_1400735E0`), `ImageMinimap` @ `0x14119a998` | Partial | `crates/render/src/terrain/icons.rs` projects icon positions. No minimap or HUD draws. |
| HP bars and selection decals | `BDatabase_setupHPBars` @ `0x1401ece90`; `SelectionDecalIntensity` @ `0x1411a0018` | Missing | `ensemble-assets` parses the `hpbars.xml` references. `render` does not draw bars or decals. |
| Audio (Wwise banks, `soundInfo.xml`) | `sound\wwise_2013\GeneratedSoundBanks\Windows\` @ `0x1411e7e40` (`sub_140007CA0`), `init.bnk` (`sub_140007CE0`), `soundInfo.xml` @ `0x1411c9310` (`sub_1405E5550`) | Missing | `crates/render/src/terrain_effect.rs` resolves TFX sound actions but nothing plays them. No audio crate is a dependency. |
| Cinematics (`.cin`) and Bink video | `BCinematic::load` @ `0x14119b0d8` (`sub_14018C7F0`), `video\%s.bik` @ `0x14119b9d8` (`sub_140190E90`) | Missing | `sim` keeps the cinematic trigger state. `render` does not play cinematics or video. |
| Camera effects (`cameraEffects.xml`) | `BCameraEffectManager_load` @ `0x140186150` | Missing | `ensemble-assets` keeps only the effect names that tactics and powers reference. |
| Depth of field and motion blur | `depthoffield` @ `0x14119b408`, `MotionBlurAmount` @ `0x1411b03d8` (`sub_14031E2B0`) | Missing | `crates/render/src/postprocess.rs` has tone map and distortion only. |
| Gamma ramp | `gammaRamp.xml` @ `0x1411aae08` (`sub_1407080F0`) | Missing | No loader in any repository. |
| Damage models on units | `BDamageTemplate_load` @ `0x1401E3F00` | Partial | `crates/sim/src/gameplay/damage_parts.rs` uses damage templates for thrown parts. `render` does not show damage states. |
| Rumble output | `BDatabase_setupRumble` @ `0x1401ec4f0` | Partial | `crates/render/src/ui/rumbles.rs` projects the sim requests. Nothing sends force feedback to the gamepad. |
| Controller configuration | `uiControlDefaults.xml` @ `0x141181890` (`sub_14007BDA0`) | Missing | `crates/render/src/input/` reads `gilrs` and keyboard input with fixed bindings. |
| Terrain self-illumination and environment-mask passes | `gputerraincomposite.bin` programs | Missing | The compositor implements the specular pass only. See [hwde-shader-comparison.md](hwde-shader-comparison.md). |
| Multiplayer session and lobby | WinSock imports (`WSASendTo`, `WSARecvFrom`); `OOSEvent` @ `0x1411c9d50` | Partial | `crates/net/src/` has a generic UDP host and client. No executable uses `net`. No session, lobby or out-of-sync report exists, and the packet format is not the retail format. |
| Game executable | retail game loop | Partial | `src/main.rs` opens a window with a placeholder menu. `src/bin/headless.rs` runs the simulation without a renderer. |

The engine strings show no water or ocean renderer; the only match is the sky material string.
Havok physics and ragdolls belong to the simulation, not to this document.

## Known defects

- The terrain viewer panics on an interactive depth texture:
  [coconutbird/openensemble#7](https://github.com/coconutbird/openensemble/issues/7).
- The viewer exits when `.env` is absent:
  [coconutbird/openensemble#2](https://github.com/coconutbird/openensemble/issues/2).
