use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use pipeline::database::hw1::Visual;
use pipeline::database::hw1::visual::{Attachment, VisualTag};
use pipeline::source::{AssetSource, StdFileProvider};

use super::model::ModelPose;
use super::renderer::{
    MeshVisibility, RendererResources, SharedResources, VisualState, WorldBindings,
};
use super::{Model, Renderer};
use crate::environment::EnvironmentMap;
use crate::terrain::LightingParams;
use crate::{RenderPhase, WorldRenderer};

mod animation_events;
mod animation_tracks;
mod animations;
mod attachment_only;
mod error;
mod ik;
mod instance_attachment;
mod loading;
mod paths;
#[cfg(test)]
mod tests;

pub use error::UnitLoadError;

pub(super) use animation_events::{
    UnitAnimationAnchor, UnitAnimationEvent, UnitAnimationEventKind, UnitTerrainAlphaShape,
};
use animation_tracks::{LoadedAnimationTrack, RenderedAnimationTrack};
use animations::AnimationAssetCache;
#[cfg(test)]
use animations::canonical_animation_path;
pub(super) use ik::{UnitAnimationFrame, UnitIkProfile};
use instance_attachment::{AttachmentAnchor, InstanceAttachment};
use paths::{canonical_art_path, canonical_model_path, model_asset_path};
const MAX_ATTACHMENT_DEPTH: usize = 32;

#[derive(Debug)]
struct UnitInstance {
    name: String,
    model: Arc<Model>,
    pose: ModelPose,
    action_animation: LoadedAnimationTrack,
    movement_animation: Option<LoadedAnimationTrack>,
    local_transform: Mat4,
    attachment: Option<InstanceAttachment>,
}

#[derive(Default)]
pub(super) struct UnitAssetCache {
    models: HashMap<String, Arc<Model>>,
    animations: AnimationAssetCache,
}

/// Renderer destination for one authored visual attachment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitAttachmentKind {
    /// A named model in the same visual definition.
    ModelReference,
    /// A direct UGX model asset.
    Model,
    /// A PFX particle effect.
    Particle,
    /// A TFX terrain-surface router.
    TerrainEffect,
    /// An LGT local-light definition.
    Light,
}

impl UnitAttachmentKind {
    fn from_authored(value: &str) -> Result<Self, UnitLoadError> {
        if value.eq_ignore_ascii_case("ModelRef") {
            Ok(Self::ModelReference)
        } else if value.eq_ignore_ascii_case("ModelFile") {
            Ok(Self::Model)
        } else if value.eq_ignore_ascii_case("ParticleFile") {
            Ok(Self::Particle)
        } else if value.eq_ignore_ascii_case("TerrainEffect") {
            Ok(Self::TerrainEffect)
        } else if value.eq_ignore_ascii_case("LightFile") {
            Ok(Self::Light)
        } else {
            Err(UnitLoadError::UnsupportedAttachmentType(value.to_owned()))
        }
    }
}

/// Runtime condition under which an authored attachment is active.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnitAttachmentTrigger {
    /// Component attachment that remains active with the unit.
    Persistent,
    /// Attachment started and stopped with a named animation state.
    Animation(String),
}

impl UnitAttachmentTrigger {
    /// Returns whether this trigger is active for the supplied animation.
    #[must_use]
    pub fn matches_animation(&self, animation: Option<&str>) -> bool {
        match self {
            Self::Persistent => true,
            Self::Animation(name) => name.eq_ignore_ascii_case(animation.unwrap_or("Idle")),
        }
    }

    /// Returns whether either active retail animation track owns this trigger.
    #[must_use]
    pub fn matches_animations(
        &self,
        action_animation: Option<&str>,
        movement_animation: Option<&str>,
    ) -> bool {
        self.matches_animation(action_animation)
            || movement_animation.is_some_and(|movement| self.matches_animation(Some(movement)))
    }
}

