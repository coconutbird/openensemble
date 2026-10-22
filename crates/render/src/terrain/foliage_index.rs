//! Decoding of the XTT foliage index buffers.

use std::collections::BTreeSet;

/// Extracts one (`local_grid_index`, `blade_type`) entry per foliage blade.
///
/// The fresh PC shader consumes a 32-bit index whose upper 16 bits select the
/// blade geometry and whose lower 16 bits are
/// `local_grid_index * 10 + vertex_in_blade`. The local XTT decoder has already
/// converted the file's big-endian bytes into those words. The shader's decimal
/// decomposition is hard-coded to ten vertices, so sets with any other geometry
/// stride cannot use this path.
#[must_use]
pub fn parse_foliage_index_buffer(indices: &[u32], num_verts_per_blade: u32) -> Vec<[u32; 2]> {
    const INDEX_VERTICES_PER_BLADE: u32 = 10;
    let mut blade_entries = Vec::new();
    if indices.is_empty() || num_verts_per_blade != INDEX_VERTICES_PER_BLADE {
        return blade_entries;
    }

    let mut seen = BTreeSet::new();
    for &packed in indices {
        let index_part = packed & 0xFFFF;
        if index_part == 0xFFFF {
            continue;
        }

        let vertex_in_blade = index_part % INDEX_VERTICES_PER_BLADE;
        if vertex_in_blade == 0 {
            let entry = [index_part / INDEX_VERTICES_PER_BLADE, packed >> 16];
            if seen.insert(entry) {
                blade_entries.push(entry);
            }
        }
    }
    blade_entries
}

#[cfg(test)]
mod tests {
    use super::parse_foliage_index_buffer;

    #[test]
    fn foliage_index_buffer_extracts_one_entry_per_blade() {
        let indices = [
            0x0002_0014_u32,
            0x0002_0015,
            0x0002_0014,
            0x0000_FFFF,
            0x0001_0028,
        ];
        assert_eq!(parse_foliage_index_buffer(&indices, 10), [[2, 2], [4, 1]]);
    }

    #[test]
    fn foliage_index_buffer_rejects_geometry_that_cannot_match_the_oracle() {
        assert_eq!(
            parse_foliage_index_buffer(&[0x0002_0014], 8),
            [] as [[u32; 2]; 0]
        );
    }
}
