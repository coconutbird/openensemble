use std::mem;

use super::{
    PackedParticleInstance, ParticleGeometry, ParticleImage, ParticleInstance,
    ParticleTextureArray, canonical_particle_texture_path,
};

#[test]
fn pfx_tga_references_resolve_to_art_ddx_stems() {
    assert_eq!(
        canonical_particle_texture_path("effects/flares/glow_01_fx.tga"),
        "art\\effects\\flares\\glow_01_fx"
    );
    assert_eq!(
        canonical_particle_texture_path("art\\effects\\white.ddx"),
        "art\\effects\\white"
    );
}

#[test]
fn particle_instance_stride_is_whole_vec4_slots() {
    assert_eq!(mem::size_of::<PackedParticleInstance>(), 11 * 16);
}

#[test]
fn segment_constructor_preserves_endpoints() {
    let instance = ParticleInstance::segment(
        [0.0, 0.0, 0.0],
        [0.0, 4.0, 0.0],
        2.0,
        [1.0; 4],
        ParticleGeometry::Beam,
    );
    assert_eq!(
        instance.axis.map(f32::to_bits),
        [0.0, 1.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        instance.position.map(f32::to_bits),
        [0.0, 2.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(instance.half_length.to_bits(), 2.0_f32.to_bits());
}

#[test]
fn texture_arrays_reject_mismatched_dimensions() {
    let first = ParticleImage::from_rgba(1, 1, vec![255; 4], 1.0).unwrap();
    let second = ParticleImage::from_rgba(2, 1, vec![255; 8], 1.0).unwrap();
    assert!(ParticleTextureArray::new(vec![first, second]).is_err());
}
