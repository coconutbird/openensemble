//! Decoded particle texture arrays that the emitters of a scene share.

use std::collections::{BTreeMap, HashMap, HashSet};

use pipeline::source::{AssetSource, StdFileProvider};
use rayon::prelude::*;

use super::{ParticleError, ParticleImage, ParticleTextureArray, canonical_particle_texture_path};

/// Decoded particle texture arrays, keyed by the authored stage paths of each array.
///
/// Many PFX emitters use the same texture stages. The cache decodes each array one time. A
/// cached array and its clones share the decoded layers.
#[derive(Clone, Debug, Default)]
pub struct ParticleTextureCache {
    arrays: HashMap<Vec<String>, Result<ParticleTextureArray, ParticleError>>,
}

impl ParticleTextureCache {
    /// Decodes the texture arrays in `sets` that the cache does not contain.
    ///
    /// This function reads each DDX file one time from `source`. Then it decodes the images and
    /// builds the arrays in parallel. The result of each array is the same as the result of
    /// [`Self::load`].
    pub fn prefetch(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        sets: impl IntoIterator<Item = Vec<String>>,
    ) {
        let mut seen = HashSet::new();
        let pending = sets
            .into_iter()
            .filter(|paths| !self.arrays.contains_key(paths) && seen.insert(paths.clone()))
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return;
        }
        let mut files = BTreeMap::new();
        for path in pending.iter().flatten() {
            files
                .entry(canonical_particle_texture_path(path))
                .or_insert_with_key(|canonical| source.resolve_with_fallback(canonical, &[".ddx"]));
        }
        let images = files
            .into_par_iter()
            .map(|(canonical, bytes)| {
                let image = match bytes {
                    Some(bytes) => ParticleImage::decode(canonical.clone(), &bytes),
                    None => Err(ParticleError::TextureNotFound(canonical.clone())),
                };
                (canonical, image)
            })
            .collect::<HashMap<_, _>>();
        let arrays = pending
            .into_par_iter()
            .map(|paths| {
                let decoded = paths
                    .iter()
                    .map(|path| images[&canonical_particle_texture_path(path)].clone())
                    .collect();
                let array = ParticleTextureArray::from_decoded(decoded);
                (paths, array)
            })
            .collect::<Vec<_>>();
        self.arrays.extend(arrays);
    }

    /// Returns the texture array of the authored stage paths `paths`.
    ///
    /// Failed stages reuse the first stage in the set that decodes.
    ///
    /// # Errors
    ///
    /// Returns an error when no authored stage can be decoded or array validation fails.
    pub fn load(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        paths: &[String],
    ) -> Result<ParticleTextureArray, ParticleError> {
        if !self.arrays.contains_key(paths) {
            self.prefetch(source, [paths.to_vec()]);
        }
        self.arrays[paths].clone()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pipeline::source::{AssetSource, LoadRule, StdFileProvider};

    use super::ParticleTextureCache;
    use crate::particle::{ParticleImage, ParticleTextureArray};

    /// An uncompressed A8R8G8B8 DDS image of `size` x `size` pixels with a different value in
    /// each byte.
    fn dds(size: u32, seed: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut push = |value: u32| bytes.extend_from_slice(&value.to_le_bytes());
        push(0x2053_4444); // "DDS "
        push(124);
        push(0x100F); // caps, height, width, pixel format
        push(size);
        push(size);
        push(size * 4);
        push(0);
        push(1);
        (0..11).for_each(|_| push(0));
        push(32);
        push(0x41); // RGB with alpha
        push(0);
        push(32);
        push(0x00FF_0000);
        push(0x0000_FF00);
        push(0x0000_00FF);
        push(0xFF00_0000);
        push(0x1000);
        (0..4).for_each(|_| push(0));
        let texels = usize::try_from(size * size * 4).expect("small image");
        bytes.extend(
            (0..texels).map(|index| seed.wrapping_add(u8::try_from(index % 251).expect("byte"))),
        );
        bytes
    }

    fn source_with(files: &[(&str, Vec<u8>)]) -> AssetSource<StdFileProvider> {
        let root = std::env::temp_dir().join(format!(
            "render-particle-texture-cache-{}",
            std::process::id()
        ));
        for (path, bytes) in files {
            let file = root.join(Path::new(path));
            std::fs::create_dir_all(file.parent().expect("parent")).expect("fixture dir");
            std::fs::write(file, bytes).expect("fixture file");
        }
        let mut source = AssetSource::with_provider(StdFileProvider);
        source
            .add_folder(&root.to_string_lossy(), LoadRule::Replace)
            .expect("fixture layer");
        source
    }

    /// Build the array the way the renderer did before the cache: decode each stage again.
    fn load_each_stage(source: &mut AssetSource<StdFileProvider>, paths: &[String]) -> String {
        let decoded = paths
            .iter()
            .map(|path| ParticleImage::load(source, path))
            .collect();
        format!("{:?}", ParticleTextureArray::from_decoded(decoded))
    }

    #[test]
    fn cached_arrays_match_decoding_each_stage() {
        let mut source = source_with(&[
            ("art/fx/big.ddx", dds(4, 3)),
            ("art/fx/small.ddx", dds(2, 7)),
        ]);
        let sets = [
            vec!["fx/big.tga".to_owned(), "fx\\small.tga".to_owned()],
            vec!["fx/small".to_owned(), "fx/missing.tga".to_owned()],
            vec!["fx/missing".to_owned()],
            vec!["ART\\FX\\BIG.TGA".to_owned()],
        ];
        let expected = sets
            .iter()
            .map(|paths| load_each_stage(&mut source, paths))
            .collect::<Vec<_>>();

        let mut prefetched = ParticleTextureCache::default();
        prefetched.prefetch(&mut source, sets.iter().chain(&sets).cloned());
        let mut lazy = ParticleTextureCache::default();
        for (paths, expected) in sets.iter().zip(&expected) {
            assert_eq!(
                &format!("{:?}", prefetched.load(&mut source, paths)),
                expected
            );
            assert_eq!(&format!("{:?}", lazy.load(&mut source, paths)), expected);
        }

        let resized = prefetched.load(&mut source, &sets[0]).expect("two layers");
        assert_eq!(resized.dimensions(), [4, 4]);
        assert_eq!(resized.resampled_layer_count(), 1);
        let fallback = prefetched.load(&mut source, &sets[1]).expect("one layer");
        assert_eq!(fallback.fallback_layer_count(), 1);
        assert!(prefetched.load(&mut source, &sets[2]).is_err());
    }
}
