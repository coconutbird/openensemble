//! CPU evaluation of particle shader inputs.

use glam::{Mat4, Vec3, Vec4};
use num_traits::ToPrimitive;

use super::ParticleRenderContext;
use super::emitter::RuntimeParticle;
use crate::particle::{
    ParticleEmitter, ParticleGeometry, ParticleInstance, ParticleMaterial, ParticleTextureArray,
    ParticleTextureDefinition,
};

pub(super) struct AppearanceSample {
    pub(super) color: [f32; 4],
    pub(super) intensity: [f32; 4],
    pub(super) scale: [f32; 3],
    pub(super) uv_rects: [[f32; 4]; 4],
}

#[derive(Clone, Copy)]
pub(super) struct SegmentDescriptor {
    pub(super) start: Vec3,
    pub(super) end: Vec3,
    pub(super) distance_alpha: Option<f32>,
    pub(super) stretch: Option<[f32; 2]>,
}

pub(super) fn particle_instance(
    emitter: &ParticleEmitter,
    particle: &RuntimeParticle,
    current_time: f32,
    transform: Mat4,
    material: &ParticleMaterial,
    context: ParticleRenderContext,
) -> Option<ParticleInstance> {
    let geometry = match emitter.kind {
        crate::particle::ParticleEmitterKind::Render(geometry) => geometry,
        crate::particle::ParticleEmitterKind::NestedEffect(_) => return None,
    };
    let alpha = particle.life_alpha(current_time);
    let appearance = sample(emitter, particle, alpha, None, material, context);
    let (mut position, axis, up_axis) = world_vectors(emitter, particle, transform);
    if geometry == ParticleGeometry::TerrainPatch {
        position.y += emitter.runtime.timing.terrain_y_offset;
    }
    Some(ParticleInstance {
        position: position.to_array(),
        rotation: particle.rotation,
        axis: axis.to_array(),
        up_axis: up_axis.to_array(),
        half_length: appearance.scale[1].abs() * 0.5,
        size: [appearance.scale[0], appearance.scale[1]],
        color: appearance.color,
        intensity: appearance.intensity,
        uv_rects: appearance.uv_rects,
        texture_layers: particle.texture_layers,
        geometry,
        soft_fade_scale: emitter.material.soft_fade_scale,
    })
}

pub(super) fn segment_instance(
    emitter: &ParticleEmitter,
    particle: &RuntimeParticle,
    current_time: f32,
    segment: SegmentDescriptor,
    material: &ParticleMaterial,
    context: ParticleRenderContext,
) -> Option<ParticleInstance> {
    let geometry = match emitter.kind {
        crate::particle::ParticleEmitterKind::Render(geometry) => geometry,
        crate::particle::ParticleEmitterKind::NestedEffect(_) => return None,
    };
    let alpha = particle.life_alpha(current_time);
    let mut appearance = sample(
        emitter,
        particle,
        alpha,
        segment.distance_alpha,
        material,
        context,
    );
    if let Some([u0, u1]) = segment.stretch {
        for rect in &mut appearance.uv_rects {
            let offset = rect[0];
            rect[0] = offset + u0;
            rect[2] = offset + u1;
        }
    }
    let mut instance = ParticleInstance::segment(
        segment.start.to_array(),
        segment.end.to_array(),
        appearance.scale[0],
        appearance.color,
        geometry,
    );
    instance.rotation = particle.rotation;
    instance.size = [appearance.scale[0], appearance.scale[1]];
    instance.up_axis = particle.up_axis.to_array();
    instance.intensity = appearance.intensity;
    instance.uv_rects = appearance.uv_rects;
    instance.texture_layers = particle.texture_layers;
    instance.soft_fade_scale = emitter.material.soft_fade_scale;
    Some(instance)
}

fn sample(
    emitter: &ParticleEmitter,
    particle: &RuntimeParticle,
    life_alpha: f32,
    distance_alpha: Option<f32>,
    material: &ParticleMaterial,
    context: ParticleRenderContext,
) -> AppearanceSample {
    let runtime = &emitter.runtime;
    let random = particle.random_values;
    let progression_random = particle.progression_random;
    let color_alpha = selector(
        life_alpha,
        distance_alpha,
        runtime.timing.beam_color_by_length(),
    );
    let opacity_alpha = selector(
        life_alpha,
        distance_alpha,
        runtime.timing.beam_opacity_by_length(),
    );
    let intensity_alpha = selector(
        life_alpha,
        distance_alpha,
        runtime.timing.beam_intensity_by_length(),
    );
    let source = runtime.color.sample(color_alpha, random[0]);
    let opacity = runtime.opacity.sample_with_progression_random(
        opacity_alpha,
        random[3],
        progression_random,
    );
    let color = tint_color(emitter, source, opacity, context);
    let intensity = runtime.intensity.sample_with_progression_random(
        intensity_alpha,
        random[0],
        progression_random,
    );
    let mut scale = runtime.scale.sample_with_progression_random(
        life_alpha,
        [random[1], random[2], random[0], random[3]],
        progression_random,
    );
    if let Some(distance) = distance_alpha
        && runtime.scale.use_progression[1]
    {
        let distance_scale = runtime.scale.progression[1].sample(distance, progression_random);
        scale = scale.map(|value| value * distance_scale);
    }
    AppearanceSample {
        color,
        intensity: [intensity, intensity, intensity, 1.0],
        scale,
        uv_rects: uv_rects(emitter, particle, material),
    }
}

