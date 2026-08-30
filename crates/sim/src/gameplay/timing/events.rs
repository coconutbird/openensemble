//! Simulation-owned animation events retained in authored timeline order.

use glam::Mat4;
use num_traits::ToPrimitive;
use pipeline::database::hw1::visual::VisualTag;

/// One authoritative event on an exact attack-animation asset.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackAnimationEvent {
    /// Normalized position within the UAX clip.
    pub position: f32,
    /// Simulation behavior selected by the visual tag type.
    pub kind: AttackAnimationEventKind,
    /// Posed component path and bone frame resolved from shipped visual assets.
    pub anchor: Option<AttackAnimationAnchor>,
}

impl AttackAnimationEvent {
    pub(super) fn attack_position(&self) -> Option<f32> {
        matches!(self.kind, AttackAnimationEventKind::Attack { .. }).then_some(self.position)
    }
}

/// Authoritative visual-tag kinds consumed by deterministic simulation.
#[derive(Debug, Clone, PartialEq)]
pub enum AttackAnimationEventKind {
    /// Launch one weapon hit/projectile from the optional posed bone.
    Attack {
        /// Bone carrying the launch point on the tagged component.
        to_bone: Option<String>,
    },
    /// Apply recoil or another rigid-body impulse from a posed bone frame.
    PhysicsImpulse(PhysicsImpulseEvent),
}

/// Authored `PhysicsImpulse` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsImpulseEvent {
    /// Bone whose live orientation transforms the local force vector.
    pub to_bone: Option<String>,
    /// Zero is angular, one linear, and two point impulse.
    pub impulse_type: u8,
    /// Local X/Y/Z force values; the visual schema stores Z in `lifespan`.
    pub force: [f32; 3],
    /// Retail's overloaded `checkSelected` bit redirects to the attached-to body.
    pub attached_to_object: bool,
}

/// Posed path from the unit's root component to one animation-event bone.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackAnimationAnchor {
    /// Attachment links in parent-to-child order.
    pub links: Vec<AttackAttachmentPose>,
    /// Posed event bone in the final component, or its component root when absent.
    pub bone_to_component: Option<Mat4>,
    /// Single-bone IK ancestor frames that can move the posed event bone.
    pub single_bone_poses: Vec<AttackSingleBonePose>,
}

/// Posed hierarchy frames needed to move an event with retail `SingleBoneIK`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackSingleBonePose {
    /// Authored `SingleBoneIK` node name.
    pub bone: String,
    /// Posed transform of the bone's parent into component space.
    pub parent_to_component: Mat4,
    /// Posed local transform before live IK is applied.
    pub local_transform: Mat4,
    /// Posed transform from the IK bone to the event bone.
    pub bone_to_event: Mat4,
}

/// One authored component attachment sampled at an event's animation position.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackAttachmentPose {
    /// Child model name; hardpoint definitions address attachment components by this name.
    pub child_component: String,
    /// Posed target bone on the parent component.
    pub to_bone: Option<Mat4>,
    /// Posed source bone on the child component.
    pub from_bone: Option<Mat4>,
    /// Whether retail discards the resolved attachment orientation.
    pub disregard_orientation: bool,
}

impl AttackAnimationAnchor {
    /// Resolve the event frame into unit-local space with mutable hardpoint transforms.
    #[must_use]
    pub fn unit_transform(&self, attachment_transform: impl Fn(&str) -> Option<Mat4>) -> Mat4 {
        self.unit_transform_with_hardpoints(attachment_transform, |_| None)
    }

    /// Resolve the event using both mutable attachment and single-bone transforms.
    #[must_use]
    pub fn unit_transform_with_hardpoints(
        &self,
        attachment_transform: impl Fn(&str) -> Option<Mat4>,
        bone_transform: impl Fn(&str) -> Option<Mat4>,
    ) -> Mat4 {
        let component = self.links.iter().fold(Mat4::IDENTITY, |parent, link| {
            parent
                * motion::transformed_attachment_transform(
                    link.to_bone,
                    link.from_bone,
                    attachment_transform(&link.child_component),
                    link.disregard_orientation,
                )
        });
        let event = self
            .single_bone_poses
            .iter()
            .find_map(|pose| {
                bone_transform(&pose.bone).map(|dynamic| {
                    pose.parent_to_component
                        * motion::premultiplied_orientation_transform(pose.local_transform, dynamic)
                        * pose.bone_to_event
                })
            })
            .or(self.bone_to_component)
            .unwrap_or(Mat4::IDENTITY);
        component * event
    }
}