/// Authored attachment metadata anchored to a resolved component bone.
///
/// Persistent model attachments are already represented in [`Unit`]'s model
/// instances. Animation-triggered model attachments remain descriptors so the
/// simulation can activate them at the correct event without making them
/// permanently visible.
#[derive(Clone, Debug)]
pub struct UnitAttachment {
    /// Component model that owns the attachment.
    pub component: String,
    /// Renderer destination encoded by the visual's `type` attribute.
    pub kind: UnitAttachmentKind,
    /// Authored attachment name/reference.
    pub name: String,
    /// Canonical direct asset path when one can be determined without loading
    /// an animation-triggered child model. Model references with a malformed
    /// target remain `None` and can be diagnosed by the caller.
    pub asset_path: Option<String>,
    /// Parent target bone, if authored.
    pub to_bone: Option<String>,
    /// Child source bone, if authored.
    pub from_bone: Option<String>,
    /// Whether child and parent animation clocks should be synchronized.
    pub sync_animations: bool,
    /// Persistent or animation-triggered lifetime.
    pub trigger: UnitAttachmentTrigger,
    anchor_transform: Mat4,
    anchor_binding: Option<AttachmentAnchor>,
    target_bone_resolved: bool,
}

impl UnitAttachment {
    /// Returns the component-local attachment anchor in unit space.
    #[must_use]
    pub fn anchor_transform(&self) -> Mat4 {
        self.anchor_transform
    }

    /// Applies a unit-to-world transform to this attachment anchor.
    #[must_use]
    pub fn world_transform(&self, unit_transform: Mat4) -> Mat4 {
        unit_transform * self.anchor_transform
    }

    /// Returns whether an authored `tobone` resolved in the decoded UGX
    /// skeleton. Attachments without a `tobone` are considered resolved.
    #[must_use]
    pub fn target_bone_resolved(&self) -> bool {
        self.target_bone_resolved
    }
}

#[derive(Default)]
struct LoadedUnit {
    instances: Vec<UnitInstance>,
    attachments: Vec<UnitAttachment>,
}

impl LoadedUnit {
    fn append(&mut self, mut other: Self) {
        self.instances.append(&mut other.instances);
        self.attachments.append(&mut other.attachments);
    }
}

/// A decoded unit assembled from a visual's recursive UGX attachments.
///
/// Halo Wars visual files describe wheels, turrets, passengers, and similar
/// parts as separate models. Their placement is driven by a parent `tobone`
/// and optional child `frombone`, rather than by one flattened mesh.
#[derive(Debug)]
pub struct Unit {
    instances: Vec<UnitInstance>,
    attachments: Vec<UnitAttachment>,
    bounds_min: Vec3,
    bounds_max: Vec3,
}

impl Unit {
    /// Loads the default model and all recursive `ModelRef`/`ModelFile`
    /// attachments from a visual definition.
    ///
    /// Component `asset` entries are used directly. Runtime tech-state logic
    /// is deliberately left to the caller because a visual alone does not
    /// identify the active tech level.
    ///
    /// # Errors
    ///
    /// Returns an error when the visual graph is malformed or any referenced
    /// model cannot be loaded.
    pub fn load(
        source: &mut AssetSource<StdFileProvider>,
        visual: &Visual,
    ) -> Result<Self, UnitLoadError> {
        Self::load_variant(source, visual, None)
    }

    /// Loads a visual while selecting one stable `Variation` logic entry.
    ///
    /// Scenario objects carry a zero-based visual variation index. The
    /// original renderer keeps that index stable for the object's lifetime and
    /// clamps it to the final entry of each component's variation list. When no
    /// index is supplied, entry zero is used as a deterministic preview
    /// fallback.
    ///
    /// # Errors
    ///
    /// Returns an error when the visual graph is malformed or any referenced
    /// model cannot be loaded.
    pub fn load_variant(
        source: &mut AssetSource<StdFileProvider>,
        visual: &Visual,
        variation_index: Option<usize>,
    ) -> Result<Self, UnitLoadError> {
        let mut cache = UnitAssetCache::default();
        Self::load_variant_with_cache(source, visual, variation_index, &mut cache)
    }

