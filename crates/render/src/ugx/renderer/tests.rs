use super::pipelines::pipeline_index;
use super::uniforms::{
    MATERIAL_FLAG_DISTORTION, MATERIAL_FLAG_EMISSIVE_XFORM, MATERIAL_FLAG_HIGHLIGHT,
    MATERIAL_FLAG_MODULATE, MATERIAL_FLAG_RECEIVES_SHADOWS, MATERIAL_FLAG_TERRAIN_CONFORM,
    MaterialUniform,
};
use crate::ugx::BlendMode;
use crate::ugx::model::{Image, Material, MaterialFeature};

#[test]
fn pipeline_indices_keep_blends_and_culling_distinct() {
    assert_eq!(pipeline_index(BlendMode::Opaque, false), 0);
    assert_eq!(pipeline_index(BlendMode::Opaque, true), 1);
    assert_eq!(pipeline_index(BlendMode::Additive, false), 6);
    assert_eq!(pipeline_index(BlendMode::Additive, true), 7);
}

#[test]
fn material_uniform_packs_oracle_uv_slots_and_effect_flags() {
    let mut material = Material::default();
    for (index, velocity) in material.uv_velocity.iter_mut().enumerate() {
        let value = u16::try_from(index).map_or(f32::INFINITY, f32::from);
        *velocity = [value, -value];
    }
    let effect = Image {
        asset_path: "test/effect".to_owned(),
        width: 1,
        height: 1,
        pixels: vec![255; 4],
        hdr_scale: 2.5,
    };
    material.emissive_xform = Some(effect.clone());
    material.distortion = Some(effect.clone());
    material.highlight = Some(effect);
    material.modulate = Some(Image {
        asset_path: "test/modulate".to_owned(),
        width: 1,
        height: 1,
        pixels: vec![255; 4],
        hdr_scale: 1.0,
    });
    material.set_feature(MaterialFeature::TERRAIN_CONFORM, true);

    let uniform = MaterialUniform::from_material(&material, false, 1.0);
    assert_eq!(
        uniform.uv_velocity0.map(f32::to_bits),
        [0.0, -0.0, 1.0, -1.0].map(f32::to_bits)
    );
    assert_eq!(
        uniform.uv_velocity2.map(f32::to_bits),
        [5.0, -5.0, 8.0, -8.0].map(f32::to_bits)
    );
    assert_eq!(
        uniform.uv_velocity4.map(f32::to_bits),
        [9.0, -9.0, 10.0, -10.0].map(f32::to_bits)
    );
    assert_eq!(
        uniform.uv_velocity5.map(f32::to_bits),
        [11.0, -11.0, 12.0, -12.0].map(f32::to_bits)
    );
    assert_eq!(uniform.hdr_scales[2].to_bits(), 2.5_f32.to_bits());
    assert_ne!(uniform.flags[0] & MATERIAL_FLAG_EMISSIVE_XFORM, 0);
    assert_ne!(uniform.flags[0] & MATERIAL_FLAG_DISTORTION, 0);
    assert_ne!(uniform.flags[0] & MATERIAL_FLAG_HIGHLIGHT, 0);
    assert_ne!(uniform.flags[0] & MATERIAL_FLAG_MODULATE, 0);
    assert_ne!(uniform.flags[0] & MATERIAL_FLAG_RECEIVES_SHADOWS, 0);
    assert_ne!(uniform.flags[0] & MATERIAL_FLAG_TERRAIN_CONFORM, 0);

    material.set_feature(MaterialFeature::RECEIVES_SHADOWS, false);
    let uniform = MaterialUniform::from_material(&material, false, 1.0);
    assert_eq!(uniform.flags[0] & MATERIAL_FLAG_RECEIVES_SHADOWS, 0);
}
