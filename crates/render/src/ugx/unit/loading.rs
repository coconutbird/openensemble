//! Recursive visual-model assembly and authored animation selection.

use glam::Mat4;
use num_traits::ToPrimitive;
use pipeline::database::hw1::visual::{Anim, Attachment, Model as VisualModel};
use pipeline::source::{AssetSource, StdFileProvider};

use super::animation_tracks::{AnimationExitAction, LoadedAnimationSegment, LoadedAnimationTrack};
use super::animations::{AnimationSelection, load_animation, select_animation};
use super::instance_attachment::{self, InstanceAttachment};
use super::paths::canonical_model_path;
use super::{
    LoadedUnit, MAX_ATTACHMENT_DEPTH, ParentAttachment, UnitAttachmentKind, UnitAttachmentTrigger,
    UnitInstance, UnitLoadContext, UnitLoadError, load_cached_model, load_named_model,
    make_unit_attachment,
};
use crate::ugx::Model;
use crate::ugx::model::ModelPose;
use motion::AnimationPose;

const MAX_ANIMATION_CHAIN_DEPTH: usize = 32;

struct ActiveAnimation<'model> {
    selection: Option<AnimationSelection<'model>>,
    track: LoadedAnimationTrack,
}

struct ActiveAnimations<'model> {
    action: ActiveAnimation<'model>,
    movement: Option<ActiveAnimation<'model>>,
}

#[derive(Clone, Copy)]
struct TrackRequest<'request> {
    animation_type: &'request str,
    exact_asset: Option<&'request str>,
    preferred_index: Option<usize>,
    uses_simulation_clock: bool,
    animation_roll: u64,
}

#[derive(Clone, Copy)]
pub(super) struct AttachmentOwner<'model> {
    pub(super) definition: &'model VisualModel,
    pub(super) model: &'model Model,
    pub(super) pose: &'model ModelPose,
    pub(super) local_transform: Mat4,
    pub(super) instance_index: usize,
    animation_selections: ActiveAnimationSelections<'model>,
    pub(super) depth: usize,
}

#[derive(Clone, Copy, Default)]
struct ActiveAnimationSelections<'model> {
    action: Option<AnimationSelection<'model>>,
    movement: Option<AnimationSelection<'model>>,
    action_definition_indices: &'model [usize],
    movement_definition_indices: &'model [usize],
}

impl AttachmentOwner<'_> {
    fn parent_binding<'attachment>(
        &'attachment self,
        attachment: &'attachment Attachment,
    ) -> ParentAttachment<'attachment> {
        ParentAttachment {
            instance_index: self.instance_index,
            model: self.model,
            pose: self.pose,
            attachment,
            action_asset_index: self
                .animation_selections
                .action
                .and_then(AnimationSelection::asset_index),
            movement_asset_index: self
                .animation_selections
                .movement
                .and_then(AnimationSelection::asset_index),
        }
    }
}

pub(super) fn load_model_definition(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    definition: &VisualModel,
    path: &str,
    parent: Option<ParentAttachment<'_>>,
    parent_transform: Mat4,
    depth: usize,
) -> Result<LoadedUnit, UnitLoadError> {
    let canonical_path = canonical_model_path(path);
    let model = load_cached_model(source, context.cache, &definition.name, &canonical_path)?;
    let active = load_active_animations(source, context, definition, parent, depth);
    let action_pose = active
        .action
        .track
        .initial_clip()
        .map(AnimationPose::at_start);
    let movement_pose = active
        .movement
        .as_ref()
        .and_then(|animation| animation.track.initial_clip())
        .map(AnimationPose::at_start);
    let pose = model.pose_tracks(action_pose.as_ref(), movement_pose.as_ref());
    let instance_index = instance_attachment::allocate_index(&mut context.next_instance_index);
    let instance_attachment =
        parent.map(|parent| InstanceAttachment::new(parent.instance_index, parent.attachment));
    let local_transform = parent.map_or(parent_transform, |parent| {
        parent_transform
            * instance_attachment::visual_transform(
                parent.model,
                parent.pose,
                &model,
                &pose,
                parent.attachment,
            )
    });
    let owner = AttachmentOwner {
        definition,
        model: &model,
        pose: &pose,
        local_transform,
        instance_index,
        animation_selections: ActiveAnimationSelections {
            action: active.action.selection,
            movement: active
                .movement
                .as_ref()
                .and_then(|animation| animation.selection),
            action_definition_indices: &active.action.track.definition_indices,
            movement_definition_indices: active.movement.as_ref().map_or(&[], |animation| {
                animation.track.definition_indices.as_slice()
            }),
        },
        depth,
    };
    let mut children = load_persistent_attachments(source, context, owner)?;
    children.append(load_animation_attachments(source, context, owner)?);
    let mut loaded = LoadedUnit {
        instances: vec![UnitInstance {
            name: definition.name.clone(),
            model,
            pose,
            action_animation: active.action.track,
            movement_animation: active.movement.map(|animation| animation.track),
            local_transform,
            attachment: instance_attachment,
        }],
        attachments: Vec::new(),
    };
    loaded.append(children);
    Ok(loaded)
}

