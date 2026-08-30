use std::mem;

use super::gpu::particle_blend_state;
use super::{
    MATERIAL_ALPHA_TEST, MATERIAL_LIGHT_VOLUME, MATERIAL_PREMULTIPLY_COLOR_ALPHA,
    MATERIAL_SOFT_FADE_RGB, PackedParticleInstance, PackedParticleMaterial, PackedParticleScene,
    ParticleBlendMode, ParticleError, ParticleGeometry, ParticleImage, ParticleInstance,
    ParticleMaterial, ParticleScene, ParticleTextureArray, canonical_particle_texture_path,
    substitute_failed_particle_layers,
};

#[test]
fn perspective_depth_coefficients_reconstruct_positive_eye_distance() {
    let projection = glam::Mat4::perspective_rh(1.0, 1.5, 0.25, 4096.0);
    let [scale, bias] = ParticleScene::perspective_depth_unproject(projection);
    for distance in [0.25_f32, 1.0, 100.0, 4096.0] {
        let clip = projection * glam::Vec4::new(0.0, 0.0, -distance, 1.0);
        let device_depth = clip.z / clip.w;
        let reconstructed = (device_depth * scale + bias).recip();
        assert!((reconstructed - distance).abs() <= distance * 0.001 + 0.000_01);
    }
}

#[test]
fn particle_scene_combines_volume_decode_and_scenario_scales() {
    let rows = [
        [2.0, 0.0, 0.0, 3.0],
        [0.0, 0.0, 4.0, 5.0],
        [0.0, 6.0, 0.0, 7.0],
    ];
    let scene = ParticleScene::new(
        glam::Mat4::IDENTITY,
        glam::Mat4::IDENTITY,
        [0.0; 3],
        [1, 1],
        [0.0; 2],
    )
    .with_light_volume(rows, 1.4);
    let packed = PackedParticleScene::from_scene(&scene);
    assert_eq!(
        packed.light_volume_row0.map(f32::to_bits),
        rows[0].map(f32::to_bits)
    );
    assert!((packed.light_volume_params[0] - 16.8).abs() < 1.0e-6);
}

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
    assert_eq!(mem::size_of::<PackedParticleInstance>(), 12 * 16);
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
    assert_eq!(instance.size.map(f32::to_bits), [2.0; 2].map(f32::to_bits));
    assert_eq!(instance.soft_fade_scale.to_bits(), 1.0_f32.to_bits());
}

#[test]
fn texture_arrays_resample_mixed_authored_dimensions() {
    let first = ParticleImage::from_rgba(1, 1, vec![255; 4], 1.0).unwrap();
    let second = ParticleImage::from_rgba(2, 1, vec![255; 8], 1.0).unwrap();
    let array = ParticleTextureArray::new(vec![first, second]).unwrap();
    assert_eq!(array.dimensions(), [2, 1]);
    assert_eq!(array.layers()[0].pixels, vec![255; 8]);
}

#[test]
fn missing_particle_stage_reuses_the_first_decodable_stage() {
    let image = ParticleImage::from_rgba(1, 1, vec![1, 2, 3, 4], 1.0).unwrap();
    let (layers, fallback_count) = substitute_failed_particle_layers(vec![
        Err(ParticleError::TextureNotFound("missing-first".to_owned())),
        Ok(image.clone()),
        Err(ParticleError::TextureNotFound("missing-last".to_owned())),
    ])
    .unwrap();
    assert_eq!(layers, vec![image.clone(), image.clone(), image]);
    assert_eq!(fallback_count, 2);
}

#[test]
fn entirely_missing_particle_set_remains_an_asset_error() {
    let error = substitute_failed_particle_layers(vec![Err(ParticleError::TextureNotFound(
        "missing".to_owned(),
    ))])
    .unwrap_err();
    assert!(matches!(error, ParticleError::TextureNotFound(path) if path == "missing"));
}

#[test]
fn blend_states_match_retail_d3d9_state() {
    for (mode, src, dst, operation) in [
        (
            ParticleBlendMode::Alpha,
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
            wgpu::BlendOperation::Add,
        ),
        (
            ParticleBlendMode::Additive,
            wgpu::BlendFactor::One,
            wgpu::BlendFactor::One,
            wgpu::BlendOperation::Add,
        ),
        (
            ParticleBlendMode::PremultipliedAlpha,
            wgpu::BlendFactor::One,
            wgpu::BlendFactor::OneMinusSrcAlpha,
            wgpu::BlendOperation::Add,
        ),
        (
            ParticleBlendMode::Subtractive,
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::One,
            wgpu::BlendOperation::ReverseSubtract,
        ),
        (
            ParticleBlendMode::Distortion,
            wgpu::BlendFactor::One,
            wgpu::BlendFactor::One,
            wgpu::BlendOperation::Add,
        ),
    ] {
        let state = particle_blend_state(mode);
        assert_eq!(state.color.src_factor, src);
        assert_eq!(state.color.dst_factor, dst);
        assert_eq!(state.color.operation, operation);
        assert_eq!(state.alpha, state.color);
    }
}

#[test]
fn material_flags_follow_retail_shader_permutations() {
    let mut material = ParticleMaterial {
        blend: ParticleBlendMode::Additive,
        light_volume: true,
        ..ParticleMaterial::default()
    };
    let additive = PackedParticleMaterial::from_material(&material).flags[0];
    assert_ne!(additive & MATERIAL_PREMULTIPLY_COLOR_ALPHA, 0);
    assert_ne!(additive & MATERIAL_SOFT_FADE_RGB, 0);
    assert_ne!(additive & MATERIAL_ALPHA_TEST, 0);
    assert_eq!(additive & MATERIAL_LIGHT_VOLUME, 0);

    material.blend = ParticleBlendMode::Subtractive;
    material.soft_particles = true;
    let subtractive = PackedParticleMaterial::from_material(&material).flags[0];
    assert_eq!(subtractive & MATERIAL_PREMULTIPLY_COLOR_ALPHA, 0);
    assert_eq!(subtractive & MATERIAL_SOFT_FADE_RGB, 0);
    assert_ne!(subtractive & MATERIAL_ALPHA_TEST, 0);

    material.blend = ParticleBlendMode::PremultipliedAlpha;
    let premultiplied = PackedParticleMaterial::from_material(&material).flags[0];
    assert_ne!(premultiplied & MATERIAL_PREMULTIPLY_COLOR_ALPHA, 0);
    assert_ne!(premultiplied & MATERIAL_SOFT_FADE_RGB, 0);
    assert_eq!(premultiplied & MATERIAL_ALPHA_TEST, 0);
    assert_ne!(premultiplied & MATERIAL_LIGHT_VOLUME, 0);
}
