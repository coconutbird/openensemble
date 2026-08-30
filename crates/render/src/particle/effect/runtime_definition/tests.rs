use pipeline::xmb::Document;

use super::{
    ParticleColorDefinition, ParticleColorKey, ParticleColorKind, ParticleColorProgression,
    ParticlePaletteEntry, ParticleScalarKey, ParticleScalarProgression,
};
use crate::particle::{ParticleEffect, ParticleEmitterShapeKind};

fn effect_from_emitter(emitter: &str) -> ParticleEffect {
    let document = Document::from_xml(&format!(
        "<ParticleEffect><ParticleEmitter>{emitter}</ParticleEmitter></ParticleEffect>"
    ))
    .expect("test XML should decode");
    ParticleEffect::from_document(&document).expect("test effect should adapt")
}

#[test]
fn retail_defaults_and_trail_lifetime_floor_are_preserved() {
    let effect = effect_from_emitter(
        r"<EmitterData>
            <ParticleType>eTrail</ParticleType>
            <ParticleLife>0.01</ParticleLife>
        </EmitterData>",
    );
    let emitter = &effect.emitters[0];
    assert_eq!(emitter.max_particles, 1000);
    assert_eq!(
        emitter.runtime.timing.update_radius.to_bits(),
        10.0_f32.to_bits()
    );
    assert_eq!(
        emitter.runtime.timing.emission_rate.value.to_bits(),
        100.0_f32.to_bits()
    );
    assert_eq!(
        emitter.runtime.timing.particle_life.value.to_bits(),
        0.11_f32.to_bits()
    );
    assert_eq!(
        emitter.runtime.timing.terrain_y_offset.to_bits(),
        0.125_f32.to_bits()
    );
}

#[test]
fn shape_fields_use_the_retail_axis_prefix_layout() {
    let effect = effect_from_emitter(
        r"<EmitterData/><ShapeData>
            <ShapeType>eCylinder</ShapeType>
            <XSize>1</XSize><YSize>2</YSize><ZSize>3</ZSize>
            <XPosOffset>4</XPosOffset><YPosOffset>5</YPosOffset><ZPosOffset>6</ZPosOffset>
            <TrajectoryInnerAngle>50</TrajectoryInnerAngle>
            <TrajectoryOuterAngle>20</TrajectoryOuterAngle>
        </ShapeData>",
    );
    let shape = &effect.emitters[0].runtime.shape;
    assert_eq!(shape.kind, ParticleEmitterShapeKind::Cylinder);
    assert_eq!(
        shape.size.map(f32::to_bits),
        [1.0, 2.0, 3.0].map(f32::to_bits)
    );
    assert_eq!(
        shape.offset.map(f32::to_bits),
        [4.0, 5.0, 6.0].map(f32::to_bits)
    );
    assert_eq!(shape.trajectory_inner_angle.to_bits(), 20.0_f32.to_bits());
    assert_eq!(shape.trajectory_outer_angle.to_bits(), 50.0_f32.to_bits());
}

#[test]
fn beam_tangents_keep_all_authored_axes_instead_of_the_retail_loader_bug() {
    let effect = effect_from_emitter(
        r"<EmitterData>
            <BeamTangent1X>1</BeamTangent1X><BeamTangent1Y>2</BeamTangent1Y>
            <BeamTangent1Z>3</BeamTangent1Z><BeamTangent2X>4</BeamTangent2X>
            <BeamTangent2Y>5</BeamTangent2Y><BeamTangent2Z>6</BeamTangent2Z>
        </EmitterData>",
    );
    let timing = &effect.emitters[0].runtime.timing;
    assert_eq!(
        timing.beam_tangent_1.map(f32::to_bits),
        [1.0, 2.0, 3.0].map(f32::to_bits)
    );
    assert_eq!(
        timing.beam_tangent_2.map(f32::to_bits),
        [4.0, 5.0, 6.0].map(f32::to_bits)
    );
}

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
    assert!(progression.sample(0.0, 0.0).abs() < 0.000_01);
    assert!((progression.sample(0.5, 0.0) - 5.0).abs() < 0.000_01);
    assert_eq!(progression.sample(0.9, 0.0).to_bits(), 1.0_f32.to_bits());

    let looping = ParticleScalarProgression {
        looping: true,
        ..progression
    };
    assert!((looping.sample(0.75, 0.0) - 7.5).abs() < 0.000_01);
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
        progression.sample(0.75).map(f32::to_bits),
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
        palette.sample(0.0, -0.4).map(f32::to_bits),
        [0.0, 1.0, 0.0, 1.0].map(f32::to_bits)
    );
}
