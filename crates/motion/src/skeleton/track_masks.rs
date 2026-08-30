//! Granny model-track masks used to combine action and movement poses.

use pipeline::ugx::types::{GrannyBone, GrannyVariant};

const UPPER_BODY_MASK: &str = "TrackMaskUpperBody";

/// Resolve Granny's inherited per-bone upper-body blend weights.
#[must_use]
pub fn upper_body_weights(bones: &[GrannyBone]) -> Vec<f32> {
    let mut weights = Vec::with_capacity(bones.len());
    for (index, bone) in bones.iter().enumerate() {
        let inherited = usize::try_from(bone.parent_index)
            .ok()
            .filter(|&parent| parent < index)
            .and_then(|parent| weights.get(parent))
            .copied()
            .unwrap_or_default();
        let weight = find_weight(bone.extended_data.as_ref()).unwrap_or(inherited);
        weights.push(finite_weight(weight));
    }
    weights
}

fn find_weight(value: Option<&GrannyVariant>) -> Option<f32> {
    let GrannyVariant::Struct(fields) = value? else {
        return None;
    };
    fields.iter().find_map(|(name, value)| {
        if !name.eq_ignore_ascii_case(UPPER_BODY_MASK) {
            return None;
        }
        let GrannyVariant::Real32(values) = value else {
            return None;
        };
        values.first().copied()
    })
}

fn finite_weight(weight: f32) -> f32 {
    if weight.is_finite() {
        weight.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use pipeline::ugx::types::{GrannyBone, GrannyVariant};

    use super::upper_body_weights;

    fn bone(parent_index: i32, weight: Option<f32>) -> GrannyBone {
        GrannyBone {
            parent_index,
            extended_data: weight.map(|weight| {
                GrannyVariant::Struct(vec![(
                    "TrackMaskUpperBody".to_owned(),
                    GrannyVariant::Real32(vec![weight]),
                )])
            }),
            ..GrannyBone::default()
        }
    }

    #[test]
    fn missing_weights_inherit_from_the_parent_like_granny() {
        let bones = vec![
            bone(-1, None),
            bone(0, Some(1.0)),
            bone(1, None),
            bone(0, None),
        ];
        assert_eq!(upper_body_weights(&bones), vec![0.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn malformed_weights_use_a_stable_renderer_range() {
        let bones = vec![bone(-1, Some(f32::NAN)), bone(0, Some(2.0))];
        assert_eq!(upper_body_weights(&bones), vec![0.0, 1.0]);
    }
}
