//! Scenario-layered clips used by trigger-authored object animations.

use super::GameplayCatalog;
use super::timing::{
    TimingAssetCache, canonical_animation_path, find_animation, load_animation_duration,
    load_visual,
};
use pipeline::database::hw1::visual::{Anim, Visual};
use pipeline::database::hw1::{Database, ProtoObject};
use pipeline::source::{AssetSource, StdFileProvider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptedAnimationClip {
    asset_path: String,
    duration_ms: u32,
}

impl ScriptedAnimationClip {
    pub(crate) fn asset_path(&self) -> &str {
        &self.asset_path
    }

    pub(crate) const fn duration_ms(&self) -> u32 {
        self.duration_ms
    }
}

impl GameplayCatalog {
    pub(crate) fn load_scripted_animation_requests(
        &mut self,
        database: &Database,
        source: &mut AssetSource<StdFileProvider>,
        requests: impl IntoIterator<Item = (String, String)>,
    ) {
        let requests = requests
            .into_iter()
            .map(|(prototype, animation)| {
                (
                    prototype.trim().to_ascii_lowercase(),
                    animation.trim().to_ascii_lowercase(),
                )
            })
            .filter(|(prototype, animation)| !prototype.is_empty() && !animation.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        let mut cache = TimingAssetCache::default();
        for (prototype, animation) in requests {
            let Some(object) = database
                .objects
                .iter()
                .find(|object| object.name.eq_ignore_ascii_case(&prototype))
            else {
                continue;
            };
            let Some(clip) = load_clip(object, &animation, source, &mut cache) else {
                continue;
            };
            self.scripted_animation_clips
                .insert((prototype, animation), clip);
        }
    }

    pub(crate) fn scripted_animation_clip(
        &self,
        prototype: &str,
        animation: &str,
    ) -> Option<&ScriptedAnimationClip> {
        self.scripted_animation_clips.get(&(
            prototype.trim().to_ascii_lowercase(),
            animation.trim().to_ascii_lowercase(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn insert_test_scripted_animation_clip(
        &mut self,
        prototype: &str,
        animation: &str,
        asset_path: &str,
        duration_ms: u32,
    ) {
        self.scripted_animation_clips.insert(
            (
                prototype.to_ascii_lowercase(),
                animation.to_ascii_lowercase(),
            ),
            ScriptedAnimationClip {
                asset_path: asset_path.to_owned(),
                duration_ms,
            },
        );
    }
}

fn load_clip(
    object: &ProtoObject,
    animation_type: &str,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> Option<ScriptedAnimationClip> {
    let visual = load_visual(object, source, cache).ok()?;
    let animation = primary_animation(&visual, animation_type)?;
    animation
        .assets
        .iter()
        .filter(|asset| asset.asset_type.eq_ignore_ascii_case("Anim"))
        .filter_map(|asset| asset.file.as_deref())
        .find_map(|file| {
            let asset_path = canonical_animation_path(file);
            let duration = load_animation_duration(&asset_path, source, cache).ok()?;
            let duration_ms = num_traits::ToPrimitive::to_u32(&(duration * 1_000.0))?;
            Some(ScriptedAnimationClip {
                asset_path,
                duration_ms,
            })
        })
}

fn primary_animation<'visual>(
    visual: &'visual Visual,
    animation_type: &str,
) -> Option<&'visual Anim> {
    visual
        .default_model
        .as_deref()
        .and_then(|name| {
            visual
                .models
                .iter()
                .find(|model| model.name.eq_ignore_ascii_case(name))
        })
        .and_then(|model| find_animation(model, animation_type))
        .or_else(|| {
            visual
                .models
                .iter()
                .find_map(|model| find_animation(model, animation_type))
        })
}
