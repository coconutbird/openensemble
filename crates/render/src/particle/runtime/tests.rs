use glam::Mat4;
use pipeline::xmb::Document;

use super::random::RetailRandom;
use super::shape::emission_position;
use super::{ParticleEmitterRuntime, ParticleEmitterState, ParticleNestedEvent};
use crate::particle::{
    ParticleEffect, ParticleEmitterShape, ParticleEmitterShapeKind, ParticleMaterial,
    ParticleRenderContext,
};

fn runtime_from_emitter(emitter_data: &str, extra: &str) -> ParticleEmitterRuntime {
    let document = Document::from_xml(&format!(
        "<ParticleEffect><ParticleEmitter><EmitterData>{emitter_data}</EmitterData>{extra}</ParticleEmitter></ParticleEffect>"
    ))
    .expect("test XML should parse");
    let mut effect = ParticleEffect::from_document(&document).expect("test effect should adapt");
    ParticleEmitterRuntime::new(effect.emitters.remove(0), 7, Mat4::IDENTITY)
}

#[test]
fn regular_emission_starts_immediately_and_obeys_the_authored_rate() {
    let mut runtime = runtime_from_emitter(
        "<MaxParticles>20</MaxParticles><EmissionRate>10</EmissionRate><EmissionTime>1</EmissionTime><ParticleLife>2</ParticleLife>",
        "",
    );
    runtime.update(0.25, Mat4::IDENTITY, Mat4::IDENTITY);
    assert_eq!(runtime.live_particle_count(), 3);
    assert_eq!(runtime.state(), ParticleEmitterState::Active);
}

#[test]
fn dormant_start_delay_is_consumed_before_emission() {
    let mut runtime = runtime_from_emitter(
        "<EmissionRate>10</EmissionRate><StartDelay>0.2</StartDelay><EmissionTime>1</EmissionTime><ParticleLife>2</ParticleLife>",
        "",
    );
    runtime.update(0.1, Mat4::IDENTITY, Mat4::IDENTITY);
    assert_eq!(runtime.live_particle_count(), 0);
    runtime.update(0.11, Mat4::IDENTITY, Mat4::IDENTITY);
    assert_eq!(runtime.live_particle_count(), 1);
}

#[test]
fn beam_tessellation_one_is_repaired_to_one_visible_segment() {
    let mut runtime = runtime_from_emitter(
        "<ParticleType>eBeam</ParticleType><BeamTesselation>1</BeamTesselation><EmissionTime>1</EmissionTime><ParticleLife>2</ParticleLife>",
        "<ScaleData><ValueX>2</ValueX><ValueY>3</ValueY><UniformValue>1</UniformValue></ScaleData><OpacityData><Value>1</Value></OpacityData><IntensityData><Value>1</Value></IntensityData>",
    );
    runtime.update(0.01, Mat4::IDENTITY, Mat4::from_translation(glam::Vec3::Y));
    let instances = runtime.instances(
        &ParticleMaterial::default(),
        ParticleRenderContext::default(),
    );
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].half_length.to_bits(), 0.5_f32.to_bits());
}

#[test]
fn nested_effects_report_spawn_transform_and_release() {
    let mut runtime = runtime_from_emitter(
        "<ParticleType>ePFX</ParticleType><EmissionRate>1</EmissionRate><EmissionTime>0.1</EmissionTime><ParticleLife>0.05</ParticleLife>",
        "<PFXData><PFXFilePath>effects/child</PFXFilePath></PFXData>",
    );
    runtime.update(0.01, Mat4::IDENTITY, Mat4::IDENTITY);
    assert!(matches!(
        runtime.take_nested_events().first(),
        Some(ParticleNestedEvent::Spawn { path, .. }) if path == "effects/child"
    ));
    runtime.update(0.1, Mat4::IDENTITY, Mat4::IDENTITY);
    assert!(
        runtime
            .take_nested_events()
            .iter()
            .any(|event| matches!(event, ParticleNestedEvent::Release { .. }))
    );
}

#[test]
fn circle_surface_and_volume_semantics_are_not_reversed() {
    let mut random = RetailRandom::from_seed(11);
    let mut shape = ParticleEmitterShape {
        kind: ParticleEmitterShapeKind::Circle,
        size: [4.0, 0.0, 0.0],
        emit_from_surface: true,
        ..ParticleEmitterShape::default()
    };
    for _ in 0..32 {
        let point = emission_position(&shape, &mut random);
        assert!((point.length() - 4.0).abs() < 1.0e-4);
    }
    shape.emit_from_surface = false;
    assert!((0..32).any(|_| emission_position(&shape, &mut random).length() < 3.0));
}
