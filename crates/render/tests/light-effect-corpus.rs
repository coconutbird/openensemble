//! Installed-asset validation for Phoenix `.lgt` light scenes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use glam::Mat4;
use render::light_effect::{LightEffect, LightEffectRuntime};

#[test]
#[ignore = "requires OPENENSEMBLE_GAME_DIR pointing at Halo Wars DE"]
fn all_shipped_light_effects_decode_and_sample_finitely() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point at Halo Wars DE");
    let mut source = pipeline::hw1::loader::load_game_dir(&game_dir);
    let paths = source
        .files_per_archive()
        .into_iter()
        .flat_map(|(_, paths)| paths)
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lgt"))
        })
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    assert!(!paths.is_empty(), "installed corpus contains no .lgt files");

    let mut versions = BTreeMap::<u32, usize>::new();
    let mut frames = 0;
    let mut keys = 0;
    let mut samples = 0;
    let mut buffered_keys = 0;
    let mut shadow_keys = 0;
    let mut shadow_paths = BTreeSet::new();
    for path in &paths {
        let effect =
            LightEffect::load(&mut source, path).unwrap_or_else(|error| panic!("{path}: {error}"));
        assert!(!effect.frames.is_empty(), "{path}: scene has no frames");
        *versions.entry(effect.version).or_default() += 1;
        frames += effect.frames.len();
        keys += effect
            .frames
            .iter()
            .map(|frame| frame.lights.len())
            .sum::<usize>();
        buffered_keys += effect
            .frames
            .iter()
            .flat_map(|frame| &frame.lights)
            .filter(|light| light.flags.light_buffered())
            .count();
        let effect_shadow_keys = effect
            .frames
            .iter()
            .flat_map(|frame| &frame.lights)
            .filter(|light| light.flags.shadows())
            .count();
        shadow_keys += effect_shadow_keys;
        if effect_shadow_keys != 0 {
            shadow_paths.insert(path.clone());
        }

        let duration = effect.duration();
        let mut runtime = LightEffectRuntime::new(Arc::new(effect));
        for fraction in [0.0, 0.25, 0.5, 0.75] {
            let delta = if duration > 0.0 {
                duration * fraction
            } else {
                0.0
            };
            for light in runtime.advance_and_sample(delta, Mat4::IDENTITY, 1.3) {
                assert!(light.position.into_iter().all(f32::is_finite), "{path}");
                assert!(light.color.into_iter().all(f32::is_finite), "{path}");
                assert!(light.radius.is_finite(), "{path}");
                samples += 1;
            }
        }
    }

    println!("light_effect_files {}", paths.len());
    println!("light_effect_versions {versions:?}");
    println!("light_effect_frames {frames}");
    println!("light_effect_keys {keys}");
    println!("light_effect_buffered_keys {buffered_keys}");
    println!("light_effect_shadow_keys {shadow_keys}");
    println!("light_effect_shadow_paths {shadow_paths:?}");
    println!("light_effect_runtime_samples {samples}");
}
