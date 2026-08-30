//! Resilient authored particle texture-stage selection.

use super::{ParticleError, ParticleImage};

pub(super) fn substitute_failed_particle_layers(
    decoded: Vec<Result<ParticleImage, ParticleError>>,
) -> Result<(Vec<ParticleImage>, usize), ParticleError> {
    let mut first_error = None;
    let mut layers = Vec::with_capacity(decoded.len());
    for result in decoded {
        match result {
            Ok(image) => layers.push(Some(image)),
            Err(error) => {
                first_error.get_or_insert(error);
                layers.push(None);
            }
        }
    }
    let Some(fallback) = layers.iter().flatten().next().cloned() else {
        return Err(first_error.unwrap_or(ParticleError::EmptyTextureArray));
    };
    let fallback_layer_count = layers.iter().filter(|layer| layer.is_none()).count();
    Ok((
        layers
            .into_iter()
            .map(|layer| layer.unwrap_or_else(|| fallback.clone()))
            .collect(),
        fallback_layer_count,
    ))
}
