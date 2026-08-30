//! Headless sampling of attack-tag bones through the authored visual component graph.

use std::collections::BTreeMap;
use std::sync::Arc;

use motion::{AnimationPose, Skeleton, SkeletonPose};
use pipeline::database::hw1::visual::{Asset, Attachment, Model, Visual};
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::uax::Reader as UaxReader;
use pipeline::uax::types::Animation;
use pipeline::ugx::Reader as UgxReader;

use super::events::{
    AttackAnimationAnchor, AttackAnimationEvent, AttackAnimationEventKind, AttackAttachmentPose,
    AttackSingleBonePose,
};

const MAX_COMPONENT_DEPTH: usize = 32;

#[derive(Debug, Default)]
pub(super) struct AnchorAssetCache {
    skeletons: BTreeMap<String, Result<Arc<Skeleton>, String>>,
    animations: BTreeMap<String, Result<Arc<Animation>, String>>,
}

struct ComponentPath<'visual> {
    models: Vec<&'visual Model>,
    attachments: Vec<&'visual Attachment>,
}

#[derive(Debug, Clone, PartialEq)]
struct SampledComponent {
    skeleton: Option<Arc<Skeleton>>,
    animation: Option<Arc<Animation>>,
}

#[derive(Debug, Clone, PartialEq)]
struct SampledAttachment {
    child_component: String,
    to_bone: Option<String>,
    from_bone: Option<String>,
    disregard_orientation: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AttackAnchorTrack {
    components: Vec<SampledComponent>,
    attachments: Vec<SampledAttachment>,
    single_bones: Vec<String>,
}

impl AttackAnchorTrack {
    fn from_path(
        path: &ComponentPath<'_>,
        components: Vec<SampledComponent>,
        single_bones: &[String],
    ) -> Self {
        let attachments = path
            .attachments
            .iter()
            .enumerate()
            .filter_map(|(index, attachment)| {
                Some(SampledAttachment {
                    child_component: path.models.get(index + 1)?.name.clone(),
                    to_bone: attachment.to_bone.clone(),
                    from_bone: attachment.from_bone.clone(),
                    disregard_orientation: attachment.disregard_orient.unwrap_or(false),
                })
            })
            .collect();
        Self {
            components,
            attachments,
            single_bones: single_bones.to_vec(),
        }
    }

    pub(super) fn sample(&self, position: f32) -> AttackAnimationAnchor {
        let poses = sample_component_poses(&self.components, position);
        let links = self
            .attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| AttackAttachmentPose {
                child_component: attachment.child_component.clone(),
                to_bone: posed_bone(
                    &self.components[index],
                    poses[index].as_ref(),
                    attachment.to_bone.as_deref(),
                ),
                from_bone: posed_bone(
                    &self.components[index + 1],
                    poses[index + 1].as_ref(),
                    attachment.from_bone.as_deref(),
                ),
                disregard_orientation: attachment.disregard_orientation,
            })
            .collect();
        let single_bone_poses = self
            .components
            .last()
            .zip(poses.last().and_then(Option::as_ref))
            .map_or_else(Vec::new, |(component, pose)| {
                self.single_bones
                    .iter()
                    .filter_map(|bone| {
                        let skeleton = component.skeleton.as_deref()?;
                        let (parent_to_component, local_transform, bone_to_event) =
                            skeleton.posed_ancestor_frames(pose, bone, bone)?;
                        Some(AttackSingleBonePose {
                            bone: bone.clone(),
                            parent_to_component,
                            local_transform,
                            bone_to_event,
                        })
                    })
                    .collect()
            });
        AttackAnimationAnchor {
            links,
            bone_to_component: None,
            single_bone_poses,
        }
    }
}

