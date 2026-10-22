//! Retail evaluation of authored PFX values, progressions and colors.

use num_traits::ToPrimitive;
use pipeline::database::hw1::pfx::{
    ParticleColorDefinition, ParticleColorKind, ParticleColorProgression, ParticleScalarKey,
    ParticleScalarProgression, ParticleScalarProperty, ParticleVarying, ParticleVectorProperty,
};

#[cfg(test)]
mod tests;

/// Applies the retail multiplicative variance equation.
pub(super) fn sample_varying(varying: ParticleVarying, signed_random: f32) -> f32 {
    varying.value * signed_random.mul_add(varying.variance, 1.0)
}

/// Samples the retail piecewise-linear progression.
pub(super) fn sample_scalar_progression(
    progression: &ParticleScalarProgression,
    alpha: f32,
    signed_random: f32,
) -> f32 {
    let alpha = progression_alpha(alpha, progression.looping, progression.cycles);
    let Some((left, right)) = progression_span(&progression.keys, alpha, |key| key.alpha) else {
        return 1.0;
    };
    let left_value = varying_key(left, signed_random);
    let right_value = varying_key(right, signed_random);
    lerp_span(left.alpha, right.alpha, alpha, left_value, right_value)
}

/// Evaluates a scalar property with independent per-particle and progression variance samples.
pub(super) fn sample_scalar_property(
    property: &ParticleScalarProperty,
    alpha: f32,
    signed_random: f32,
    progression_random: f32,
) -> f32 {
    let base = sample_varying(property.base, signed_random);
    if property.use_progression {
        base * sample_scalar_progression(&property.progression, alpha, progression_random)
    } else {
        base
    }
}

/// Samples X/Y/Z with independent per-particle and progression variance.
///
/// The result includes the authored uniform multiplier.
pub(super) fn sample_vector_property(
    property: &ParticleVectorProperty,
    alpha: f32,
    signed_randoms: [f32; 4],
    progression_random: f32,
) -> [f32; 3] {
    let uniform = sample_value(property.value[3], property.variance[3], signed_randoms[3]);
    std::array::from_fn(|axis| {
        let base = sample_value(
            property.value[axis],
            property.variance[axis],
            signed_randoms[axis],
        );
        let progression = if property.use_progression[axis] {
            sample_scalar_progression(&property.progression[axis], alpha, progression_random)
        } else {
            1.0
        };
        base * uniform * progression
    })
}

/// Samples the authored RGBA gradient.
pub(super) fn sample_color_progression(
    progression: &ParticleColorProgression,
    alpha: f32,
) -> [f32; 4] {
    let alpha = progression_alpha(alpha, progression.looping, progression.cycles);
    let Some((left, right)) = progression_span(&progression.keys, alpha, |key| key.alpha) else {
        return [1.0; 4];
    };
    std::array::from_fn(|channel| {
        lerp_span(
            left.alpha,
            right.alpha,
            alpha,
            left.color[channel],
            right.color[channel],
        )
    })
}

/// Samples the source color before player/sun tint and opacity.
pub(super) fn sample_color(
    definition: &ParticleColorDefinition,
    alpha: f32,
    signed_random: f32,
) -> [f32; 4] {
    match definition.kind {
        ParticleColorKind::Single => definition.color,
        ParticleColorKind::Progression => sample_color_progression(&definition.progression, alpha),
        ParticleColorKind::Palette => palette_color(definition, signed_random),
    }
}

fn palette_color(definition: &ParticleColorDefinition, signed_random: f32) -> [f32; 4] {
    if definition.palette.is_empty() {
        return [1.0; 4];
    }
    let unit = signed_random.mul_add(0.5, 0.5).clamp(0.0, 1.0);
    let count = definition.palette.len().min(8);
    let max_index = (count - 1).to_f32().unwrap_or(7.0);
    let index = unit
        .mul_add(max_index, 0.5)
        .floor()
        .to_usize()
        .unwrap_or(count - 1)
        .min(count - 1);
    definition.palette[index].color
}

fn progression_span<T>(keys: &[T], alpha: f32, key_alpha: impl Fn(&T) -> f32) -> Option<(&T, &T)> {
    for pair in keys.windows(2) {
        if key_alpha(&pair[1]) >= alpha {
            return Some((&pair[0], &pair[1]));
        }
    }
    None
}

fn progression_alpha(alpha: f32, looping: bool, cycles: f32) -> f32 {
    let alpha = alpha.clamp(0.0, 1.0);
    if !looping {
        return alpha;
    }
    let cycles = if cycles > 0.000_01 { cycles } else { 1.0 };
    (alpha * cycles) % 1.0
}

fn varying_key(key: &ParticleScalarKey, signed_random: f32) -> f32 {
    sample_value(key.value, key.variance, signed_random)
}

fn sample_value(value: f32, variance: f32, signed_random: f32) -> f32 {
    value * signed_random.mul_add(variance, 1.0)
}

fn lerp_span(left_alpha: f32, right_alpha: f32, alpha: f32, left: f32, right: f32) -> f32 {
    let length = right_alpha - left_alpha;
    let local = if length.abs() > f32::EPSILON {
        (alpha - left_alpha) / length
    } else {
        0.0
    };
    left + (right - left) * local
}