fn load_active_animations<'model>(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    definition: &'model VisualModel,
    parent: Option<ParentAttachment<'_>>,
    depth: usize,
) -> ActiveAnimations<'model> {
    let synchronized = parent.is_some_and(|parent| parent.attachment.sync_anims.unwrap_or(false));
    let requested_action = context
        .animation_type
        .clone()
        .unwrap_or_else(|| "Idle".to_owned());
    let exact_action_asset = (depth == 0)
        .then(|| context.animation_asset.clone())
        .flatten();
    let action_clock = context.uses_simulation_clock;
    let action_roll = context.animation_roll;
    let action = load_active_animation(
        source,
        context,
        definition,
        depth,
        TrackRequest {
            animation_type: &requested_action,
            exact_asset: exact_action_asset.as_deref(),
            preferred_index: synchronized
                .then(|| parent.and_then(|parent| parent.action_asset_index))
                .flatten(),
            uses_simulation_clock: action_clock,
            animation_roll: action_roll,
        },
    );
    let movement_roll = context.movement_animation_roll;
    let movement = context.movement_animation_type.clone().map(|requested| {
        load_active_animation(
            source,
            context,
            definition,
            depth,
            TrackRequest {
                animation_type: &requested,
                exact_asset: None,
                preferred_index: synchronized
                    .then(|| parent.and_then(|parent| parent.movement_asset_index))
                    .flatten(),
                uses_simulation_clock: false,
                animation_roll: movement_roll,
            },
        )
    });
    ActiveAnimations { action, movement }
}

fn load_active_animation<'model>(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    definition: &'model VisualModel,
    depth: usize,
    request: TrackRequest<'_>,
) -> ActiveAnimation<'model> {
    let selection = select_animation(
        definition,
        request.animation_type,
        request.exact_asset,
        request.preferred_index,
        model_animation_roll(request.animation_roll, &definition.name, depth),
    );
    let (segments, definition_indices, loop_from) = selection.map_or_else(
        || (Vec::new(), Vec::new(), None),
        |selection| load_animation_chain(source, context, definition, depth, request, selection),
    );
    ActiveAnimation {
        selection,
        track: LoadedAnimationTrack {
            segments,
            definition_indices,
            loop_from,
            uses_simulation_clock: request.uses_simulation_clock,
        },
    }
}

fn load_animation_chain(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    definition: &VisualModel,
    depth: usize,
    request: TrackRequest<'_>,
    initial_selection: AnimationSelection<'_>,
) -> (Vec<LoadedAnimationSegment>, Vec<usize>, Option<usize>) {
    let mut segments = Vec::new();
    let mut definition_indices = Vec::new();
    let mut selection = initial_selection;
    let mut loop_from = None;
    for chain_depth in 0..MAX_ANIMATION_CHAIN_DEPTH {
        let Some(animation) = definition.anims.get(selection.definition_index()) else {
            break;
        };
        if let Some(index) = segments
            .iter()
            .position(|segment: &LoadedAnimationSegment| {
                segment
                    .animation_type
                    .eq_ignore_ascii_case(&animation.anim_type)
            })
        {
            loop_from = Some(index);
            break;
        }
        let segment = load_animation_segment(source, context, definition, animation, selection);
        if chain_depth > 0 && segment.clip.is_none() {
            break;
        }
        let exit_action = segment.exit_action;
        definition_indices.push(selection.definition_index());
        segments.push(segment);
        match exit_action {
            AnimationExitAction::Loop => {
                loop_from = Some(segments.len() - 1);
                break;
            }
            AnimationExitAction::Freeze => break,
            AnimationExitAction::Transition => {}
        }
        let Some(target) = transition_target(animation) else {
            break;
        };
        let roll = model_animation_roll(
            request.animation_roll
                ^ u64::try_from(chain_depth + 1)
                    .unwrap_or(u64::MAX)
                    .wrapping_mul(0x9E37_79B9),
            &definition.name,
            depth,
        );
        let Some(target_selection) =
            select_animation(definition, target, None, request.preferred_index, roll)
        else {
            break;
        };
        selection = target_selection;
    }
    (segments, definition_indices, loop_from)
}