    pub(super) fn load_variant_with_cache(
        source: &mut AssetSource<StdFileProvider>,
        visual: &Visual,
        variation_index: Option<usize>,
        cache: &mut UnitAssetCache,
    ) -> Result<Self, UnitLoadError> {
        Self::load_variant_with_animation_cache(
            source,
            visual,
            variation_index,
            UnitAnimationRequest::default(),
            cache,
        )
    }

    pub(super) fn load_variant_with_animation_cache(
        source: &mut AssetSource<StdFileProvider>,
        visual: &Visual,
        variation_index: Option<usize>,
        animation: UnitAnimationRequest<'_>,
        cache: &mut UnitAssetCache,
    ) -> Result<Self, UnitLoadError> {
        let default_model = visual
            .default_model
            .as_deref()
            .ok_or(UnitLoadError::MissingDefaultModel)?;
        let selection = VisualSelection {
            visual,
            variation_index,
        };
        let mut context = UnitLoadContext {
            selection,
            animation_type: animation.animation_type.map(str::to_owned),
            animation_asset: animation.animation_asset.map(str::to_owned),
            uses_simulation_clock: animation.uses_simulation_clock,
            animation_roll: animation.animation_roll,
            movement_animation_type: animation.movement_animation_type.map(str::to_owned),
            movement_animation_roll: animation.movement_animation_roll,
            cache,
            next_instance_index: 0,
        };
        let loaded =
            load_named_model(source, &mut context, default_model, None, Mat4::IDENTITY, 0)?;
        let (bounds_min, bounds_max) = unit_bounds(&loaded.instances);
        Ok(Self {
            instances: loaded.instances,
            attachments: loaded.attachments,
            bounds_min,
            bounds_max,
        })
    }

    /// Returns the number of rendered model instances, including repeated
    /// references such as the Warthog's four wheels.
    #[must_use]
    pub fn component_count(&self) -> usize {
        self.instances.len()
    }

    /// Returns the total triangle count across every model instance.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.instances
            .iter()
            .map(|instance| instance.model.triangle_count())
            .sum()
    }

    /// Returns the assembled unit's local-space lower bound.
    #[must_use]
    pub fn bounds_min(&self) -> [f32; 3] {
        self.bounds_min.to_array()
    }

    /// Returns the assembled unit's local-space upper bound.
    #[must_use]
    pub fn bounds_max(&self) -> [f32; 3] {
        self.bounds_max.to_array()
    }

    /// Returns the visual model names in render order.
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.instances.iter().map(|instance| instance.name.as_str())
    }

    /// Returns every persistent and animation-triggered authored attachment.
    #[must_use]
    pub fn attachments(&self) -> &[UnitAttachment] {
        &self.attachments
    }

    /// Whether this decoded placement carries the UAX selected by the sim.
    #[must_use]
    pub fn has_scripted_animation(&self) -> bool {
        self.instances.iter().any(|instance| {
            instance.action_animation.uses_simulation_clock && instance.action_animation.has_clip()
        })
    }

    pub(super) fn animation_tags(&self) -> impl Iterator<Item = &VisualTag> {
        self.instances.iter().flat_map(|instance| {
            instance.action_animation.tags().chain(
                instance
                    .movement_animation
                    .iter()
                    .flat_map(LoadedAnimationTrack::tags),
            )
        })
    }

    /// Returns attachments active for a named animation while always retaining
    /// persistent component attachments.
    pub fn attachments_for_animation<'unit>(
        &'unit self,
        animation: Option<&'unit str>,
    ) -> impl Iterator<Item = &'unit UnitAttachment> {
        self.attachments_for_animations(animation, None)
    }

    /// Returns attachments active on either the action or movement track.
    pub fn attachments_for_animations<'unit>(
        &'unit self,
        action_animation: Option<&'unit str>,
        movement_animation: Option<&'unit str>,
    ) -> impl Iterator<Item = &'unit UnitAttachment> {
        self.attachments.iter().filter(move |attachment| {
            attachment
                .trigger
                .matches_animations(action_animation, movement_animation)
        })
    }

    /// Counts attachment target bones that did not resolve in decoded UGX
    /// geometry. The original runtime tolerates these and omits the bone-side
    /// transform, so they remain visible as diagnostics rather than load errors.
    #[must_use]
    pub fn unresolved_attachment_bone_count(&self) -> usize {
        self.attachments
            .iter()
            .filter(|attachment| !attachment.target_bone_resolved())
            .count()
    }
}

