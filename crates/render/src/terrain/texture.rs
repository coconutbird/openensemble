//! Texture utilities for terrain rendering.
//!
//! Provides mipmap generation and texture helper functions.

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
    (u32::BITS - width.max(height).leading_zeros()).max(1)
}

/// Generate mipmaps for an RGBA image on CPU.
///
/// Uses box filtering (2x2 average) for downsampling.
/// Returns a vector of mip levels, each containing RGBA pixel data.
/// Level 0 is the original image, level 1 is half size, etc.
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
    let mut mips = Vec::new();

    // Level 0 is the original
    mips.push(pixels.to_vec());

    let mut current_width = width;
    let mut current_height = height;
    let mut current_pixels = pixels.to_vec();

    // Generate mip levels until we reach 1x1
    while current_width > 1 || current_height > 1 {
        let new_width = (current_width / 2).max(1);
        let new_height = (current_height / 2).max(1);
        let Some(pixel_count) = usize::try_from(new_width)
            .ok()
            .and_then(|width| {
                usize::try_from(new_height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
        else {
            return mips;
        };
        let mut new_pixels = vec![0u8; pixel_count];

        // Box filter: average 2x2 blocks
        for y in 0..new_height {
            for x in 0..new_width {
                let src_x = (x * 2).min(current_width - 1);
                let src_y = (y * 2).min(current_height - 1);

                // Sample up to 4 pixels (handle edge cases)
                let mut r: u32 = 0;
                let mut g: u32 = 0;
                let mut b: u32 = 0;
                let mut a: u32 = 0;
                let mut count: u32 = 0;

                for dy in 0..2 {
                    for dx in 0..2 {
                        let sx = (src_x + dx).min(current_width - 1);
                        let sy = (src_y + dy).min(current_height - 1);
                        let idx = usize::try_from(
                            (u64::from(sy) * u64::from(current_width) + u64::from(sx)) * 4,
                        )
                        .unwrap_or(usize::MAX);
                        if idx + 3 < current_pixels.len() {
                            r += u32::from(current_pixels[idx]);
                            g += u32::from(current_pixels[idx + 1]);
                            b += u32::from(current_pixels[idx + 2]);
                            a += u32::from(current_pixels[idx + 3]);
                            count += 1;
                        }
                    }
                }

                let [Some(r), Some(g), Some(b), Some(a)] = [r, g, b, a].map(|channel| {
                    channel
                        .checked_div(count)
                        .and_then(|value| u8::try_from(value).ok())
                }) else {
                    continue;
                };
                let dst_idx =
                    usize::try_from((u64::from(y) * u64::from(new_width) + u64::from(x)) * 4)
                        .unwrap_or(usize::MAX);
                if let Some(pixel) = new_pixels.get_mut(dst_idx..dst_idx + 4) {
                    pixel.copy_from_slice(&[r, g, b, a]);
                }
            }
        }

        mips.push(new_pixels.clone());
        current_width = new_width;
        current_height = new_height;
        current_pixels = new_pixels;
    }

    mips
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
}
