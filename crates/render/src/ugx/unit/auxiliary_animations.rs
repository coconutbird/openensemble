//! Component-local simulation clocks for independent turret animations.

use super::ik::UnitAnimationFrame;
use super::{Unit, UnitAttachment, UnitAttachmentTrigger, UnitRenderer};

#[derive(Clone, Debug, Default)]
pub(super) enum ActionAnimationClock {
    #[default]
    Placement,
    Auxiliary {
        action_name: String,
        duration_seconds: f32,
    },
}

impl ActionAnimationClock {
    pub(super) fn position(&self, frame: &UnitAnimationFrame<'_>) -> Option<f32> {
        match self {
            Self::Placement => frame.simulation_position,
            Self::Auxiliary {
                action_name,
                duration_seconds,
            } => frame
                .world
                .get_unit(frame.entity_id)?
                .turret_combat
                .attacks()
                .find(|attack| attack.action_name().eq_ignore_ascii_case(action_name))?
                .animation_position(*duration_seconds),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::ugx) struct AuxiliaryAnimationRequest<'animation> {
    pub(in crate::ugx) action_name: &'animation str,
    pub(in crate::ugx) component: &'animation str,
    pub(in crate::ugx) animation_type: &'animation str,
    pub(in crate::ugx) animation_asset: &'animation str,
    pub(in crate::ugx) duration_seconds: f32,
    pub(in crate::ugx) animation_roll: u64,
}

#[derive(Clone, Debug)]
pub(super) struct OwnedAuxiliaryAnimationRequest {
    pub(super) action_name: String,
    pub(super) component: String,
    pub(super) animation_type: String,
    pub(super) animation_asset: String,
    pub(super) duration_seconds: f32,
    pub(super) animation_roll: u64,
}

impl From<AuxiliaryAnimationRequest<'_>> for OwnedAuxiliaryAnimationRequest {
    fn from(request: AuxiliaryAnimationRequest<'_>) -> Self {
        Self {
            action_name: request.action_name.to_owned(),
            component: request.component.to_owned(),
            animation_type: request.animation_type.to_owned(),
            animation_asset: request.animation_asset.to_owned(),
            duration_seconds: request.duration_seconds,
            animation_roll: request.animation_roll,
        }
    }
}

pub(super) fn selected_request_index(
    requests: &[OwnedAuxiliaryAnimationRequest],
    component: &str,
    inherited: Option<usize>,
) -> Option<usize> {
    requests
        .iter()
        .position(|request| request.component.eq_ignore_ascii_case(component))
        .or_else(|| inherited.filter(|index| *index < requests.len()))
}

pub(super) fn loaded_attachment_matches(
    unit: &Unit,
    attachment: &UnitAttachment,
    action_animation: Option<&str>,
    movement_animation: Option<&str>,
) -> bool {
    if attachment
        .trigger
        .matches_animations(action_animation, movement_animation)
    {
        return true;
    }
    let UnitAttachmentTrigger::Animation(expected) = &attachment.trigger else {
        return true;
    };
    unit.instances
        .iter()
        .filter(|instance| instance.name.eq_ignore_ascii_case(&attachment.component))
        .any(|instance| {
            instance
                .action_animation
                .segments
                .iter()
                .any(|segment| segment.animation_type.eq_ignore_ascii_case(expected))
                || instance.movement_animation.as_ref().is_some_and(|track| {
                    track
                        .segments
                        .iter()
                        .any(|segment| segment.animation_type.eq_ignore_ascii_case(expected))
                })
        })
}

pub(super) fn rendered_attachment_matches(
    renderer: &UnitRenderer,
    attachment: &UnitAttachment,
    action_animation: Option<&str>,
    movement_animation: Option<&str>,
) -> bool {
    if matches!(attachment.trigger, UnitAttachmentTrigger::Persistent) {
        return true;
    }
    let UnitAttachmentTrigger::Animation(expected) = &attachment.trigger else {
        return false;
    };
    renderer
        .instances
        .iter()
        .filter(|instance| instance.name.eq_ignore_ascii_case(&attachment.component))
        .any(|instance| {
            instance
                .action_animation
                .selected_animation_type()
                .is_some_and(|active| active.eq_ignore_ascii_case(expected))
                || instance
                    .movement_animation
                    .as_ref()
                    .and_then(super::RenderedAnimationTrack::selected_animation_type)
                    .is_some_and(|active| active.eq_ignore_ascii_case(expected))
        })
        || attachment
            .trigger
            .matches_animations(action_animation, movement_animation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(component: &str) -> OwnedAuxiliaryAnimationRequest {
        OwnedAuxiliaryAnimationRequest {
            action_name: component.to_owned(),
            component: component.to_owned(),
            animation_type: "Attack".to_owned(),
            animation_asset: "attack".to_owned(),
            duration_seconds: 1.0,
            animation_roll: 0,
        }
    }

    #[test]
    fn explicit_nested_turret_overrides_an_inherited_component_track() {
        let requests = [request("parent"), request("child")];

        assert_eq!(selected_request_index(&requests, "other", Some(0)), Some(0));
        assert_eq!(selected_request_index(&requests, "CHILD", Some(0)), Some(1));
    }
}