fn tint_color(
    emitter: &ParticleEmitter,
    source: [f32; 4],
    opacity: f32,
    context: ParticleRenderContext,
) -> [f32; 4] {
    let definition = &emitter.runtime.color;
    let mut tint = Vec4::ONE;
    if definition.player_color {
        let intensity = definition.player_color_intensity;
        tint *= Vec4::from_array(context.player_color)
            * Vec4::new(intensity, intensity, intensity, 1.0);
    }
    if definition.sun_color {
        let intensity = definition.sun_color_intensity;
        tint *=
            Vec4::from_array(context.sun_color) * Vec4::new(intensity, intensity, intensity, 1.0);
    }
    let rgb = Vec3::from_array([source[0], source[1], source[2]]) * tint.truncate();
    [
        rgb.x,
        rgb.y,
        rgb.z,
        opacity * context.emitter_opacity * tint.w,
    ]
}

fn uv_rects(
    emitter: &ParticleEmitter,
    particle: &RuntimeParticle,
    material: &ParticleMaterial,
) -> [[f32; 4]; 4] {
    let definitions = [
        &emitter.material.diffuse[0],
        &emitter.material.diffuse[1],
        &emitter.material.diffuse[2],
        &emitter.material.intensity,
    ];
    let textures = [
        material.diffuse[0].as_ref(),
        material.diffuse[1].as_ref(),
        material.diffuse[2].as_ref(),
        material.intensity.as_ref(),
    ];
    std::array::from_fn(|index| {
        uv_rect(
            definitions[index],
            textures[index],
            particle.age_seconds,
            particle.random_values[0],
        )
    })
}

fn uv_rect(
    definition: &ParticleTextureDefinition,
    texture: Option<&ParticleTextureArray>,
    age_seconds: f32,
    signed_random: f32,
) -> [f32; 4] {
    let animation = definition.uv_animation;
    if animation.enabled
        && animation.frame_width > f32::EPSILON
        && animation.frame_height > f32::EPSILON
        && let Some(texture) = texture
    {
        return animated_rect(definition, texture, age_seconds);
    }
    let random_u = if animation.random_scroll_u {
        signed_random
    } else {
        0.0
    };
    let random_v = if animation.random_scroll_v {
        signed_random
    } else {
        0.0
    };
    let u = animation.scroll_u.mul_add(age_seconds, random_u);
    let v = animation.scroll_v.mul_add(age_seconds, random_v);
    [u, v, u + 1.0, v + 1.0]
}

fn animated_rect(
    definition: &ParticleTextureDefinition,
    texture: &ParticleTextureArray,
    age_seconds: f32,
) -> [f32; 4] {
    let animation = definition.uv_animation;
    let dimensions = texture
        .dimensions()
        .map(|value| value.to_f32().unwrap_or(1.0));
    let frame_width = animation.frame_width.min(dimensions[0]);
    let frame_height = animation.frame_height.min(dimensions[1]);
    let columns = (dimensions[0] / frame_width).floor().max(1.0);
    let rows = (dimensions[1] / frame_height).floor().max(1.0);
    let total = columns * rows;
    let frame = (age_seconds.max(0.0) * animation.frames_per_second)
        .floor()
        .rem_euclid(total);
    let column = frame.rem_euclid(columns);
    let row = (frame / columns).floor();
    let width = frame_width / dimensions[0];
    let height = frame_height / dimensions[1];
    let u = column * width;
    let v = row * height;
    [u, v, u + width, v + height]
}

fn world_vectors(
    emitter: &ParticleEmitter,
    particle: &RuntimeParticle,
    transform: Mat4,
) -> (Vec3, Vec3, Vec3) {
    if emitter.runtime.timing.tied_to_emitter() {
        (
            transform.transform_point3(particle.position),
            transform
                .transform_vector3(particle.velocity)
                .normalize_or_zero(),
            transform
                .transform_vector3(particle.up_axis)
                .normalize_or_zero(),
        )
    } else {
        (
            particle.position,
            particle.velocity.normalize_or_zero(),
            particle.up_axis.normalize_or_zero(),
        )
    }
}

fn selector(life: f32, distance: Option<f32>, by_length: bool) -> f32 {
    if by_length {
        distance.unwrap_or(life)
    } else {
        life
    }
}
