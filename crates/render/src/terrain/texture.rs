//! Texture utilities for terrain rendering.
//!
//! Provides mipmap generation and texture helper functions.

use rayon::prelude::*;

/// Calculate the number of mip levels for a given texture size.
///
/// Returns the number of mip levels from the largest dimension down to 1x1.
///
/// # Examples
/// ```
/// use render::terrain::mip_level_count;
/// assert_eq!(mip_level_count(512, 512), 10); // 512 -> 256 -> ... -> 1
/// assert_eq!(mip_level_count(256, 128), 9);  // max(256,128) = 256 -> ... -> 1
/// ```
#[must_use]
pub fn mip_level_count(width: u32, height: u32) -> u32 {
    width.max(height).bit_width().max(1)
}

/// Generate mipmaps for an RGBA image on CPU.
///
/// Uses box filtering (2x2 average) for downsampling.
/// Returns a vector of mip levels, each containing RGBA pixel data.
/// Level 0 is the original image, level 1 is half size, etc.
/// The rows of each level are made in parallel. The result does not depend on the thread count.
///
/// # Arguments
/// * `pixels` - RGBA pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
///
/// # Returns
/// Vector of mip levels, each as `Vec<u8>` of RGBA data.
#[must_use]
pub fn generate_mipmaps(pixels: &[u8], width: u32, height: u32) -> Vec<Vec<u8>> {
    // Level 0 is the original
    let mut mips = vec![pixels.to_vec()];

    let mut current_width = width;
    let mut current_height = height;

    // Generate mip levels until we reach 1x1
    while current_width > 1 || current_height > 1 {
        let new_width = (current_width / 2).max(1);
        let new_height = (current_height / 2).max(1);
        let Some(row_bytes) = usize::try_from(new_width)
            .ok()
            .and_then(|width| width.checked_mul(4))
        else {
            return mips;
        };
        let Some(pixel_count) = usize::try_from(new_height)
            .ok()
            .and_then(|height| height.checked_mul(row_bytes))
        else {
            return mips;
        };
        let mut new_pixels = vec![0u8; pixel_count];
        let current_pixels = mips.last().map_or(&[][..], Vec::as_slice);
        new_pixels
            .par_chunks_mut(row_bytes)
            .zip(0..new_height)
            .for_each(|(row, y)| {
                downsample_row(current_pixels, [current_width, current_height], y, row);
            });

        mips.push(new_pixels);
        current_width = new_width;
        current_height = new_height;
    }

    mips
}

/// Write the row `row_y` of the next mip level of `current_pixels` (size `[width, height]`)
/// into `row`.
fn downsample_row(current_pixels: &[u8], [width, height]: [u32; 2], row_y: u32, row: &mut [u8]) {
    let (pixels, _) = row.as_chunks_mut::<4>();
    for (column, pixel) in (0..).zip(pixels) {
        let src_x = (column * 2).min(width - 1);
        let src_y = (row_y * 2).min(height - 1);

        // Box filter: average 2x2 blocks. Sample up to 4 pixels (handle edge cases)
        let mut sum = [0_u32; 4];
        let mut count: u32 = 0;

        for dy in 0..2 {
            for dx in 0..2 {
                let sx = (src_x + dx).min(width - 1);
                let sy = (src_y + dy).min(height - 1);
                let idx = usize::try_from((u64::from(sy) * u64::from(width) + u64::from(sx)) * 4)
                    .unwrap_or(usize::MAX);
                if let Some(source) = current_pixels.get(idx..idx.saturating_add(4)) {
                    for (total, &value) in sum.iter_mut().zip(source) {
                        *total += u32::from(value);
                    }
                    count += 1;
                }
            }
        }

        let [Some(red), Some(green), Some(blue), Some(alpha)] = sum.map(|channel| {
            channel
                .checked_div(count)
                .and_then(|value| u8::try_from(value).ok())
        }) else {
            continue;
        };
        *pixel = [red, green, blue, alpha];
    }
}

/// Get the dimensions of a mip level given the base size.
///
/// # Arguments
/// * `base_width` - Width at mip level 0
/// * `base_height` - Height at mip level 0
/// * `mip_level` - The mip level to calculate dimensions for
///
/// # Returns
/// (width, height) at the specified mip level
#[must_use]
pub fn mip_dimensions(base_width: u32, base_height: u32, mip_level: u32) -> (u32, u32) {
    let width = (base_width >> mip_level).max(1);
    let height = (base_height >> mip_level).max(1);
    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mip_level_count() {
        assert_eq!(mip_level_count(1, 1), 1);
        assert_eq!(mip_level_count(2, 2), 2);
        assert_eq!(mip_level_count(4, 4), 3);
        assert_eq!(mip_level_count(512, 512), 10);
        assert_eq!(mip_level_count(1024, 1024), 11);
    }

    #[test]
    fn test_mip_dimensions() {
        assert_eq!(mip_dimensions(512, 512, 0), (512, 512));
        assert_eq!(mip_dimensions(512, 512, 1), (256, 256));
        assert_eq!(mip_dimensions(512, 512, 9), (1, 1));
    }

    #[test]
    fn test_generate_mipmaps() {
        // 4x4 solid red image
        let pixels = [255u8, 0, 0, 255].repeat(16);
        let mips = generate_mipmaps(&pixels, 4, 4);

        assert_eq!(mips.len(), 3); // 4x4, 2x2, 1x1
        assert_eq!(mips[0].len(), 64); // 4*4*4
        assert_eq!(mips[1].len(), 16); // 2*2*4
        assert_eq!(mips[2].len(), 4); // 1*1*4
    }

    #[test]
    fn generate_mipmaps_truncates_odd_edges_and_box_averages() {
        // 5x2 image: pixel (x, y) is [40x + y, 100y, x, 200].
        let pixels = (0..2_u8)
            .flat_map(|y| (0..5_u8).flat_map(move |x| [40 * x + y, 100 * y, x, 200]))
            .collect::<Vec<_>>();
        let mips = generate_mipmaps(&pixels, 5, 2);

        assert_eq!(mips.len(), 3);
        assert_eq!(mips[0], pixels);
        // Level 1 is 2x1. Column 4 of level 0 is not sampled.
        assert_eq!(mips[1], [20, 50, 0, 200, 100, 50, 2, 200]);
        // Level 2 is 1x1. The missing row repeats the last row.
        assert_eq!(mips[2], [60, 50, 1, 200]);
    }
}
