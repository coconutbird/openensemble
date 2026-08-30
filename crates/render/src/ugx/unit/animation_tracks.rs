//! Independent upper-body action and lower-body movement playback state.

use std::sync::Arc;

use num_traits::ToPrimitive;
use pipeline::database::hw1::visual::VisualTag;
use pipeline::uax::types::Animation;

use super::animation_events::{AnimationEventCursor, crossed_tag};
use motion::AnimationPose;

const POSITION_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum AnimationExitAction {
    #[default]
    Loop,
    Freeze,
    Transition,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LoadedAnimationSegment {
    pub(super) animation_type: String,
    pub(super) clip: Option<Arc<Animation>>,
    pub(super) tags: Vec<VisualTag>,
    pub(super) exit_action: AnimationExitAction,
    pub(super) tween_seconds: f32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LoadedAnimationTrack {
    pub(super) segments: Vec<LoadedAnimationSegment>,
    pub(super) definition_indices: Vec<usize>,
    pub(super) loop_from: Option<usize>,
    pub(super) uses_simulation_clock: bool,
}

impl LoadedAnimationTrack {
    pub(super) fn initial_clip(&self) -> Option<&Animation> {
        self.segments.first()?.clip.as_deref()
    }

    pub(super) fn has_clip(&self) -> bool {
        self.initial_clip().is_some()
    }

    pub(super) fn tags(&self) -> impl Iterator<Item = &VisualTag> {
        self.segments.iter().flat_map(|segment| &segment.tags)
    }
}

#[derive(Debug)]
pub(super) struct RenderedAnimationTrack {
    segments: Vec<LoadedAnimationSegment>,
    loop_from: Option<usize>,
    uses_simulation_clock: bool,
    event_cursors: Vec<AnimationEventCursor>,
    active_segment: Option<usize>,
    last_simulation_position: Option<f32>,
    simulation_cycle: u32,
    transition_playback: Option<TransitionPlayback>,
}

pub(super) struct AnimationTrackSample {
    pub(super) pose: Option<AnimationPose>,
    pub(super) normalized_position: Option<f32>,
    pub(super) crossed_tags: Vec<VisualTag>,
}

#[derive(Clone, Copy, Debug)]
struct TimelinePoint {
    segment: usize,
    normalized_position: f32,
    timeline_seconds: f32,
    elapsed_in_segment: f32,
    tween_from: Option<usize>,
    simulation_root_phase: Option<f32>,
    rebaseline_segment: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct TransitionPlayback {
    segment: usize,
    started_phase: f32,
    timeline_start_seconds: f32,
}

impl RenderedAnimationTrack {
    pub(super) fn from_loaded(track: &LoadedAnimationTrack) -> Self {
        let segment_count = track
            .segments
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, segment)| segment.clip.is_none().then_some(index))
            .unwrap_or(track.segments.len());
        let event_cursors = (0..segment_count)
            .map(|_| AnimationEventCursor::default())
            .collect();
        Self {
            segments: track.segments[..segment_count].to_vec(),
            loop_from: track.loop_from.filter(|index| *index < segment_count),
            uses_simulation_clock: track.uses_simulation_clock,
            event_cursors,
            active_segment: None,
            last_simulation_position: None,
            simulation_cycle: 0,
            transition_playback: None,
        }
    }

    pub(super) fn sample(
        &mut self,
        bind_local: impl Fn(&str) -> Option<glam::Mat4>,
        simulation_position: Option<f32>,
        presentation_phase: f32,
        emit_events: bool,
    ) -> AnimationTrackSample {
        let Some(point) = self.timeline_point(simulation_position, presentation_phase) else {
            return AnimationTrackSample::empty();
        };
        let Some(segment) = self.segments.get(point.segment) else {
            return AnimationTrackSample::empty();
        };
        let pose = segment
            .clip
            .as_deref()
            .map(|animation| AnimationPose::at_position(animation, point.normalized_position));
        let tween_pose = point
            .tween_from
            .and_then(|index| self.segments.get(index))
            .and_then(|previous| {
                let duration = previous.tween_seconds;
                (duration > f32::EPSILON && point.elapsed_in_segment < duration)
                    .then_some((previous, duration))
            })
            .and_then(|(previous, duration)| {
                previous
                    .clip
                    .as_deref()
                    .map(|animation| (AnimationPose::at_position(animation, 1.0), duration))
            });
        let pose = match (tween_pose, pose) {
            (Some((previous, duration)), Some(current)) => Some(AnimationPose::blended(
                &previous,
                &current,
                (point.elapsed_in_segment / duration).clamp(0.0, 1.0),
                bind_local,
            )),
            (_, pose) => pose,
        };
        let crossed_tags = self.crossed_timeline_tags(&point, emit_events);
        self.active_segment = Some(point.segment);
        AnimationTrackSample {
            pose,
            normalized_position: Some(point.normalized_position),
            crossed_tags,
        }
    }

    pub(super) fn presentation_duration(&self) -> Option<f32> {
        self.segments
            .first()
            .and_then(|segment| segment.clip.as_deref())
            .map(|animation| animation.duration)
            .filter(|duration| duration.is_finite() && *duration > f32::EPSILON)
    }

    pub(super) fn active_animation_type(&self) -> Option<&str> {
        self.active_segment
            .and_then(|index| self.segments.get(index))
            .map(|segment| segment.animation_type.as_str())
    }

    fn timeline_point(
        &mut self,
        simulation_position: Option<f32>,
        presentation_phase: f32,
    ) -> Option<TimelinePoint> {
        let root_duration = self.presentation_duration()?;
        if self.uses_simulation_clock {
            let position = simulation_position?.clamp(0.0, 1.0);
            let event_phase = self.simulation_event_phase(position);
            if position < 1.0 || !self.has_transition_continuation() {
                return Some(TimelinePoint {
                    segment: 0,
                    normalized_position: position,
                    timeline_seconds: position * root_duration,
                    elapsed_in_segment: position * root_duration,
                    tween_from: None,
                    simulation_root_phase: Some(event_phase),
                    rebaseline_segment: None,
                });
            }
            let starting_chain = self.transition_playback.is_none();
            let mut point = self.transition_timeline_point(presentation_phase, root_duration)?;
            point.simulation_root_phase = starting_chain.then_some(event_phase);
            return Some(point);
        }
        let presentation_phase = presentation_phase.max(0.0);
        if self.has_transition_continuation() && presentation_phase >= 1.0 {
            return self.transition_timeline_point(presentation_phase, root_duration);
        }
        self.resolve_presentation_time(presentation_phase * root_duration)
    }

    fn transition_timeline_point(
        &mut self,
        presentation_phase: f32,
        root_duration: f32,
    ) -> Option<TimelinePoint> {
        let presentation_phase = presentation_phase.max(0.0);
        let Some(playback) = self.transition_playback else {
            return self.begin_transition(0, presentation_phase, root_duration);
        };
        let duration = segment_duration(self.segments.get(playback.segment)?)?;
        let elapsed = (presentation_phase - playback.started_phase).max(0.0) * root_duration;
        if elapsed >= duration
            && self
                .segments
                .get(playback.segment)
                .is_some_and(|segment| segment.exit_action == AnimationExitAction::Transition)
            && self.transition_successor(playback.segment).is_some()
        {
            return self.begin_transition(
                playback.segment,
                presentation_phase,
                playback.timeline_start_seconds + duration,
            );
        }
        self.resolve_presentation_time(playback.timeline_start_seconds + elapsed)
    }

    fn begin_transition(
        &mut self,
        from_segment: usize,
        presentation_phase: f32,
        timeline_start_seconds: f32,
    ) -> Option<TimelinePoint> {
        let segment = self.transition_successor(from_segment)?;
        self.transition_playback = Some(TransitionPlayback {
            segment,
            started_phase: presentation_phase,
            timeline_start_seconds,
        });
        Some(TimelinePoint {
            segment,
            normalized_position: 0.0,
            timeline_seconds: timeline_start_seconds,
            elapsed_in_segment: 0.0,
            tween_from: Some(from_segment),
            simulation_root_phase: None,
            rebaseline_segment: Some(segment),
        })
    }

    fn transition_successor(&self, segment: usize) -> Option<usize> {
        let next = segment.saturating_add(1);
        (next < self.segments.len())
            .then_some(next)
            .or_else(|| self.loop_from.filter(|index| *index < self.segments.len()))
    }

    fn simulation_event_phase(&mut self, position: f32) -> f32 {
        if self
            .last_simulation_position
            .is_some_and(|previous| position + POSITION_EPSILON < previous)
        {
            self.simulation_cycle = self.simulation_cycle.wrapping_add(1);
            self.transition_playback = None;
        }
        self.last_simulation_position = Some(position);
        self.simulation_cycle.to_f32().unwrap_or(f32::MAX) + position
    }

    fn has_transition_continuation(&self) -> bool {
        self.segments
            .first()
            .is_some_and(|segment| segment.exit_action == AnimationExitAction::Transition)
            && (self.segments.len() > 1 || self.loop_from.is_some())
    }

    fn resolve_presentation_time(&self, elapsed_seconds: f32) -> Option<TimelinePoint> {
        let timeline_seconds = elapsed_seconds.max(0.0);
        let loop_from = self.loop_from.filter(|index| *index < self.segments.len());
        let prefix_end = loop_from.unwrap_or(self.segments.len());
        let mut remaining = timeline_seconds;
        for index in 0..prefix_end {
            let duration = segment_duration(self.segments.get(index)?)?;
            if remaining < duration {
                let normalized = remaining / duration;
                return Some(TimelinePoint {
                    segment: index,
                    normalized_position: normalized,
                    timeline_seconds,
                    elapsed_in_segment: remaining,
                    tween_from: index.checked_sub(1),
                    simulation_root_phase: None,
                    rebaseline_segment: None,
                });
            }
            remaining -= duration;
        }
        let Some(loop_from) = loop_from else {
            let segment = self.segments.len().checked_sub(1)?;
            return Some(TimelinePoint {
                segment,
                normalized_position: 1.0,
                timeline_seconds,
                elapsed_in_segment: segment_duration(self.segments.get(segment)?)?,
                tween_from: segment.checked_sub(1),
                simulation_root_phase: None,
                rebaseline_segment: None,
            });
        };
        let cycle_duration = self.segments[loop_from..]
            .iter()
            .filter_map(segment_duration)
            .sum::<f32>();
        if cycle_duration <= f32::EPSILON {
            return Some(TimelinePoint {
                segment: loop_from,
                normalized_position: 0.0,
                timeline_seconds,
                elapsed_in_segment: 0.0,
                tween_from: loop_from.checked_sub(1),
                simulation_root_phase: None,
                rebaseline_segment: None,
            });
        }
        let cycle = (remaining / cycle_duration).floor();
        remaining = remaining.rem_euclid(cycle_duration);
        for index in loop_from..self.segments.len() {
            let duration = segment_duration(self.segments.get(index)?)?;
            if remaining < duration {
                let normalized = remaining / duration;
                return Some(TimelinePoint {
                    segment: index,
                    normalized_position: normalized,
                    timeline_seconds,
                    elapsed_in_segment: remaining,
                    tween_from: cycle_tween_source(index, loop_from, cycle, self.segments.len()),
                    simulation_root_phase: None,
                    rebaseline_segment: None,
                });
            }
            remaining -= duration;
        }
        None
    }

    fn crossed_timeline_tags(
        &mut self,
        point: &TimelinePoint,
        emit_events: bool,
    ) -> Vec<VisualTag> {
        let mut phases = self.timeline_event_phases(point.timeline_seconds);
        let rebaseline = point.rebaseline_segment.and_then(|index| {
            phases
                .get(index)
                .copied()
                .map(|phase| (index, phase - POSITION_EPSILON))
        });
        if let (Some(root_phase), Some(phase)) = (phases.first_mut(), point.simulation_root_phase) {
            *root_phase = phase;
        }
        let mut crossed = Vec::new();
        for (index, ((segment, cursor), phase)) in self
            .segments
            .iter()
            .zip(&mut self.event_cursors)
            .zip(phases)
            .enumerate()
        {
            let Some(interval) = cursor.advance(phase) else {
                continue;
            };
            crossed.extend(
                segment
                    .tags
                    .iter()
                    .filter(|tag| {
                        internal_pose_tag(&tag.tag_type)
                            || emit_events && presentation_tag(&tag.tag_type)
                    })
                    .filter(|tag| {
                        point.rebaseline_segment != Some(index)
                            || tag_event_position(tag) >= POSITION_EPSILON
                    })
                    .filter(|tag| crossed_tag(interval, tag_event_position(tag)))
                    .cloned(),
            );
        }
        if let Some((index, phase)) = rebaseline
            && let Some(cursor) = self.event_cursors.get_mut(index)
        {
            cursor.rebaseline(phase);
        }
        crossed
    }

    fn timeline_event_phases(&self, elapsed_seconds: f32) -> Vec<f32> {
        let mut phases = vec![-POSITION_EPSILON; self.segments.len()];
        let loop_from = self.loop_from.filter(|index| *index < self.segments.len());
        let prefix_end = loop_from.unwrap_or(self.segments.len());
        let mut start = 0.0;
        for (index, phase) in phases.iter_mut().enumerate().take(prefix_end) {
            let Some(duration) = self.segments.get(index).and_then(segment_duration) else {
                return phases;
            };
            *phase = one_shot_event_phase(elapsed_seconds, start, duration);
            start += duration;
        }
        let Some(loop_from) = loop_from else {
            return phases;
        };
        let cycle_duration = self.segments[loop_from..]
            .iter()
            .filter_map(segment_duration)
            .sum::<f32>();
        if elapsed_seconds < start || cycle_duration <= f32::EPSILON {
            return phases;
        }
        let loop_elapsed = elapsed_seconds - start;
        let cycle = (loop_elapsed / cycle_duration).floor();
        let cycle_elapsed = loop_elapsed.rem_euclid(cycle_duration);
        let mut segment_start = 0.0;
        for (index, phase) in phases.iter_mut().enumerate().skip(loop_from) {
            let Some(duration) = self.segments.get(index).and_then(segment_duration) else {
                return phases;
            };
            *phase = loop_event_phase(cycle, cycle_elapsed, segment_start, duration);
            segment_start += duration;
        }
        phases
    }
}

impl AnimationTrackSample {
    fn empty() -> Self {
        Self {
            pose: None,
            normalized_position: None,
            crossed_tags: Vec::new(),
        }
    }
}

fn segment_duration(segment: &LoadedAnimationSegment) -> Option<f32> {
    segment
        .clip
        .as_deref()
        .map(|animation| animation.duration)
        .filter(|duration| duration.is_finite() && *duration > f32::EPSILON)
}

fn one_shot_event_phase(elapsed: f32, start: f32, duration: f32) -> f32 {
    if elapsed <= start {
        -POSITION_EPSILON
    } else {
        ((elapsed - start) / duration).clamp(0.0, 1.0)
    }
}

fn loop_event_phase(cycle: f32, elapsed: f32, start: f32, duration: f32) -> f32 {
    if elapsed <= start {
        if cycle > 0.0 {
            cycle
        } else {
            -POSITION_EPSILON
        }
    } else if elapsed >= start + duration {
        cycle + 1.0
    } else {
        cycle + (elapsed - start) / duration
    }
}

fn cycle_tween_source(
    segment: usize,
    loop_from: usize,
    cycle: f32,
    segment_count: usize,
) -> Option<usize> {
    if segment > loop_from {
        return segment.checked_sub(1);
    }
    if loop_from > 0 || cycle >= 1.0 {
        return segment_count.checked_sub(1);
    }
    None
}

fn internal_pose_tag(tag_type: &str) -> bool {
    tag_type.eq_ignore_ascii_case("GroundIK") || tag_type.eq_ignore_ascii_case("SweetSpot")
}

fn tag_event_position(tag: &VisualTag) -> f32 {
    if tag.tag_type.eq_ignore_ascii_case("SweetSpot") {
        tag.start.or(tag.position).unwrap_or_default()
    } else {
        tag.position.unwrap_or_default()
    }
}

fn presentation_tag(tag_type: &str) -> bool {
    tag_type.eq_ignore_ascii_case("TerrainEffect")
        || tag_type.eq_ignore_ascii_case("Particle")
        || tag_type.eq_ignore_ascii_case("Light")
        || tag_type.eq_ignore_ascii_case("CameraShake")
        || tag_type.eq_ignore_ascii_case("TerrainAlpha")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Mat4;
    use pipeline::database::hw1::visual::VisualTag;
    use pipeline::uax::types::Animation;

    use super::{
        AnimationExitAction, LoadedAnimationSegment, LoadedAnimationTrack, RenderedAnimationTrack,
    };

    fn clip(duration: f32) -> Arc<Animation> {
        Arc::new(Animation {
            name: Some("test".to_owned()),
            duration,
            time_step: 1.0 / 30.0,
            oversampling: 1.0,
            track_groups: Vec::new(),
            default_loop_count: 0,
            flags: 0,
        })
    }

    fn tag(tag_type: &str) -> VisualTag {
        VisualTag {
            tag_type: tag_type.to_owned(),
            position: Some(0.5),
            name: Some(tag_type.to_owned()),
            ..VisualTag::default()
        }
    }

    fn track(
        animation_type: &str,
        tags: Vec<VisualTag>,
        uses_simulation_clock: bool,
    ) -> LoadedAnimationTrack {
        LoadedAnimationTrack {
            segments: vec![LoadedAnimationSegment {
                animation_type: animation_type.to_owned(),
                clip: Some(clip(1.0)),
                tags,
                exit_action: AnimationExitAction::Loop,
                tween_seconds: 0.0,
            }],
            definition_indices: vec![0],
            loop_from: Some(0),
            uses_simulation_clock,
        }
    }

    fn sample(
        track: &mut RenderedAnimationTrack,
        simulation_position: Option<f32>,
        presentation_phase: f32,
    ) -> super::AnimationTrackSample {
        track.sample(
            |_| Some(Mat4::IDENTITY),
            simulation_position,
            presentation_phase,
            true,
        )
    }

    #[test]
    fn action_and_movement_tracks_keep_independent_render_event_cursors() {
        let mut action = RenderedAnimationTrack::from_loaded(&track(
            "Attack",
            vec![tag("Particle"), tag("Attack")],
            false,
        ));
        let mut movement = RenderedAnimationTrack::from_loaded(&track(
            "Walk",
            vec![tag("Light"), tag("Sound")],
            false,
        ));

        assert!(sample(&mut action, None, 0.25).crossed_tags.is_empty());
        assert!(sample(&mut movement, None, 0.25).crossed_tags.is_empty());
        let action_tags = sample(&mut action, None, 0.75).crossed_tags;
        let movement_tags = sample(&mut movement, None, 0.75).crossed_tags;

        assert_eq!(action_tags.len(), 1);
        assert!(action_tags[0].tag_type.eq_ignore_ascii_case("Particle"));
        assert_eq!(movement_tags.len(), 1);
        assert!(movement_tags[0].tag_type.eq_ignore_ascii_case("Light"));
    }

    #[test]
    fn sweet_spot_crosses_at_its_start_not_full_weight_position() {
        let mut sweet_spot = tag("SweetSpot");
        sweet_spot.start = Some(0.2);
        sweet_spot.position = Some(0.5);
        sweet_spot.end = Some(0.8);
        let mut rendered =
            RenderedAnimationTrack::from_loaded(&track("Attack", vec![sweet_spot], true));

        assert!(
            sample(&mut rendered, Some(0.1), 0.0)
                .crossed_tags
                .is_empty()
        );
        assert_eq!(sample(&mut rendered, Some(0.3), 0.0).crossed_tags.len(), 1);
    }

    #[test]
    fn transition_exit_action_chains_once_then_loops_the_target() {
        let mut loaded = track("IdleWalk", vec![], false);
        loaded.segments[0].exit_action = AnimationExitAction::Transition;
        loaded.segments.push(LoadedAnimationSegment {
            animation_type: "Walk".to_owned(),
            clip: Some(clip(2.0)),
            tags: vec![],
            exit_action: AnimationExitAction::Loop,
            tween_seconds: 0.0,
        });
        loaded.loop_from = Some(1);
        let mut rendered = RenderedAnimationTrack::from_loaded(&loaded);

        sample(&mut rendered, None, 0.5);
        assert_eq!(rendered.active_animation_type(), Some("IdleWalk"));
        sample(&mut rendered, None, 1.5);
        assert_eq!(rendered.active_animation_type(), Some("Walk"));
        assert_eq!(
            sample(&mut rendered, None, 1.5).normalized_position,
            Some(0.0)
        );
        assert_eq!(
            sample(&mut rendered, None, 2.0).normalized_position,
            Some(0.25)
        );
    }

    #[test]
    fn missing_transition_clip_holds_the_last_valid_pose() {
        let mut loaded = track("Attack", vec![], false);
        loaded.segments[0].exit_action = AnimationExitAction::Transition;
        loaded.loop_from = None;
        loaded.segments.push(LoadedAnimationSegment {
            animation_type: "MissingRecover".to_owned(),
            clip: None,
            tags: vec![],
            exit_action: AnimationExitAction::Freeze,
            tween_seconds: 0.0,
        });
        let mut rendered = RenderedAnimationTrack::from_loaded(&loaded);

        let late = sample(&mut rendered, None, 2.0);
        assert!(late.pose.is_some());
        assert_eq!(late.normalized_position, Some(1.0));
        assert_eq!(rendered.active_animation_type(), Some("Attack"));
    }

    #[test]
    fn transition_crossing_emits_outgoing_and_incoming_presentation_tags() {
        let mut loaded = track("Attack", vec![tag("Particle")], false);
        loaded.segments[0].exit_action = AnimationExitAction::Transition;
        loaded.segments.push(LoadedAnimationSegment {
            animation_type: "Recover".to_owned(),
            clip: Some(clip(2.0)),
            tags: vec![tag("Light")],
            exit_action: AnimationExitAction::Loop,
            tween_seconds: 0.0,
        });
        loaded.loop_from = Some(1);
        let mut rendered = RenderedAnimationTrack::from_loaded(&loaded);

        assert!(sample(&mut rendered, None, 0.25).crossed_tags.is_empty());
        let boundary = sample(&mut rendered, None, 2.0);
        assert_eq!(boundary.normalized_position, Some(0.0));
        assert_eq!(boundary.crossed_tags.len(), 1);
        assert!(
            boundary.crossed_tags[0]
                .tag_type
                .eq_ignore_ascii_case("Particle")
        );
        let incoming = sample(&mut rendered, None, 3.0).crossed_tags;
        assert_eq!(incoming.len(), 1);
        assert!(incoming[0].tag_type.eq_ignore_ascii_case("Light"));
    }

    #[test]
    fn self_transition_finishes_outgoing_tags_then_refires_start_tags() {
        let mut at_start = tag("Particle");
        at_start.position = Some(0.0);
        let mut at_middle = tag("Light");
        at_middle.position = Some(0.5);
        let mut at_end = tag("CameraShake");
        at_end.position = Some(0.9);
        let mut loaded = track("Idle", vec![at_start, at_middle, at_end], false);
        loaded.segments[0].exit_action = AnimationExitAction::Transition;
        let mut rendered = RenderedAnimationTrack::from_loaded(&loaded);

        assert!(sample(&mut rendered, None, 0.0).crossed_tags.is_empty());
        let initial = sample(&mut rendered, None, 0.75).crossed_tags;
        assert_eq!(initial.len(), 2);
        let boundary = sample(&mut rendered, None, 1.2);
        assert_eq!(boundary.normalized_position, Some(0.0));
        assert_eq!(boundary.crossed_tags.len(), 1);
        assert!(
            boundary.crossed_tags[0]
                .tag_type
                .eq_ignore_ascii_case("CameraShake")
        );
        let restarted = sample(&mut rendered, None, 1.3).crossed_tags;
        assert_eq!(restarted.len(), 1);
        assert!(restarted[0].tag_type.eq_ignore_ascii_case("Particle"));
    }

    #[test]
    fn first_late_sample_does_not_replay_transition_history() {
        let mut loaded = track("Attack", vec![tag("Particle")], false);
        loaded.segments[0].exit_action = AnimationExitAction::Transition;
        loaded.segments.push(LoadedAnimationSegment {
            animation_type: "Recover".to_owned(),
            clip: Some(clip(2.0)),
            tags: vec![tag("Light")],
            exit_action: AnimationExitAction::Loop,
            tween_seconds: 0.0,
        });
        loaded.loop_from = Some(1);
        let mut rendered = RenderedAnimationTrack::from_loaded(&loaded);

        assert!(sample(&mut rendered, None, 2.0).crossed_tags.is_empty());
    }

    #[test]
    fn simulation_clock_event_cursor_survives_cycle_wraps() {
        let mut rendered =
            RenderedAnimationTrack::from_loaded(&track("Attack", vec![tag("Particle")], true));
        sample(&mut rendered, Some(0.25), 0.0);
        assert_eq!(sample(&mut rendered, Some(0.75), 0.5).crossed_tags.len(), 1);
        sample(&mut rendered, Some(0.1), 1.0);
        assert_eq!(sample(&mut rendered, Some(0.75), 1.5).crossed_tags.len(), 1);
    }
}