#[derive(Clone, Copy)]
struct VisualSelection<'visual> {
    visual: &'visual Visual,
    variation_index: Option<usize>,
}

struct UnitLoadContext<'visual, 'cache> {
    selection: VisualSelection<'visual>,
    animation_type: Option<String>,
    animation_asset: Option<String>,
    uses_simulation_clock: bool,
    animation_roll: u64,
    movement_animation_type: Option<String>,
    movement_animation_roll: u64,
    cache: &'cache mut UnitAssetCache,
    next_instance_index: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct UnitAnimationRequest<'animation> {
    pub(super) animation_type: Option<&'animation str>,
    pub(super) animation_asset: Option<&'animation str>,
    pub(super) uses_simulation_clock: bool,
    pub(super) animation_roll: u64,
    pub(super) movement_animation_type: Option<&'animation str>,
    pub(super) movement_animation_roll: u64,
}

#[derive(Clone, Copy)]
struct ParentAttachment<'instance> {
    instance_index: usize,
    model: &'instance Model,
    pose: &'instance ModelPose,
    attachment: &'instance Attachment,
    action_asset_index: Option<usize>,
    movement_asset_index: Option<usize>,
}

fn load_named_model(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    name: &str,
    parent: Option<ParentAttachment<'_>>,
    parent_transform: Mat4,
    depth: usize,
) -> Result<LoadedUnit, UnitLoadError> {
    if depth >= MAX_ATTACHMENT_DEPTH {
        return Err(UnitLoadError::AttachmentDepthExceeded);
    }
    let definition = context
        .selection
        .visual
        .models
        .iter()
        .find(|model| model.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| UnitLoadError::ModelReferenceNotFound(name.to_owned()))?;
    let path = model_asset_path(
        context.selection.visual,
        definition,
        context.selection.variation_index,
    );
    if let Some(path) = path {
        return loading::load_model_definition(
            source,
            context,
            definition,
            path,
            parent,
            parent_transform,
            depth,
        );
    }
    if parent.is_none() {
        return attachment_only::load(definition, parent_transform);
    }
    Err(UnitLoadError::ModelAssetMissing(definition.name.clone()))
}

fn load_cached_model(
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut UnitAssetCache,
    component: &str,
    canonical_path: &str,
) -> Result<Arc<Model>, UnitLoadError> {
    let key = canonical_path.to_ascii_lowercase();
    if let Some(model) = cache.models.get(&key) {
        return Ok(Arc::clone(model));
    }
    let model =
        Arc::new(
            Model::load(source, canonical_path).map_err(|source| UnitLoadError::Model {
                component: component.to_owned(),
                path: canonical_path.to_owned(),
                source,
            })?,
        );
    cache.models.insert(key, Arc::clone(&model));
    Ok(model)
}

fn make_unit_attachment(
    selection: VisualSelection<'_>,
    owner: loading::AttachmentOwner<'_>,
    attachment: &Attachment,
    trigger: UnitAttachmentTrigger,
) -> Result<UnitAttachment, UnitLoadError> {
    let kind = UnitAttachmentKind::from_authored(&attachment.attach_type)?;
    let to_bone_transform = attachment.to_bone.as_deref().and_then(|name| {
        instance_attachment::find_bone(owner.model, owner.pose, name, "tobone", &attachment.name)
    });
    let target_bone_resolved = attachment.to_bone.is_none() || to_bone_transform.is_some();
    let asset_path = match kind {
        UnitAttachmentKind::ModelReference => selection
            .visual
            .models
            .iter()
            .find(|model| model.name.eq_ignore_ascii_case(&attachment.name))
            .and_then(|model| model_asset_path(selection.visual, model, selection.variation_index))
            .map(canonical_model_path),
        UnitAttachmentKind::Model => Some(canonical_model_path(&attachment.name)),
        UnitAttachmentKind::Particle
        | UnitAttachmentKind::TerrainEffect
        | UnitAttachmentKind::Light => Some(canonical_art_path(&attachment.name)),
    };

    Ok(UnitAttachment {
        component: owner.definition.name.clone(),
        kind,
        name: attachment.name.clone(),
        asset_path,
        to_bone: attachment.to_bone.clone(),
        from_bone: attachment.from_bone.clone(),
        sync_animations: attachment.sync_anims.unwrap_or(false),
        trigger,
        anchor_transform: owner.local_transform
            * instance_attachment::attachment_transform(
                to_bone_transform,
                None,
                attachment.disregard_orient.unwrap_or(false),
            ),
        anchor_binding: Some(AttachmentAnchor::new(owner.instance_index, attachment)),
        target_bone_resolved,
    })
}

