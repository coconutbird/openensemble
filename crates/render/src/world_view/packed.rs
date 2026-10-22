//! Conversion of the packed XTD textures into terrain-world axes.

/// Converts the packed XTD source axes into the terrain-world axes.
///
/// The source atlas and the XTT material world are diagonally mirrored. A
/// complete conversion must transpose texels and swap each packed vector's
/// red/blue components; transposing texels alone moves the height correctly
/// but applies X/Z displacement from the wrong orientation and folds cliffs.
#[must_use]
pub(crate) fn xtd_packed_to_world(words: &[u32], width: u32) -> Vec<u32> {
    let width = usize::try_from(width).expect("XTD texture width must fit usize");
    let expected_len = width
        .checked_mul(width)
        .expect("XTD texture dimensions must fit usize");
    assert_eq!(
        words.len(),
        expected_len,
        "packed XTD texture must be square"
    );

    let mut transformed = vec![0_u32; words.len()];
    for world_x in 0..width {
        for world_z in 0..width {
            let destination = world_x * width + world_z;
            let source = world_z * width + world_x;
            transformed[destination] = swap_rgb10a2_red_blue(words[source]);
        }
    }
    transformed
}

fn swap_rgb10a2_red_blue(packed: u32) -> u32 {
    let red = packed & 0x0000_03ff;
    let green = packed & 0x000f_fc00;
    let blue = packed & 0x3ff0_0000;
    let alpha = packed & 0xc000_0000;
    alpha | (red << 20) | green | (blue >> 20)
}

#[cfg(test)]
mod tests {
    use super::xtd_packed_to_world;

    fn pack(red: u32, green: u32, blue: u32, alpha: u32) -> u32 {
        red | (green << 10) | (blue << 20) | (alpha << 30)
    }

    #[test]
    fn xtd_world_conversion_transposes_texels_and_vector_axes() {
        let source = [
            pack(1, 101, 201, 0),
            pack(2, 102, 202, 1),
            pack(3, 103, 203, 2),
            pack(4, 104, 204, 3),
        ];

        assert_eq!(
            xtd_packed_to_world(&source, 2),
            [
                pack(201, 101, 1, 0),
                pack(203, 103, 3, 2),
                pack(202, 102, 2, 1),
                pack(204, 104, 4, 3),
            ]
        );
    }
}
