use std::mem;

use super::{
    PATCH_SUBDIVISIONS, PackedPatchInstance, TerrainPatchImage, TerrainPatchInstance,
    canonical_patch_path, patch_grid,
};

#[test]
fn instance_matches_five_float4_oracle_control_point() {
    let instance = TerrainPatchInstance {
        center: [1.0, 2.0, 3.0],
        axis_u: [4.0, 5.0, 6.0],
        axis_v: [7.0, 8.0, 9.0],
        y_offset: 10.0,
        intensity: 11.0,
        color: [12.0, 13.0, 14.0, 15.0],
        uv_rect: [16.0, 17.0, 18.0, 19.0],
    }
    .packed();
    assert_eq!(mem::size_of::<PackedPatchInstance>(), 5 * 16);
    assert_eq!(
        instance.position.map(f32::to_bits),
        [1.0, 2.0, 3.0, 1.0].map(f32::to_bits)
    );
    assert_eq!(
        instance.axis_v_and_axis_u_x.map(f32::to_bits),
        [7.0, 8.0, 9.0, 4.0].map(f32::to_bits)
    );
    assert_eq!(
        instance.axis_u_yz_offset_intensity.map(f32::to_bits),
        [5.0, 6.0, 10.0, 11.0].map(f32::to_bits)
    );
}

#[test]
fn carrier_grid_matches_oracle_maximum_tessellation() {
    let (vertices, indices) = patch_grid();
    let vertices_per_axis = usize::from(PATCH_SUBDIVISIONS + 1);
    assert_eq!(vertices.len(), vertices_per_axis.pow(2));
    assert_eq!(indices.len(), usize::from(PATCH_SUBDIVISIONS).pow(2) * 6);
    assert_eq!(
        vertices
            .first()
            .copied()
            .map(|value| value.map(f32::to_bits)),
        Some([0.0, 0.0].map(f32::to_bits))
    );
    assert_eq!(
        vertices
            .last()
            .copied()
            .map(|value| value.map(f32::to_bits)),
        Some([1.0, 1.0].map(f32::to_bits))
    );
}

#[test]
fn decal_map_suffixes_canonicalize_to_one_base() {
    assert_eq!(
        canonical_patch_path("decals/warthog01_df.ddx"),
        "art\\decals\\warthog01"
    );
    assert_eq!(
        canonical_patch_path("art\\decals\\warthog01_nm"),
        "art\\decals\\warthog01"
    );
}

#[test]
fn invalid_rgba_size_is_rejected() {
    assert!(TerrainPatchImage::from_rgba(2, 2, vec![0; 15]).is_err());
}