pub(super) fn resolve_event_anchors(
    visual: &Visual,
    target_model: &Model,
    single_bones: &[String],
    selection: (&str, usize, &str),
    events: &mut [AttackAnimationEvent],
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AnchorAssetCache,
) -> Result<AttackAnchorTrack, String> {
    let path = component_path(visual, &target_model.name).ok_or_else(|| {
        format!(
            "visual component {} is not attached below the default model",
            target_model.name
        )
    })?;
    let (animation_type, selected_asset_index, selected_asset_path) = selection;
    let components = sample_components(
        &path,
        animation_type,
        selected_asset_index,
        selected_asset_path,
        source,
        cache,
    )?;
    for event in events {
        event.anchor = sample_event_anchor(&path, &components, single_bones, event);
    }
    Ok(AttackAnchorTrack::from_path(
        &path,
        components,
        single_bones,
    ))
}

fn sample_components(
    path: &ComponentPath<'_>,
    animation_type: &str,
    selected_asset_index: usize,
    selected_asset_path: &str,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AnchorAssetCache,
) -> Result<Vec<SampledComponent>, String> {
    path.models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            let skeleton = load_skeleton(model, source, cache).ok();
            let synchronized = index == 0
                || path
                    .attachments
                    .get(index.saturating_sub(1))
                    .is_some_and(|attachment| attachment.sync_anims.unwrap_or(false));
            let animation = (skeleton.is_some() && synchronized)
                .then(|| {
                    selected_component_animation(
                        model,
                        animation_type,
                        selected_asset_index,
                        selected_asset_path,
                    )
                })
                .flatten()
                .and_then(|path| load_animation(path, source, cache).ok());
            Ok(SampledComponent {
                skeleton,
                animation,
            })
        })
        .collect()
}