fn unit_bounds(instances: &[UnitInstance]) -> (Vec3, Vec3) {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for instance in instances {
        let local_min = Vec3::from_array(instance.model.bounds_min());
        let local_max = Vec3::from_array(instance.model.bounds_max());
        for x in [local_min.x, local_max.x] {
            for y in [local_min.y, local_max.y] {
                for z in [local_min.z, local_max.z] {
                    let point = instance
                        .local_transform
                        .transform_point3(Vec3::new(x, y, z));
                    minimum = minimum.min(point);
                    maximum = maximum.max(point);
                }
            }
        }
    }
    if minimum.is_finite() && maximum.is_finite() {
        (minimum, maximum)
    } else {
        (Vec3::ZERO, Vec3::ZERO)
    }
}

struct RenderedUnitInstance {
    name: String,
    visible: bool,
    local_transform: Mat4,
    model: Arc<Model>,
    pose: ModelPose,
    action_animation: RenderedAnimationTrack,
    movement_animation: Option<RenderedAnimationTrack>,
    attachment: Option<InstanceAttachment>,
    renderer: Renderer,
}

/// GPU resources for every model instance in a decoded [`Unit`].
pub struct UnitRenderer {
    instances: Vec<RenderedUnitInstance>,
    attachments: Vec<UnitAttachment>,
    unit_transform: Mat4,
    active_action_animation: Option<String>,
    active_movement_animation: Option<String>,
    ik: ik::UnitIkRuntime,
}

