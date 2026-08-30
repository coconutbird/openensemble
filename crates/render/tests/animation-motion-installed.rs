//! Installed-data inventory for Granny animation accumulation/root-motion flags.

use std::collections::{BTreeMap, BTreeSet};

#[test]
#[ignore = "requires OPENENSEMBLE_GAME_DIR pointing at Halo Wars DE"]
fn reports_shipped_animation_motion_extraction() {
    let game_dir = std::env::var("OPENENSEMBLE_GAME_DIR")
        .expect("OPENENSEMBLE_GAME_DIR must point at Halo Wars DE");
    let mut source = pipeline::hw1::loader::load_game_dir(&game_dir);
    let visual_paths = source
        .files_per_archive()
        .into_iter()
        .flat_map(|(_, paths)| paths)
        .filter(|path| path.ends_with(".vis.xmb"))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let mut animation_types = BTreeMap::<String, BTreeSet<String>>::new();
    for path in visual_paths {
        let bytes = source
            .resolve_with_fallback(&path, &[])
            .expect("indexed visual");
        let document = pipeline::xmb::Reader::read(&bytes).expect("decode visual XMB");
        let visual = pipeline::database::hw1::visual::parse(&document).expect("parse visual");
        for animation in visual.models.iter().flat_map(|model| &model.anims) {
            for asset in &animation.assets {
                if asset.asset_type.eq_ignore_ascii_case("Anim")
                    && let Some(path) = asset.file.as_deref()
                {
                    animation_types
                        .entry(canonical_uax_path(path))
                        .or_default()
                        .insert(animation.anim_type.to_ascii_lowercase());
                }
            }
        }
    }

    let mut flag_counts = BTreeMap::<u32, usize>::new();
    let mut extracted = Vec::new();
    let mut missing = Vec::new();
    let mut decoded = 0;
    for (path, types) in animation_types {
        let Some(bytes) = source.resolve_with_fallback(&path, &[".uax"]) else {
            missing.push(path);
            continue;
        };
        let animation = pipeline::uax::Reader::read(&bytes)
            .unwrap_or_else(|error| panic!("decode {path}: {error}"));
        decoded += 1;
        for group in &animation.track_groups {
            *flag_counts.entry(group.flags).or_default() += 1;
            if group.flags & 0x5 != 0 {
                extracted.push(format!(
                    "{path} types={types:?} flags={:#x} loop={:?} group={:?}",
                    group.flags, group.loop_translation, group.name,
                ));
            }
        }
    }
    println!("decoded_uax={decoded} track_group_flags={flag_counts:?}");
    println!("missing_optional_uax={missing:?}");
    println!("motion_extracted={}", extracted.len());
    for sample in &extracted {
        println!("  {sample}");
    }
    assert!(
        decoded > 0,
        "installed visual corpus references no UAX files"
    );
}

fn canonical_uax_path(path: &str) -> String {
    let normalized = path.trim().replace('/', "\\");
    if normalized.to_ascii_lowercase().starts_with("art\\") {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}
