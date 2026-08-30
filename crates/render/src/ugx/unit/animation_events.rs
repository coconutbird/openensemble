//! Renderer-only animation-tag crossing state.

use glam::{Mat4, Vec2};
use pipeline::database::hw1::visual::VisualTag;

const POSITION_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Debug)]
pub(in crate::ugx) struct UnitAnimationEvent {
    pub(in crate::ugx) kind: UnitAnimationEventKind,
    pub(in crate::ugx) transform: Mat4,
    pub(in crate::ugx) source_owner_id: u64,
    pub(in crate::ugx) anchor: Option<UnitAnimationAnchor>,
}

#[derive(Clone, Debug)]
pub(in crate::ugx) enum UnitAnimationEventKind {
    TerrainEffect(String),
    Particle {
        path: String,
        lifespan_seconds: f32,
    },
    Light {
        path: String,
        lifespan_seconds: f32,
    },
    CameraShake {
        strength: f32,
        lifespan_seconds: f32,
        check_selected: bool,
    },
    TerrainAlpha {
        shape: UnitTerrainAlphaShape,
        enabled: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ugx) enum UnitTerrainAlphaShape {
    Rectangle { half_extents: Vec2 },
    Circle { radius: f32 },
}

#[derive(Clone, Debug)]
pub(in crate::ugx) struct UnitAnimationAnchor {
    pub(in crate::ugx) instance_index: usize,
    pub(in crate::ugx) to_bone: Option<String>,
    pub(in crate::ugx) disregard_orientation: bool,
}

pub(super) fn camera_shake_event(
    tag: &VisualTag,
    transform: Mat4,
    source_owner_id: u64,
) -> Option<UnitAnimationEvent> {
    if !tag.tag_type.eq_ignore_ascii_case("CameraShake") {
        return None;
    }
    Some(UnitAnimationEvent {
        kind: UnitAnimationEventKind::CameraShake {
            strength: tag
                .force
                .filter(|force| force.is_finite())
                .unwrap_or_default()
                .max(0.0),
            lifespan_seconds: tag
                .lifespan
                .filter(|lifespan| lifespan.is_finite())
                .unwrap_or_default()
                .max(0.0),
            check_selected: tag.check_selected.unwrap_or(false),
        },
        transform,
        source_owner_id,
        anchor: None,
    })
}

pub(super) fn terrain_alpha_event(
    tag: &VisualTag,
    transform: Mat4,
    source_owner_id: u64,
) -> Option<UnitAnimationEvent> {
    if !tag.tag_type.eq_ignore_ascii_case("TerrainAlpha") {
        return None;
    }
    let (shape, enabled) = parse_terrain_alpha(tag.user_data.as_deref()?)?;
    Some(UnitAnimationEvent {
        kind: UnitAnimationEventKind::TerrainAlpha { shape, enabled },
        transform,
        source_owner_id,
        anchor: None,
    })
}

fn parse_terrain_alpha(user_data: &str) -> Option<(UnitTerrainAlphaShape, bool)> {
    let tokens = user_data
        .split(|character: char| {
            character.is_ascii_whitespace() || character == ',' || character == '='
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut size_x = None;
    let mut size_z = None;
    let mut circle = None;
    let mut enabled = None;
    for &[key, value] in tokens.as_chunks::<2>().0 {
        if key.eq_ignore_ascii_case("sizeX") {
            size_x = finite_nonnegative(value);
        } else if key.eq_ignore_ascii_case("sizeZ") {
            size_z = finite_nonnegative(value);
        } else if key.eq_ignore_ascii_case("type") {
            circle = if value.eq_ignore_ascii_case("circle") {
                Some(true)
            } else if value.eq_ignore_ascii_case("rect") || value.eq_ignore_ascii_case("rectangle")
            {
                Some(false)
            } else {
                None
            };
        } else if key.eq_ignore_ascii_case("value") {
            enabled = if value.eq_ignore_ascii_case("on") {
                Some(true)
            } else if value.eq_ignore_ascii_case("off") {
                Some(false)
            } else {
                None
            };
        }
    }
    let size_x = size_x?;
    let shape = if circle? {
        UnitTerrainAlphaShape::Circle { radius: size_x }
    } else {
        UnitTerrainAlphaShape::Rectangle {
            half_extents: Vec2::new(size_x, size_z?),
        }
    };
    Some((shape, enabled?))
}

fn finite_nonnegative(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AnimationEventCursor {
    phase: Option<f32>,
}

impl AnimationEventCursor {
    pub(super) fn advance(&mut self, phase: f32) -> Option<(f32, f32)> {
        if !phase.is_finite() {
            return None;
        }
        let previous = self.phase.replace(phase)?;
        (phase >= previous).then_some((previous, phase))
    }

    pub(super) fn rebaseline(&mut self, phase: f32) {
        self.phase = phase.is_finite().then_some(phase);
    }
}

pub(super) fn crossed_tag(interval: (f32, f32), authored_position: f32) -> bool {
    let (previous, current) = interval;
    if !authored_position.is_finite() || current <= previous {
        return false;
    }
    let position = authored_position.clamp(0.0, 1.0);
    if position < POSITION_EPSILON && previous < POSITION_EPSILON && current >= POSITION_EPSILON {
        return true;
    }
    let occurrence = ((previous - position).floor() + 1.0) + position;
    occurrence <= current
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec2, Vec3};
    use pipeline::database::hw1::visual::VisualTag;

    use super::{
        AnimationEventCursor, UnitAnimationEventKind, UnitTerrainAlphaShape, camera_shake_event,
        crossed_tag, terrain_alpha_event,
    };

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn first_sample_establishes_a_cursor_without_replaying_old_tags() {
        let mut cursor = AnimationEventCursor::default();
        assert_eq!(cursor.advance(0.75), None);
        assert_eq!(cursor.advance(0.8), Some((0.75, 0.8)));
    }

    #[test]
    fn crossing_is_strict_at_the_start_and_inclusive_at_the_end() {
        assert!(crossed_tag((0.1, 0.5), 0.5));
        assert!(!crossed_tag((0.5, 0.9), 0.5));
        assert!(crossed_tag((0.0, 0.1), 0.0));
    }

    #[test]
    fn absolute_loop_phase_detects_wrapped_tags_once_per_update() {
        assert!(crossed_tag((0.9, 1.1), 0.05));
        assert!(crossed_tag((0.9, 2.1), 0.75));
        assert!(!crossed_tag((1.1, 1.4), 0.5));
    }

    #[test]
    fn rewind_rebaselines_without_refiring_tags() {
        let mut cursor = AnimationEventCursor::default();
        cursor.advance(1.0);
        assert_eq!(cursor.advance(0.25), None);
        assert_eq!(cursor.advance(0.5), Some((0.25, 0.5)));
    }

    #[test]
    fn camera_shake_tag_needs_no_named_asset() {
        let transform = Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
        let tag = VisualTag {
            tag_type: "CameraShake".to_owned(),
            name: None,
            force: Some(2.5),
            lifespan: Some(0.75),
            check_selected: Some(true),
            ..VisualTag::default()
        };

        let event = camera_shake_event(&tag, transform, 42).expect("camera shake event");
        assert_eq!(event.transform, transform);
        assert_eq!(event.source_owner_id, 42);
        assert!(event.anchor.is_none());
        match event.kind {
            UnitAnimationEventKind::CameraShake {
                strength,
                lifespan_seconds,
                check_selected,
            } => {
                assert_near(strength, 2.5);
                assert_near(lifespan_seconds, 0.75);
                assert!(check_selected);
            }
            _ => panic!("unexpected animation event"),
        }
    }

    #[test]
    fn terrain_alpha_parses_shipped_key_value_syntax_without_an_asset() {
        let tag = VisualTag {
            tag_type: "TerrainAlpha".to_owned(),
            user_data: Some("sizeX=12 sizeZ=8 value=off type=rect".to_owned()),
            ..VisualTag::default()
        };

        let event = terrain_alpha_event(&tag, Mat4::IDENTITY, 9).expect("terrain alpha event");
        match event.kind {
            UnitAnimationEventKind::TerrainAlpha { shape, enabled } => {
                assert_eq!(
                    shape,
                    UnitTerrainAlphaShape::Rectangle {
                        half_extents: Vec2::new(12.0, 8.0)
                    }
                );
                assert!(!enabled);
            }
            _ => panic!("unexpected animation event"),
        }
    }

    #[test]
    fn invalid_terrain_alpha_values_are_not_projected() {
        let tag = VisualTag {
            tag_type: "TerrainAlpha".to_owned(),
            user_data: Some("sizeX=NaN value=off type=circle".to_owned()),
            ..VisualTag::default()
        };
        assert!(terrain_alpha_event(&tag, Mat4::IDENTITY, 0).is_none());
    }
}
