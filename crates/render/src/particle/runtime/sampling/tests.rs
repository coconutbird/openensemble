use pipeline::database::hw1::pfx::{
    ParticleColorDefinition, ParticleColorKey, ParticleColorKind, ParticleColorProgression,
    ParticlePaletteEntry, ParticleScalarKey, ParticleScalarProgression,
};

use super::{sample_color, sample_color_progression, sample_scalar_progression};

#[test]
fn scalar_progression_matches_retail_extrapolation_and_white_fallback() {
    let progression = ParticleScalarProgression {
        keys: vec![
            ParticleScalarKey {
                alpha: 0.2,
                value: 2.0,
                variance: 0.0,
            },
            ParticleScalarKey {
                alpha: 0.8,
                value: 8.0,
                variance: 0.0,
            },
        ],
        cycles: 0.0,
        looping: false,
    };
    assert!(sample_scalar_progression(&progression, 0.0, 0.0).abs() < 0.000_01);
    assert!((sample_scalar_progression(&progression, 0.5, 0.0) - 5.0).abs() < 0.000_01);
    assert_eq!(
        sample_scalar_progression(&progression, 0.9, 0.0).to_bits(),
        1.0_f32.to_bits()
    );

    let looping = ParticleScalarProgression {
        looping: true,
        ..progression
    };
    assert!((sample_scalar_progression(&looping, 0.75, 0.0) - 7.5).abs() < 0.000_01);
}

#[test]
fn color_progression_and_palette_use_the_retail_shader_selection() {
    let progression = ParticleColorProgression {
        keys: vec![
            ParticleColorKey {
                alpha: 0.0,
                color: [0.0, 0.0, 0.0, 0.0],
            },
            ParticleColorKey {
                alpha: 0.5,
                color: [1.0, 0.0, 0.0, 1.0],
            },
        ],
        cycles: 0.0,
        looping: false,
    };
    assert_eq!(
        sample_color_progression(&progression, 0.75).map(f32::to_bits),
        [1.0; 4].map(f32::to_bits)
    );

    let palette = ParticleColorDefinition {
        kind: ParticleColorKind::Palette,
        palette: [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ]
        .into_iter()
        .map(|color| ParticlePaletteEntry { color, weight: 0.0 })
        .collect(),
        ..ParticleColorDefinition::default()
    };
    assert_eq!(
        sample_color(&palette, 0.0, -0.4).map(f32::to_bits),
        [0.0, 1.0, 0.0, 1.0].map(f32::to_bits)
    );
}
