//! Cached UAX loading for static idle and sim-selected scripted clips.

use std::collections::HashMap;
use std::sync::Arc;

use pipeline::database::hw1::visual::Model as VisualModel;
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::uax::Reader as UaxReader;
use pipeline::uax::types::Animation;

#[derive(Default)]
pub(super) struct AnimationAssetCache {
    animations: HashMap<String, Option<Arc<Animation>>>,
}

pub(super) fn load_start_animation(
    source: &mut AssetSource<StdFileProvider>,
    model: &VisualModel,
    cache: &mut AnimationAssetCache,
) -> Option<Arc<Animation>> {
    let path = model
        .anims
        .iter()
        .find(|animation| animation.anim_type.eq_ignore_ascii_case("Idle"))
        .and_then(|animation| {
            animation.assets.iter().find_map(|asset| {
                asset
                    .asset_type
                    .eq_ignore_ascii_case("Anim")
                    .then_some(asset.file.as_deref())
                    .flatten()
            })
        })?;
    load_animation(source, path, &model.name, cache)
}

pub(super) fn load_animation(
    source: &mut AssetSource<StdFileProvider>,
    path: &str,
    model_name: &str,
    cache: &mut AnimationAssetCache,
) -> Option<Arc<Animation>> {
    let canonical_path = canonical_animation_path(path);
    let key = canonical_path.to_ascii_lowercase();
    if let Some(animation) = cache.animations.get(&key) {
        return animation.clone();
    }
    let Some(bytes) = source.resolve_with_fallback(&canonical_path, &[".uax"]) else {
        log::warn!(
            "UGX visual model '{model_name}' is missing optional animation '{canonical_path}'; using bind pose"
        );
        cache.animations.insert(key, None);
        return None;
    };
    let animation = match UaxReader::read(&bytes) {
        Ok(animation) => Arc::new(animation),
        Err(error) => {
            log::warn!(
                "UGX visual model '{model_name}' could not decode optional animation '{canonical_path}'; using bind pose: {error}"
            );
            cache.animations.insert(key, None);
            return None;
        }
    };
    cache.animations.insert(key, Some(Arc::clone(&animation)));
    Some(animation)
}

pub(super) fn canonical_animation_path(path: &str) -> String {
    let normalized = path
        .trim()
        .replace('/', "\\")
        .trim_start_matches('\\')
        .to_owned();
    if normalized.to_ascii_lowercase().starts_with("art\\") {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}
