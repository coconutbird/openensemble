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
pub fn mip_level_count(width: u32, height: u32) -> u32 {
    ((width.max(height) as f32).log2().floor() as u32) + 1
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
/// Vector of mip levels, each as Vec<u8> of RGBA data.
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
        let mut new_pixels = vec![0u8; (new_width * new_height * 4) as usize];

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
                        let idx = ((sy * current_width + sx) * 4) as usize;
                        if idx + 3 < current_pixels.len() {
                            r += current_pixels[idx] as u32;
                            g += current_pixels[idx + 1] as u32;
                            b += current_pixels[idx + 2] as u32;
                            a += current_pixels[idx + 3] as u32;
                            count += 1;
                        }
                    }
                }

                if count > 0 {
                    let dst_idx = ((y * new_width + x) * 4) as usize;
                    new_pixels[dst_idx] = (r / count) as u8;
                    new_pixels[dst_idx + 1] = (g / count) as u8;
                    new_pixels[dst_idx + 2] = (b / count) as u8;
                    new_pixels[dst_idx + 3] = (a / count) as u8;
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
