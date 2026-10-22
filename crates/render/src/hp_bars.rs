//! Hit-point bars over squads, units and buildings.
//!
//! The style types are a plain copy of the `hpbars.xml` data (`HPBarDefinition`), so that this
//! crate does not depend on the database. Retail draws squad bars (`BHPBar_renderSquadHPBar`,
//! 0x140044080) and unit or building bars (`BHPBar_renderUnitHPBar`, 0x140043530) with the
//! same `renderSquadHPBar` technique of `HPBar\HPBar.bin` (`BHPBar_drawSquadHPBarBatch`,
//! 0x140046F90):
//!
//! - The geometry shader makes a camera-facing world quad of `SizeX` by `SizeY` world units
//!   around the anchor, from the camera right and up directions.
//! - The pixel shader tints the frame texture by the player selection color. Inside each of
//!   the hit-point, shield and ammunition UV rectangles, it blends by the mask alpha to the
//!   stage color on the filled part (`u <= u1 + (u2 - u1) * ratio`), or to the grey
//!   (0.1, 0.1, 0.1, 0.5) on the empty part. It discards a pixel with alpha 0.001 or less.
//! - The blend is source alpha over the destination, with no depth test.
//!
//! [`HpBarRenderer`] draws the bars into the tone-mapped output.

mod renderer;

use std::collections::HashMap;

use glam::Vec3;
use pipeline::source::{AssetSource, StdFileProvider};

use crate::terrain::{TerrainPatchImage, load_ddx_image};

pub use renderer::HpBarRenderer;

/// One `HPBar` entry of `hpbars.xml`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HpBarStyle {
    /// Name of the style, for example `UNSC_Health`.
    pub name: String,
    /// Frame texture path, without the `art\` prefix and the `.ddx` extension.
    pub texture: String,
    /// Fill mask texture path, without the `art\` prefix and the `.ddx` extension.
    pub mask_texture: String,
    /// Default width and height of the bar, in world units.
    pub size: [f32; 2],
    /// Hit-point fill rectangle `[u1, v1, u2, v2]` in texture space.
    pub hitpoint_uv: Option<[f32; 4]>,
    /// Shield fill rectangle `[u1, v1, u2, v2]` in texture space.
    pub shield_uv: Option<[f32; 4]>,
    /// Ammunition fill rectangle `[u1, v1, u2, v2]` in texture space.
    pub ammo_uv: Option<[f32; 4]>,
}

/// One `Stage` of a `ColorStages` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStage {
    /// Highest ratio that the stage applies to.
    pub value: f32,
    /// Stage intensity from the file. Retail parses it but does not send it to the GPU, so
    /// the renderer does not use it.
    pub intensity: f32,
    /// Display color of the stage.
    pub color: [u8; 3],
}

/// The fill colors of the bars (`ColorStages name="DEFAULT"` in retail).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HpBarColorStages {
    /// Stages of the hit-point fill, in file order.
    pub hitpoint: Vec<ColorStage>,
    /// Stages of the shield fill, in file order.
    pub shield: Vec<ColorStage>,
    /// Stages of the ammunition fill, in file order.
    pub ammo: Vec<ColorStage>,
}

/// Index of a style in the style list given to the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HpBarStyleId(pub usize);

/// One bar of a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HpBarInstance {
    /// World position of the bar center.
    pub position: Vec3,
    /// Width and height of the bar, in world units.
    pub size: [f32; 2],
    /// Style of the bar.
    pub style: HpBarStyleId,
    /// Hit-point ratio, 0 to 1.
    pub hitpoints: f32,
    /// Shield ratio, 0 to 1, if the object has a shield.
    pub shield: Option<f32>,
    /// Ammunition ratio, 0 to 1, if the object uses ammunition.
    pub ammo: Option<f32>,
    /// Display color of the owner, which tints the frame.
    pub player_color: [u8; 3],
}