fn sample_event_anchor(
    path: &ComponentPath<'_>,
    components: &[SampledComponent],
    single_bones: &[String],
    event: &AttackAnimationEvent,
) -> Option<AttackAnimationAnchor> {
    let poses = sample_component_poses(components, event.position);
    let links = path
        .attachments
        .iter()
        .enumerate()
        .map(|(index, attachment)| {
            let parent = components.get(index)?;
            let parent_pose = poses.get(index)?.as_ref();
            let child = components.get(index + 1)?;
            let child_pose = poses.get(index + 1)?.as_ref();
            Some(AttackAttachmentPose {
                child_component: path.models.get(index + 1)?.name.clone(),
                to_bone: posed_bone(parent, parent_pose, attachment.to_bone.as_deref()),
                from_bone: posed_bone(child, child_pose, attachment.from_bone.as_deref()),
                disregard_orientation: attachment.disregard_orient.unwrap_or(false),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let component = components.last()?;
    let pose = poses.last()?.as_ref();
    let bone_to_component = posed_bone(component, pose, event_bone(event));
    let single_bone_poses = event_bone(event).map_or_else(Vec::new, |event_bone| {
        single_bones
            .iter()
            .filter_map(|bone| {
                let skeleton = component.skeleton.as_deref()?;
                let pose = pose?;
                let (parent_to_component, local_transform, bone_to_event) =
                    skeleton.posed_ancestor_frames(pose, bone, event_bone)?;
                Some(AttackSingleBonePose {
                    bone: bone.clone(),
                    parent_to_component,
                    local_transform,
                    bone_to_event,
                })
            })
            .collect()
    });
    if matches!(event.kind, AttackAnimationEventKind::Attack { .. }) && bone_to_component.is_none()
    {
        return None;
    }
    Some(AttackAnimationAnchor {
        links,
        bone_to_component,
        single_bone_poses,
    })
}

fn sample_component_poses(
    components: &[SampledComponent],
    position: f32,
) -> Vec<Option<SkeletonPose>> {
    let position = if position.is_finite() {
        position.clamp(0.0, 1.0)
    } else {
        0.0
    };
    components
        .iter()
        .map(|component| {
            let animation_pose = component
                .animation
                .as_deref()
                .map(|animation| AnimationPose::at_position(animation, position));
            component
                .skeleton
                .as_deref()
                .map(|skeleton| skeleton.pose(animation_pose.as_ref()))
        })
        .collect()
}

fn posed_bone(
    component: &SampledComponent,
    pose: Option<&SkeletonPose>,
    name: Option<&str>,
) -> Option<glam::Mat4> {
    let skeleton = component.skeleton.as_deref()?;
    let pose = pose?;
    name.and_then(|name| skeleton.posed_bone_to_model(pose, name))
}

fn event_bone(event: &AttackAnimationEvent) -> Option<&str> {
    match &event.kind {
        AttackAnimationEventKind::Attack { to_bone } => to_bone.as_deref(),
        AttackAnimationEventKind::PhysicsImpulse(impulse) => impulse.to_bone.as_deref(),
    }
}

fn component_path<'visual>(
    visual: &'visual Visual,
    target: &str,
) -> Option<ComponentPath<'visual>> {
    let target_model = find_model(visual, target)?;
    if let Some(root) = visual
        .default_model
        .as_deref()
        .and_then(|name| find_model(visual, name))
    {
        let mut models = vec![root];
        let mut attachments = Vec::new();
        if find_component_path(visual, root, target, &mut models, &mut attachments) {
            return Some(ComponentPath {
                models,
                attachments,
            });
        }
    }

    // Logic-selected squad-mode models are alternate visual roots rather than
    // attachments below Default. Retail evaluates their bones in unit space.
    Some(ComponentPath {
        models: vec![target_model],
        attachments: Vec::new(),
    })
}

fn find_component_path<'visual>(
    visual: &'visual Visual,
    current: &'visual Model,
    target: &str,
    models: &mut Vec<&'visual Model>,
    attachments: &mut Vec<&'visual Attachment>,
) -> bool {
    if current.name.eq_ignore_ascii_case(target) {
        return true;
    }
    if models.len() >= MAX_COMPONENT_DEPTH {
        return false;
    }
    let Some(component) = &current.component else {
        return false;
    };
    for attachment in component
        .attachments
        .iter()
        .filter(|attachment| attachment.attach_type.eq_ignore_ascii_case("ModelRef"))
    {
        let Some(child) = find_model(visual, &attachment.name) else {
            continue;
        };
        if models
            .iter()
            .any(|model| model.name.eq_ignore_ascii_case(&child.name))
        {
            continue;
        }
        models.push(child);
        attachments.push(attachment);
        if find_component_path(visual, child, target, models, attachments) {
            return true;
        }
        attachments.pop();
        models.pop();
    }
    false
}

fn find_model<'visual>(visual: &'visual Visual, name: &str) -> Option<&'visual Model> {
    visual
        .models
        .iter()
        .find(|model| model.name.eq_ignore_ascii_case(name))
}

fn load_skeleton(
    model: &Model,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AnchorAssetCache,
) -> Result<Arc<Skeleton>, String> {
    let asset = model
        .component
        .as_ref()
        .and_then(|component| component.assets.iter().find_map(model_asset_path))
        .ok_or_else(|| format!("visual component {} has no direct model asset", model.name))?;
    let path = canonical_art_path(asset);
    let key = path.to_ascii_lowercase();
    if !cache.skeletons.contains_key(&key) {
        let skeleton = source
            .resolve_with_fallback(&path, &[".ugx"])
            .ok_or_else(|| format!("model {path} was not found"))
            .and_then(|bytes| {
                UgxReader::read(&bytes)
                    .map(|geometry| Arc::new(Skeleton::from_geometry(&geometry)))
                    .map_err(|error| format!("failed to parse {path}: {error}"))
            });
        cache.skeletons.insert(key.clone(), skeleton);
    }
    cache
        .skeletons
        .get(&key)
        .expect("skeleton cache entry was inserted")
        .clone()
}

