//! Cached UAX loading for static idle and sim-selected scripted clips.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use pipeline::database::hw1::visual::{Asset, Model as VisualModel};
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::uax::Reader as UaxReader;
use pipeline::uax::types::Animation;

#[derive(Default)]
pub(super) struct AnimationAssetCache {
    animations: HashMap<String, Option<Arc<Animation>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AnimationSelection<'model> {
    definition_index: usize,
    asset_index: Option<usize>,
    asset_path: Option<&'model str>,
}

impl<'model> AnimationSelection<'model> {
    pub(super) const fn definition_index(self) -> usize {
        self.definition_index
    }

    pub(super) const fn asset_index(self) -> Option<usize> {
        self.asset_index
    }

    pub(super) const fn asset_path(self) -> Option<&'model str> {
        self.asset_path
    }
}

pub(super) fn select_animation<'model>(
    model: &'model VisualModel,
    requested_type: &str,
    exact_asset_path: Option<&str>,
    preferred_asset_index: Option<usize>,
    selection_roll: u64,
) -> Option<AnimationSelection<'model>> {
    let definition_index = find_animation_index(model, requested_type)?;
    let assets = &model.anims[definition_index].assets;
    let exact_index = exact_asset_path.and_then(|expected| {
        let expected = canonical_animation_identity(expected);
        assets.iter().position(|asset| {
            authored_animation_path(asset)
                .is_some_and(|path| canonical_animation_identity(path) == expected)
        })
    });
    let asset_index = exact_index
        .or_else(|| {
            preferred_asset_index.filter(|&index| animation_asset_path(assets, index).is_some())
        })
        .or_else(|| weighted_animation_asset_index(assets, selection_roll));
    Some(AnimationSelection {
        definition_index,
        asset_index,
        asset_path: asset_index.and_then(|index| animation_asset_path(assets, index)),
    })
}

fn weighted_animation_asset_index(assets: &[Asset], selection_roll: u64) -> Option<usize> {
    let valid = assets
        .iter()
        .enumerate()
        .filter(|(_, asset)| authored_animation_path(asset).is_some())
        .collect::<Vec<_>>();
    let fallback = valid.first().map(|(index, _)| *index)?;
    let weight_sum = valid.iter().fold(0_u64, |sum, (_, asset)| {
        sum.saturating_add(u64::try_from(asset.weight.unwrap_or(1).max(0)).unwrap_or_default())
    });
    if weight_sum == 0 {
        return Some(fallback);
    }
    let target = unbiased_bounded(selection_roll, weight_sum);
    let mut cumulative = 0_u64;
    for (index, asset) in valid {
        cumulative = cumulative
            .saturating_add(u64::try_from(asset.weight.unwrap_or(1).max(0)).unwrap_or_default());
        if cumulative > target {
            return Some(index);
        }
    }
    Some(fallback)
}

fn unbiased_bounded(seed: u64, upper_bound: u64) -> u64 {
    let threshold = upper_bound.wrapping_neg() % upper_bound;
    let mut random = splitmix64(seed);
    loop {
        let product = u128::from(random) * u128::from(upper_bound);
        let low = u64::try_from(product & u128::from(u64::MAX)).unwrap_or_default();
        if low >= threshold {
            return u64::try_from(product >> 64).unwrap_or_default();
        }
        random = splitmix64(random);
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn find_animation_index(model: &VisualModel, requested_type: &str) -> Option<usize> {
    let find = |name: &str| {
        model
            .anims
            .iter()
            .position(|animation| animation.anim_type.eq_ignore_ascii_case(name))
    };
    find(requested_type).or_else(|| {
        if requested_type.eq_ignore_ascii_case("Sprint") {
            find("Run").or_else(|| find("Walk"))
        } else if requested_type.eq_ignore_ascii_case("Recover")
            || requested_type.eq_ignore_ascii_case("Research")
            || requested_type.eq_ignore_ascii_case("Train")
        {
            find("Idle")
        } else if requested_type.eq_ignore_ascii_case("Run")
            || requested_type.eq_ignore_ascii_case("Jog")
        {
            find("Walk")
        } else {
            None
        }
    })
}

fn animation_asset_path(assets: &[Asset], index: usize) -> Option<&str> {
    assets.get(index).and_then(authored_animation_path)
}

fn authored_animation_path(asset: &Asset) -> Option<&str> {
    asset
        .asset_type
        .eq_ignore_ascii_case("Anim")
        .then_some(asset.file.as_deref())
        .flatten()
}

fn canonical_animation_identity(path: &str) -> String {
    let mut identity = canonical_animation_path(path).to_ascii_lowercase();
    if Path::new(&identity)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("uax"))
    {
        identity.truncate(identity.len() - ".uax".len());
    }
    identity
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

#[cfg(test)]
mod tests {
    use pipeline::database::hw1::visual::{Anim, Asset, Model};

    use super::select_animation;

    fn animation(anim_type: &str, files: &[&str]) -> Anim {
        Anim {
            anim_type: anim_type.to_owned(),
            assets: files
                .iter()
                .map(|file| Asset {
                    asset_type: "Anim".to_owned(),
                    file: Some((*file).to_owned()),
                    ..Asset::default()
                })
                .collect(),
            ..Anim::default()
        }
    }

    #[test]
    fn exact_root_asset_determines_the_synchronized_variant_index() {
        let model = Model {
            anims: vec![animation("Attack", &["attack_a", "attack_b"])],
            ..Model::default()
        };

        let selected = select_animation(&model, "attack", Some("ART/ATTACK_B.UAX"), Some(0), 0)
            .expect("attack animation");
        assert_eq!(selected.definition_index(), 0);
        assert_eq!(selected.asset_index(), Some(1));
        assert_eq!(selected.asset_path(), Some("attack_b"));
    }

    #[test]
    fn synchronized_child_uses_its_own_asset_at_the_parent_index() {
        let model = Model {
            anims: vec![animation("Death", &["child_death_a", "child_death_b"])],
            ..Model::default()
        };

        let selected =
            select_animation(&model, "Death", None, Some(1), 0).expect("death animation");
        assert_eq!(selected.asset_index(), Some(1));
        assert_eq!(selected.asset_path(), Some("child_death_b"));
    }

    #[test]
    fn retail_animation_substitutions_select_the_authored_fallback() {
        let model = Model {
            anims: vec![animation("Walk", &["walk"]), animation("Idle", &["idle"])],
            ..Model::default()
        };

        assert_eq!(
            select_animation(&model, "Sprint", None, None, 0)
                .expect("sprint fallback")
                .definition_index(),
            0
        );
        assert_eq!(
            select_animation(&model, "Research", None, None, 0)
                .expect("research fallback")
                .definition_index(),
            1
        );
        assert!(select_animation(&model, "Death", None, None, 0).is_none());
    }

    #[test]
    fn weighted_selection_ignores_zero_weight_assets_without_first_asset_bias() {
        let mut model = Model {
            anims: vec![animation("Idle", &["never", "common", "rare"])],
            ..Model::default()
        };
        model.anims[0].assets[0].weight = Some(0);
        model.anims[0].assets[1].weight = Some(3);
        model.anims[0].assets[2].weight = Some(1);
        let selected = (0..256)
            .map(|roll| {
                select_animation(&model, "Idle", None, None, roll)
                    .unwrap()
                    .asset_index()
                    .unwrap()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(selected, std::collections::BTreeSet::from([1, 2]));
    }
}