/// Returns the fill color for `ratio`: the color of the first stage with `value >= ratio`.
///
/// The color channels are display values from 0 to 1. Retail does not use the stage
/// intensity. If no stage applies, the color is black, as the retail default `0xFF000000`.
#[must_use]
pub fn stage_color(stages: &[ColorStage], ratio: f32) -> [f32; 3] {
    stages
        .iter()
        .find(|stage| stage.value >= ratio)
        .map_or([0.0; 3], |stage| {
            stage.color.map(|channel| f32::from(channel) / 255.0)
        })
}

/// Loads the frame and mask textures of all styles.
///
/// The result maps each texture path of the styles to its decoded image. A texture that is
/// missing or cannot be decoded is not in the map, and the styles that use it do not draw.
#[must_use]
pub fn load_hp_bar_images(
    source: &mut AssetSource<StdFileProvider>,
    styles: &[HpBarStyle],
) -> HashMap<String, TerrainPatchImage> {
    let mut images = HashMap::new();
    for path in styles
        .iter()
        .flat_map(|style| [&style.texture, &style.mask_texture])
    {
        let key = texture_key(path);
        if images.contains_key(&key) {
            continue;
        }
        match load_ddx_image(source, &asset_path(path)) {
            Ok(Some(image)) => {
                images.insert(key, image);
            }
            Ok(None) => log::warn!("HP bar texture not found: {path}"),
            Err(error) => log::warn!("could not load HP bar texture: {error}"),
        }
    }
    images
}

/// Normalizes a texture path of a style for lookups: backslashes, no leading separator,
/// lowercase.
pub(crate) fn texture_key(path: &str) -> String {
    path.trim()
        .replace('/', "\\")
        .trim_start_matches('\\')
        .to_ascii_lowercase()
}

/// Converts a style texture path into an asset path with the `art\` prefix.
fn asset_path(path: &str) -> String {
    let key = texture_key(path);
    if key.starts_with("art\\") {
        key
    } else {
        format!("art\\{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hitpoint_stages() -> Vec<ColorStage> {
        vec![
            ColorStage {
                value: 0.38,
                intensity: 1.0,
                color: [208, 16, 16],
            },
            ColorStage {
                value: 0.5,
                intensity: 1.0,
                color: [146, 228, 65],
            },
            ColorStage {
                value: 1.0,
                intensity: 2.0,
                color: [255, 0, 51],
            },
        ]
    }

    fn assert_color(actual: [f32; 3], expected: [f32; 3]) {
        let close = actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 1e-6);
        assert!(close, "color {actual:?} is not {expected:?}");
    }

    #[test]
    fn stage_color_picks_the_first_stage_at_or_above_the_ratio() {
        let stages = hitpoint_stages();
        let red = [208.0 / 255.0, 16.0 / 255.0, 16.0 / 255.0];
        let green = [146.0 / 255.0, 228.0 / 255.0, 65.0 / 255.0];
        assert_color(stage_color(&stages, 0.0), red);
        assert_color(stage_color(&stages, 0.38), red);
        assert_color(stage_color(&stages, 0.381), green);
        assert_color(stage_color(&stages, 0.5), green);
        // The stage intensity does not change the color.
        assert_color(stage_color(&stages, 0.75), [1.0, 0.0, 0.2]);
        // No stage applies above the last value, and an empty list gives black.
        assert_color(stage_color(&stages, 1.5), [0.0; 3]);
        assert_color(stage_color(&[], 0.5), [0.0; 3]);
    }

    #[test]
    fn texture_paths_get_the_art_prefix_once() {
        assert_eq!(
            asset_path(r"ui\flash\HUD\hud_hitpntbar\unsc_unit_hptbar\unsc_unit1"),
            r"art\ui\flash\hud\hud_hitpntbar\unsc_unit_hptbar\unsc_unit1"
        );
        assert_eq!(asset_path("/art/ui/bar"), r"art\ui\bar");
        assert_eq!(texture_key(r" UI\Bar "), r"ui\bar");
    }
}