impl UnitRenderer {
    /// Uploads every component in an assembled unit.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        unit: &Unit,
        unit_transform: Mat4,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            unit,
            unit_transform,
            WorldBindings::default(),
        )
    }

    /// Uploads every component with a scenario-global environment fallback.
    #[must_use]
    pub fn new_with_environment(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        unit: &Unit,
        unit_transform: Mat4,
        environment: Option<&EnvironmentMap>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            unit,
            unit_transform,
            WorldBindings {
                environment,
                ..WorldBindings::default()
            },
        )
    }

    /// Uploads every component with environment and directional-shadow inputs.
    #[must_use]
    pub fn new_with_environment_and_shadow(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        unit: &Unit,
        unit_transform: Mat4,
        environment: Option<&EnvironmentMap>,
        shadow_view: Option<&wgpu::TextureView>,
    ) -> Self {
        Self::new_with_world(
            device,
            queue,
            surface_format,
            unit,
            unit_transform,
            WorldBindings {
                environment,
                directional_shadow: shadow_view,
                ..WorldBindings::default()
            },
        )
    }

    /// Uploads every component with all scenario-global rendering inputs.
    #[must_use]
    pub fn new_with_world(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        unit: &Unit,
        unit_transform: Mat4,
        world: WorldBindings<'_>,
    ) -> Self {
        let resources = RendererResources::new_with_world(device, queue, surface_format, world);
        Self::new_with_resources(device, queue, unit, unit_transform, &resources)
    }

    /// Uploads every component using an existing scenario-global pipeline set.
    #[must_use]
    pub fn new_with_resources(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        unit: &Unit,
        unit_transform: Mat4,
        resources: &RendererResources,
    ) -> Self {
        Self::new_with_shared(device, queue, unit, unit_transform, &resources.shared)
    }

    pub(super) fn new_with_shared(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        unit: &Unit,
        unit_transform: Mat4,
        shared: &Arc<SharedResources>,
    ) -> Self {
        Self::new_with_shared_mesh_mask(device, queue, unit, unit_transform, shared, None)
    }

    pub(super) fn new_with_shared_mesh_mask(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        unit: &Unit,
        unit_transform: Mat4,
        shared: &Arc<SharedResources>,
        mesh_mask: Option<&sim::UnitVisualMeshMask>,
    ) -> Self {
        let visibility = mesh_mask.map_or_else(MeshVisibility::default, |mask| MeshVisibility {
            only: mask.only(),
            hidden: mask.hidden(),
            section_overrides: mask.section_overrides(),
        });
        let instances = unit
            .instances
            .iter()
            .map(|instance| {
                let model_transform = unit_transform * instance.local_transform;
                RenderedUnitInstance {
                    name: instance.name.clone(),
                    visible: mesh_mask.is_none_or(|mask| mask.is_component_visible(&instance.name)),
                    local_transform: instance.local_transform,
                    model: Arc::clone(&instance.model),
                    pose: instance.pose.clone(),
                    action_animation: RenderedAnimationTrack::from_loaded(
                        &instance.action_animation,
                    ),
                    movement_animation: instance
                        .movement_animation
                        .as_ref()
                        .map(RenderedAnimationTrack::from_loaded),
                    attachment: instance.attachment.clone(),
                    renderer: {
                        let renderer = Renderer::new_with_shared_visibility(
                            device,
                            queue,
                            &instance.model,
                            model_transform,
                            Arc::clone(shared),
                            visibility,
                        );
                        renderer.update_joints(queue, instance.pose.joint_matrices());
                        renderer
                    },
                }
            })
            .collect();
        Self {
            instances,
            attachments: unit.attachments.clone(),
            unit_transform,
            active_action_animation: None,
            active_movement_animation: None,
            ik: ik::UnitIkRuntime::default(),
        }
    }

    /// Updates all component transforms and shared frame lighting.
    pub fn update_frame(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        unit_transform: Mat4,
        lighting: &LightingParams,
    ) {
        self.update_frame_at_time(queue, view_projection, unit_transform, lighting, 0.0);
    }

    /// Updates transforms, lighting, and animated legacy material UVs.
    pub fn update_frame_at_time(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        unit_transform: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
    ) {
        self.update_frame_with_visual_state_at_time(
            queue,
            view_projection,
            unit_transform,
            lighting,
            time_seconds,
            VisualState::default(),
        );
    }

    pub(super) fn update_frame_with_visual_state_at_time(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        unit_transform: Mat4,
        lighting: &LightingParams,
        time_seconds: f32,
        visual_state: VisualState,
    ) {
        self.unit_transform = unit_transform;
        for instance in &mut self.instances {
            instance.renderer.update_frame_with_visual_state_at_time(
                queue,
                view_projection,
                unit_transform * instance.local_transform,
                lighting,
                time_seconds,
                visual_state,
            );
        }
    }

    pub(super) fn presentation_animation_duration(&self) -> f32 {
        self.instances
            .iter()
            .find_map(|instance| {
                instance
                    .movement_animation
                    .as_ref()
                    .and_then(RenderedAnimationTrack::presentation_duration)
                    .or_else(|| instance.action_animation.presentation_duration())
            })
            .unwrap_or_default()
    }

    fn animation_event(
        &self,
        instance_index: usize,
        tag: &VisualTag,
        source_owner_id: u64,
    ) -> Option<UnitAnimationEvent> {
        let instance = self.instances.get(instance_index)?;
        if let Some(event) =
            animation_events::camera_shake_event(tag, self.unit_transform, source_owner_id)
        {
            return Some(event);
        }
        if tag.tag_type.eq_ignore_ascii_case("TerrainAlpha") {
            let mut transform = self.unit_transform;
            if tag.to_bone.is_some() {
                let anchor = UnitAnimationAnchor {
                    instance_index,
                    to_bone: tag.to_bone.clone(),
                    disregard_orientation: true,
                };
                transform.w_axis = self.animation_anchor_world_transform(&anchor)?.w_axis;
            }
            return animation_events::terrain_alpha_event(tag, transform, source_owner_id);
        }
        let path = tag.name.as_deref()?.trim();
        if path.is_empty() {
            return None;
        }
        let anchor = UnitAnimationAnchor {
            instance_index,
            to_bone: tag.to_bone.clone(),
            disregard_orientation: tag.disregard_orient.unwrap_or(false),
        };
        let attached_transform = self.animation_anchor_world_transform(&anchor)?;
        let has_bone = anchor.to_bone.as_deref().is_some_and(|bone| {
            instance
                .model
                .posed_bone_to_model(&instance.pose, bone)
                .is_some()
        });
        let lifespan_seconds = tag
            .lifespan
            .filter(|lifespan| lifespan.is_finite())
            .unwrap_or(0.25)
            .max(0.0);
        let (kind, transform, anchor) = if tag.tag_type.eq_ignore_ascii_case("TerrainEffect") {
            (
                UnitAnimationEventKind::TerrainEffect(path.to_owned()),
                if has_bone {
                    attached_transform
                } else {
                    self.unit_transform
                },
                None,
            )
        } else if tag.tag_type.eq_ignore_ascii_case("Particle") {
            (
                UnitAnimationEventKind::Particle {
                    path: path.to_owned(),
                    lifespan_seconds,
                },
                attached_transform,
                Some(anchor),
            )
        } else if tag.tag_type.eq_ignore_ascii_case("Light") {
            (
                UnitAnimationEventKind::Light {
                    path: path.to_owned(),
                    lifespan_seconds,
                },
                attached_transform,
                Some(anchor),
            )
        } else {
            return None;
        };
        Some(UnitAnimationEvent {
            kind,
            transform,
            source_owner_id,
            anchor,
        })
    }

    pub(super) fn animation_anchor_world_transform(
        &self,
        anchor: &UnitAnimationAnchor,
    ) -> Option<Mat4> {
        let instance = self.instances.get(anchor.instance_index)?;
        let bone_transform = anchor
            .to_bone
            .as_deref()
            .and_then(|bone| instance.model.posed_bone_to_model(&instance.pose, bone));
        Some(
            bone_transform.map_or(self.unit_transform * instance.local_transform, |bone| {
                let bone = if anchor.disregard_orientation {
                    Mat4::from_translation(bone.w_axis.truncate())
                } else {
                    bone
                };
                self.unit_transform * instance.local_transform * bone
            }),
        )
    }

    /// Returns attachment metadata with anchors refreshed from the live poses.
    #[must_use]
    pub fn attachments(&self) -> &[UnitAttachment] {
        &self.attachments
    }

    /// Returns one live attachment anchor in world space.
    #[must_use]
    pub fn attachment_world_transform(&self, index: usize) -> Option<Mat4> {
        self.attachments
            .get(index)
            .map(|attachment| attachment.world_transform(self.unit_transform))
    }

    /// Draws the full recursive unit graph.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.render_phase(RenderPhase::World, pass);
    }

    /// Draws every component through the camera-relative sky pipeline.
    pub fn render_sky<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.render_phase(RenderPhase::Sky, pass);
    }

    /// Draws every component's authored screen-space distortion pass.
    pub fn render_distortion<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.render_phase(RenderPhase::Distortion, pass);
    }

    /// Draws every shadow-enabled component into one directional cascade.
    pub fn render_shadow<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>, cascade: usize) {
        self.render_phase(RenderPhase::Shadow { cascade }, pass);
    }

    /// Returns the current unit-to-world transform.
    #[must_use]
    pub fn unit_transform(&self) -> Mat4 {
        self.unit_transform
    }

    pub(in crate::ugx) fn active_action_animation_type(&self) -> Option<&str> {
        self.active_action_animation.as_deref()
    }

    pub(in crate::ugx) fn active_movement_animation_type(&self) -> Option<&str> {
        self.active_movement_animation.as_deref()
    }
}

impl WorldRenderer for UnitRenderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        for instance in &self.instances {
            if instance.visible {
                instance.renderer.render_phase(phase, pass);
            }
        }
    }
}