pub(super) fn simulation_events(tags: &[VisualTag]) -> Vec<AttackAnimationEvent> {
    let mut events = tags
        .iter()
        .enumerate()
        .filter_map(|(authored_index, tag)| {
            let position = finite_position(tag.position);
            let kind = if tag.tag_type.eq_ignore_ascii_case("Attack") {
                Some(AttackAnimationEventKind::Attack {
                    to_bone: nonempty(tag.to_bone.as_deref()),
                })
            } else if tag.tag_type.eq_ignore_ascii_case("PhysicsImpulse") {
                Some(AttackAnimationEventKind::PhysicsImpulse(physics_impulse(
                    tag,
                )))
            } else {
                None
            }?;
            Some((
                authored_index,
                AttackAnimationEvent {
                    position,
                    kind,
                    anchor: None,
                },
            ))
        })
        .collect::<Vec<_>>();
    events.sort_by(|left, right| {
        left.1
            .position
            .total_cmp(&right.1.position)
            .then_with(|| left.0.cmp(&right.0))
    });
    events.into_iter().map(|(_, event)| event).collect()
}

fn physics_impulse(tag: &VisualTag) -> PhysicsImpulseEvent {
    PhysicsImpulseEvent {
        to_bone: nonempty(tag.to_bone.as_deref()),
        impulse_type: finite_value(tag.start)
            .round()
            .clamp(0.0, f32::from(u8::MAX))
            .to_u8()
            .unwrap_or_default(),
        force: [
            finite_value(tag.force),
            finite_value(tag.force2),
            finite_value(tag.lifespan),
        ],
        attached_to_object: tag.check_selected.unwrap_or(false),
    }
}

fn finite_position(value: Option<f32>) -> f32 {
    finite_value(value).clamp(0.0, 1.0)
}

fn finite_value(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};
    use pipeline::database::hw1::visual::VisualTag;

    use super::{
        AttackAnimationAnchor, AttackAnimationEventKind, AttackSingleBonePose, simulation_events,
    };

    #[test]
    fn simulation_events_are_sorted_stably_by_authored_position() {
        let tags = vec![
            VisualTag {
                tag_type: "PhysicsImpulse".to_owned(),
                position: Some(0.08),
                start: Some(1.0),
                force: Some(-4.0),
                force2: Some(-3.0),
                ..VisualTag::default()
            },
            VisualTag {
                tag_type: "Attack".to_owned(),
                position: Some(0.01),
                to_bone: Some(" launch ".to_owned()),
                ..VisualTag::default()
            },
            VisualTag {
                tag_type: "PhysicsImpulse".to_owned(),
                position: Some(0.01),
                start: Some(0.0),
                force: Some(-6.0),
                ..VisualTag::default()
            },
        ];

        let events = simulation_events(&tags);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].position.to_bits(), 0.01_f32.to_bits());
        assert!(matches!(
            events[0].kind,
            AttackAnimationEventKind::Attack { ref to_bone }
                if to_bone.as_deref() == Some("launch")
        ));
        assert!(matches!(
            events[1].kind,
            AttackAnimationEventKind::PhysicsImpulse(_)
        ));
        let AttackAnimationEventKind::PhysicsImpulse(impulse) = &events[2].kind else {
            panic!("expected trailing linear impulse");
        };
        assert_eq!(impulse.impulse_type, 1);
        assert!(
            glam::Vec3::from_array(impulse.force)
                .abs_diff_eq(glam::Vec3::new(-4.0, -3.0, 0.0), f32::EPSILON)
        );
    }

    #[test]
    fn event_anchor_follows_a_live_single_bone_ancestor() {
        let parent = Mat4::from_translation(Vec3::Y * 2.0);
        let mut local = Mat4::from_rotation_x(0.25);
        local.w_axis = Vec3::X.extend(1.0);
        let bone_to_event = Mat4::from_translation(Vec3::Z * 3.0);
        let anchor = AttackAnimationAnchor {
            links: Vec::new(),
            bone_to_component: Some(parent * local * bone_to_event),
            single_bone_poses: vec![AttackSingleBonePose {
                bone: "spine".to_owned(),
                parent_to_component: parent,
                local_transform: local,
                bone_to_event,
            }],
        };
        let dynamic = Mat4::from_rotation_y(0.5);

        let transformed = anchor.unit_transform_with_hardpoints(
            |_| None,
            |bone| bone.eq_ignore_ascii_case("spine").then_some(dynamic),
        );
        let expected =
            parent * motion::premultiplied_orientation_transform(local, dynamic) * bone_to_event;

        assert!(transformed.abs_diff_eq(expected, 1.0e-5));
    }
}