fn load_animation_segment(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    definition: &VisualModel,
    animation: &Anim,
    selection: AnimationSelection<'_>,
) -> LoadedAnimationSegment {
    let clip = selection.asset_path().and_then(|path| {
        load_animation(
            source,
            path,
            &definition.name,
            &mut context.cache.animations,
        )
    });
    let tags = selection
        .asset_index()
        .and_then(|index| animation.assets.get(index))
        .map(|asset| asset.tags.clone())
        .unwrap_or_default();
    LoadedAnimationSegment {
        animation_type: animation.anim_type.clone(),
        clip,
        tags,
        exit_action: animation_exit_action(animation),
        tween_seconds: animation
            .tween_time
            .filter(|frames| *frames >= 0)
            .and_then(|frames| frames.to_f32())
            .map_or(0.0, |frames| frames / 30.0),
    }
}

fn animation_exit_action(animation: &Anim) -> AnimationExitAction {
    match animation.exit_action.as_deref() {
        Some(action) if action.eq_ignore_ascii_case("Freeze") => AnimationExitAction::Freeze,
        Some(action) if action.eq_ignore_ascii_case("Transition") => {
            AnimationExitAction::Transition
        }
        _ => AnimationExitAction::Loop,
    }
}

fn transition_target(animation: &Anim) -> Option<&str> {
    animation
        .tween_to_animation
        .as_deref()
        .map(str::trim)
        .filter(|target| !target.is_empty())
}

fn model_animation_roll(seed: u64, model_name: &str, depth: usize) -> u64 {
    model_name.bytes().fold(
        seed ^ u64::try_from(depth).unwrap_or(u64::MAX),
        |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3),
    )
}

fn load_persistent_attachments(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    owner: AttachmentOwner<'_>,
) -> Result<LoadedUnit, UnitLoadError> {
    let mut loaded = LoadedUnit::default();
    let Some(component) = &owner.definition.component else {
        return Ok(loaded);
    };
    for attachment in &component.attachments {
        let descriptor = make_unit_attachment(
            context.selection,
            owner,
            attachment,
            UnitAttachmentTrigger::Persistent,
        )?;
        loaded.append(load_child_model(
            source,
            context,
            owner,
            attachment,
            descriptor.kind,
        )?);
        loaded.attachments.push(descriptor);
    }
    Ok(loaded)
}

fn load_animation_attachments(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    owner: AttachmentOwner<'_>,
) -> Result<LoadedUnit, UnitLoadError> {
    let mut loaded = LoadedUnit::default();
    for (animation_index, animation) in owner.definition.anims.iter().enumerate() {
        for attachment in &animation.attachments {
            let descriptor = make_unit_attachment(
                context.selection,
                owner,
                attachment,
                UnitAttachmentTrigger::Animation(animation.anim_type.clone()),
            )?;
            if owner
                .animation_selections
                .action_definition_indices
                .contains(&animation_index)
                || owner
                    .animation_selections
                    .movement_definition_indices
                    .contains(&animation_index)
            {
                loaded.append(load_child_model(
                    source,
                    context,
                    owner,
                    attachment,
                    descriptor.kind,
                )?);
            }
            loaded.attachments.push(descriptor);
        }
    }
    Ok(loaded)
}

fn load_child_model(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    owner: AttachmentOwner<'_>,
    attachment: &Attachment,
    kind: UnitAttachmentKind,
) -> Result<LoadedUnit, UnitLoadError> {
    match kind {
        UnitAttachmentKind::ModelReference => load_named_model(
            source,
            context,
            &attachment.name,
            Some(owner.parent_binding(attachment)),
            owner.local_transform,
            owner.depth + 1,
        ),
        UnitAttachmentKind::Model => load_model_file(
            source,
            context,
            owner.parent_binding(attachment),
            owner.local_transform,
            owner.depth + 1,
        ),
        UnitAttachmentKind::Particle
        | UnitAttachmentKind::TerrainEffect
        | UnitAttachmentKind::Light => Ok(LoadedUnit::default()),
    }
}

fn load_model_file(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    parent: ParentAttachment<'_>,
    parent_transform: Mat4,
    depth: usize,
) -> Result<LoadedUnit, UnitLoadError> {
    if depth >= MAX_ATTACHMENT_DEPTH {
        return Err(UnitLoadError::AttachmentDepthExceeded);
    }
    let canonical_path = canonical_model_path(&parent.attachment.name);
    let model = load_cached_model(
        source,
        context.cache,
        &parent.attachment.name,
        &canonical_path,
    )?;
    let pose = model.pose(None);
    instance_attachment::allocate_index(&mut context.next_instance_index);
    let local_transform = parent_transform
        * instance_attachment::visual_transform(
            parent.model,
            parent.pose,
            &model,
            &pose,
            parent.attachment,
        );
    Ok(LoadedUnit {
        instances: vec![UnitInstance {
            name: parent.attachment.name.clone(),
            model,
            pose,
            action_animation: LoadedAnimationTrack::default(),
            movement_animation: None,
            local_transform,
            attachment: Some(InstanceAttachment::new(
                parent.instance_index,
                parent.attachment,
            )),
        }],
        attachments: Vec::new(),
    })
}