fn load_animation(
    path: &str,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut AnchorAssetCache,
) -> Result<Arc<Animation>, String> {
    let path = canonical_animation_path(path);
    let key = path.to_ascii_lowercase();
    if !cache.animations.contains_key(&key) {
        let animation = source
            .resolve_with_fallback(&path, &[".uax"])
            .ok_or_else(|| format!("animation {path} was not found"))
            .and_then(|bytes| {
                UaxReader::read(&bytes)
                    .map(Arc::new)
                    .map_err(|error| format!("failed to parse {path}: {error}"))
            });
        cache.animations.insert(key.clone(), animation);
    }
    cache
        .animations
        .get(&key)
        .expect("animation cache entry was inserted")
        .clone()
}

fn selected_component_animation<'model>(
    model: &'model Model,
    animation_type: &str,
    selected_index: usize,
    selected_path: &str,
) -> Option<&'model str> {
    let animation = model
        .anims
        .iter()
        .find(|animation| animation.anim_type.eq_ignore_ascii_case(animation_type))?;
    animation
        .assets
        .iter()
        .find(|asset| animation_identity(asset) == Some(canonical_identity(selected_path)))
        .and_then(animation_asset_path)
        .or_else(|| {
            animation
                .assets
                .get(selected_index)
                .and_then(animation_asset_path)
        })
}

fn animation_identity(asset: &Asset) -> Option<String> {
    animation_asset_path(asset).map(canonical_identity)
}

fn canonical_identity(path: &str) -> String {
    canonical_animation_path(path)
        .to_ascii_lowercase()
        .trim_end_matches(".uax")
        .to_owned()
}

fn animation_asset_path(asset: &Asset) -> Option<&str> {
    asset
        .asset_type
        .eq_ignore_ascii_case("Anim")
        .then_some(asset.file.as_deref())
        .flatten()
}

fn model_asset_path(asset: &Asset) -> Option<&str> {
    asset
        .asset_type
        .eq_ignore_ascii_case("Model")
        .then_some(asset.file.as_deref())
        .flatten()
}

fn canonical_art_path(path: &str) -> String {
    let normalized = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if normalized
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("art\\"))
    {
        normalized
    } else {
        format!("art\\{normalized}")
    }
}

fn canonical_animation_path(path: &str) -> String {
    let mut path = canonical_art_path(path);
    if !path.to_ascii_lowercase().ends_with(".uax") {
        path.push_str(".uax");
    }
    path
}

#[cfg(test)]
mod tests {
    use pipeline::database::hw1::visual::{Attachment, Component, Model, Visual};

    use super::component_path;

    #[test]
    fn component_path_follows_only_the_branch_owning_the_event_model() {
        let visual = Visual {
            default_model: Some("root".to_owned()),
            models: vec![
                model("root", Some(("turret", "root_to_turret"))),
                model("turret", Some(("cannon", "turret_to_cannon"))),
                model("cannon", None),
            ],
            logic: None,
        };

        let path = component_path(&visual, "CANNON").expect("component path");

        assert_eq!(
            path.models
                .iter()
                .map(|model| model.name.as_str())
                .collect::<Vec<_>>(),
            ["root", "turret", "cannon"]
        );
        assert_eq!(
            path.attachments[1].to_bone.as_deref(),
            Some("turret_to_cannon")
        );
    }

    #[test]
    fn component_path_uses_an_unattached_logic_model_as_an_alternate_root() {
        let visual = Visual {
            default_model: Some("default".to_owned()),
            models: vec![model("default", None), model("rage", None)],
            logic: None,
        };

        let path = component_path(&visual, "rage").expect("alternate root path");

        assert_eq!(path.models.len(), 1);
        assert_eq!(path.models[0].name, "rage");
        assert!(path.attachments.is_empty());
    }

    fn model(name: &str, child: Option<(&str, &str)>) -> Model {
        Model {
            name: name.to_owned(),
            component: Some(Component {
                attachments: child
                    .map(|(name, to_bone)| Attachment {
                        attach_type: "ModelRef".to_owned(),
                        name: name.to_owned(),
                        to_bone: Some(to_bone.to_owned()),
                        ..Attachment::default()
                    })
                    .into_iter()
                    .collect(),
                ..Component::default()
            }),
            ..Model::default()
        }
    }
}
