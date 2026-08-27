use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};
use pipeline::database::hw1::Visual;
use pipeline::database::hw1::visual::{Attachment, Model as VisualModel};
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::uax::Reader as UaxReader;

use super::animation::AnimationPose;
use super::model::ModelPose;
use super::renderer::{RendererResources, SharedResources, WorldBindings};
use super::{LoadError, Model, Renderer};
use crate::environment::EnvironmentMap;
use crate::terrain::LightingParams;
use crate::{RenderPhase, WorldRenderer};

const MAX_ATTACHMENT_DEPTH: usize = 32;

/// Errors produced while resolving a visual's recursive model graph.
#[derive(Debug, thiserror::Error)]
pub enum UnitLoadError {
    /// The visual does not name a root model.
    #[error("visual has no default model")]
    MissingDefaultModel,
    /// A named model reference was absent from the visual.
    #[error("visual model reference not found: {0}")]
    ModelReferenceNotFound(String),
    /// A visual model had no direct UGX model asset.
    #[error("visual model '{0}' has no model asset")]
    ModelAssetMissing(String),
    /// A component UGX failed to resolve or decode.
    #[error("failed to load visual model '{component}' from '{path}': {source}")]
    Model {
        /// Named component in the visual graph.
        component: String,
        /// Resolved game asset path.
        path: String,
        /// Underlying UGX error.
        #[source]
        source: LoadError,
    },
    /// The model-reference graph contains a cycle or is unreasonably deep.
    #[error("visual attachment graph exceeds {MAX_ATTACHMENT_DEPTH} levels")]
    AttachmentDepthExceeded,
    /// The decoded visual contains an attachment outside the shipped schema.
    #[error("unsupported visual attachment type '{0}'")]
    UnsupportedAttachmentType(String),
}

#[derive(Debug)]
struct UnitInstance {
    name: String,
    model: Arc<Model>,
    pose: ModelPose,
    local_transform: Mat4,
}

#[derive(Default)]
pub(super) struct UnitAssetCache {
    models: HashMap<String, Arc<Model>>,
    animations: HashMap<String, Option<AnimationPose>>,
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
            Self::Animation(name) => {
                animation.is_some_and(|active| name.eq_ignore_ascii_case(active))
            }
        }
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
        let default_model = visual
            .default_model
            .as_deref()
            .ok_or(UnitLoadError::MissingDefaultModel)?;
        let selection = VisualSelection {
            visual,
            variation_index,
        };
        let mut context = UnitLoadContext { selection, cache };
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

    /// Returns attachments active for a named animation while always retaining
    /// persistent component attachments.
    pub fn attachments_for_animation<'unit>(
        &'unit self,
        animation: Option<&'unit str>,
    ) -> impl Iterator<Item = &'unit UnitAttachment> {
        self.attachments
            .iter()
            .filter(move |attachment| attachment.trigger.matches_animation(animation))
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
    cache: &'cache mut UnitAssetCache,
}

fn load_named_model(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    name: &str,
    parent: Option<(&Model, &ModelPose, &Attachment)>,
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
    )
    .ok_or_else(|| UnitLoadError::ModelAssetMissing(definition.name.clone()))?;
    load_model_definition(
        source,
        context,
        definition,
        path,
        parent,
        parent_transform,
        depth,
    )
}

fn load_model_definition(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    definition: &VisualModel,
    path: &str,
    parent: Option<(&Model, &ModelPose, &Attachment)>,
    parent_transform: Mat4,
    depth: usize,
) -> Result<LoadedUnit, UnitLoadError> {
    let canonical_path = canonical_model_path(path);
    let model = load_cached_model(source, context.cache, &definition.name, &canonical_path)?;
    let animation = load_start_animation(source, definition, context.cache);
    let pose = model.pose(animation.as_ref());
    let local_transform = parent.map_or(
        parent_transform,
        |(parent_model, parent_pose, attachment)| {
            parent_transform
                * visual_attachment_transform(parent_model, parent_pose, &model, &pose, attachment)
        },
    );

    let mut children = LoadedUnit::default();
    let mut attachments = Vec::new();
    if let Some(component) = &definition.component {
        for attachment in &component.attachments {
            let descriptor = make_unit_attachment(
                context.selection,
                definition,
                &model,
                &pose,
                local_transform,
                attachment,
                UnitAttachmentTrigger::Persistent,
            )?;
            match descriptor.kind {
                UnitAttachmentKind::ModelReference => children.append(load_named_model(
                    source,
                    context,
                    &attachment.name,
                    Some((&model, &pose, attachment)),
                    local_transform,
                    depth + 1,
                )?),
                UnitAttachmentKind::Model => children.append(load_model_file(
                    source,
                    context,
                    &model,
                    &pose,
                    attachment,
                    local_transform,
                    depth + 1,
                )?),
                UnitAttachmentKind::Particle
                | UnitAttachmentKind::TerrainEffect
                | UnitAttachmentKind::Light => {}
            }
            attachments.push(descriptor);
        }
    }

    for animation in &definition.anims {
        for attachment in &animation.attachments {
            attachments.push(make_unit_attachment(
                context.selection,
                definition,
                &model,
                &pose,
                local_transform,
                attachment,
                UnitAttachmentTrigger::Animation(animation.anim_type.clone()),
            )?);
        }
    }

    let mut loaded = LoadedUnit {
        instances: vec![UnitInstance {
            name: definition.name.clone(),
            model,
            pose,
            local_transform,
        }],
        attachments,
    };
    loaded.append(children);
    Ok(loaded)
}

fn load_model_file(
    source: &mut AssetSource<StdFileProvider>,
    context: &mut UnitLoadContext<'_, '_>,
    parent_model: &Model,
    parent_pose: &ModelPose,
    attachment: &Attachment,
    parent_transform: Mat4,
    depth: usize,
) -> Result<LoadedUnit, UnitLoadError> {
    if depth >= MAX_ATTACHMENT_DEPTH {
        return Err(UnitLoadError::AttachmentDepthExceeded);
    }
    let canonical_path = canonical_model_path(&attachment.name);
    let model = load_cached_model(source, context.cache, &attachment.name, &canonical_path)?;
    let pose = model.pose(None);
    let local_transform = parent_transform
        * visual_attachment_transform(parent_model, parent_pose, &model, &pose, attachment);
    Ok(LoadedUnit {
        instances: vec![UnitInstance {
            name: attachment.name.clone(),
            model,
            pose,
            local_transform,
        }],
        attachments: Vec::new(),
    })
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
    owner: &VisualModel,
    parent_model: &Model,
    parent_pose: &ModelPose,
    parent_transform: Mat4,
    attachment: &Attachment,
    trigger: UnitAttachmentTrigger,
) -> Result<UnitAttachment, UnitLoadError> {
    let kind = UnitAttachmentKind::from_authored(&attachment.attach_type)?;
    let to_bone_transform = attachment.to_bone.as_deref().and_then(|name| {
        find_attachment_bone(parent_model, parent_pose, name, "tobone", &attachment.name)
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
        component: owner.name.clone(),
        kind,
        name: attachment.name.clone(),
        asset_path,
        to_bone: attachment.to_bone.clone(),
        from_bone: attachment.from_bone.clone(),
        sync_animations: attachment.sync_anims.unwrap_or(false),
        trigger,
        anchor_transform: parent_transform * to_bone_transform.unwrap_or(Mat4::IDENTITY),
        target_bone_resolved,
    })
}

fn direct_model_asset_path(model: &VisualModel) -> Option<&str> {
    model.component.as_ref()?.assets.iter().find_map(|asset| {
        asset
            .asset_type
            .eq_ignore_ascii_case("Model")
            .then_some(asset.file.as_deref())
            .flatten()
    })
}

fn model_asset_path<'visual>(
    visual: &'visual Visual,
    model: &'visual VisualModel,
    variation_index: Option<usize>,
) -> Option<&'visual str> {
    let component = model.component.as_ref()?;
    let selected = component
        .logic
        .as_ref()
        .filter(|logic| logic.logic_type.eq_ignore_ascii_case("Variation"))
        .and_then(|logic| {
            let last = logic.entries.len().checked_sub(1)?;
            logic.entries.get(variation_index.unwrap_or(0).min(last))
        });
    if let Some(entry) = selected {
        if let Some(path) = entry.asset.as_ref().and_then(|asset| {
            asset
                .asset_type
                .eq_ignore_ascii_case("Model")
                .then_some(asset.file.as_deref())
                .flatten()
        }) {
            return Some(path);
        }
        if let Some(reference) = entry.model_ref.as_deref() {
            let referenced = visual
                .models
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(reference))?;
            if let Some(path) = direct_model_asset_path(referenced) {
                return Some(path);
            }
        }
    }
    direct_model_asset_path(model)
}

fn load_start_animation(
    source: &mut AssetSource<StdFileProvider>,
    model: &VisualModel,
    cache: &mut UnitAssetCache,
) -> Option<AnimationPose> {
    let path = model
        .anims
        .iter()
        .find(|animation| animation.anim_type.eq_ignore_ascii_case("Idle"))
        .and_then(|animation| {
            animation.assets.iter().find_map(|asset| {
                asset
                    .asset_type
                    .eq_ignore_ascii_case("Anim")
                    .then_some(asset.file.as_deref())
                    .flatten()
            })
        })?;
    let canonical_path = canonical_animation_path(path);
    let key = canonical_path.to_ascii_lowercase();
    if let Some(animation) = cache.animations.get(&key) {
        return animation.clone();
    }
    let Some(bytes) = source.resolve_with_fallback(&canonical_path, &[".uax"]) else {
        log::warn!(
            "UGX visual model '{}' is missing optional idle animation '{}'; using bind pose",
            model.name,
            canonical_path,
        );
        cache.animations.insert(key, None);
        return None;
    };
    let animation = match UaxReader::read(&bytes) {
        Ok(animation) => animation,
        Err(error) => {
            log::warn!(
                "UGX visual model '{}' could not decode optional idle animation '{}'; using bind pose: {error}",
                model.name,
                canonical_path,
            );
            cache.animations.insert(key, None);
            return None;
        }
    };
    let pose = AnimationPose::at_start(&animation);
    cache.animations.insert(key, Some(pose.clone()));
    Some(pose)
}

fn visual_attachment_transform(
    parent: &Model,
    parent_pose: &ModelPose,
    child: &Model,
    child_pose: &ModelPose,
    attachment: &Attachment,
) -> Mat4 {
    let to_bone = attachment.to_bone.as_deref().and_then(|name| {
        find_attachment_bone(parent, parent_pose, name, "tobone", &attachment.name)
    });
    let from_bone = attachment.from_bone.as_deref().and_then(|name| {
        find_attachment_bone(child, child_pose, name, "frombone", &attachment.name)
    });
    attachment_transform(to_bone, from_bone)
}

fn find_attachment_bone(
    model: &Model,
    pose: &ModelPose,
    bone_name: &str,
    role: &str,
    attachment_name: &str,
) -> Option<Mat4> {
    let matrix = model.posed_bone_to_model(pose, bone_name);
    if matrix.is_none() {
        // Missing attachment bones are valid in shipped visuals. The original
        // getRenderTransform path simply omits that side of the alignment.
        log::debug!("UGX attachment '{attachment_name}' has no {role} '{bone_name}'");
    }
    matrix
}

fn attachment_transform(to_bone: Option<Mat4>, from_bone: Option<Mat4>) -> Mat4 {
    match (to_bone, from_bone) {
        // Original row-vector expression: inverse(from) * to. Transposing
        // into glam's column-vector convention reverses that order.
        (Some(to), Some(from)) => to * from.inverse(),
        (Some(to), None) => to,
        (None, Some(from)) => from.inverse(),
        (None, None) => Mat4::IDENTITY,
    }
}

fn canonical_model_path(path: &str) -> String {
    canonical_art_path(path)
}

fn canonical_animation_path(path: &str) -> String {
    canonical_art_path(path)
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
    local_transform: Mat4,
    renderer: Renderer,
}

/// GPU resources for every model instance in a decoded [`Unit`].
pub struct UnitRenderer {
    instances: Vec<RenderedUnitInstance>,
    unit_transform: Mat4,
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
        let instances = unit
            .instances
            .iter()
            .map(|instance| {
                let model_transform = unit_transform * instance.local_transform;
                RenderedUnitInstance {
                    local_transform: instance.local_transform,
                    renderer: {
                        let renderer = Renderer::new_with_shared(
                            device,
                            queue,
                            &instance.model,
                            model_transform,
                            Arc::clone(shared),
                        );
                        renderer.update_joints(queue, instance.pose.joint_matrices());
                        renderer
                    },
                }
            })
            .collect();
        Self {
            instances,
            unit_transform,
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
        self.unit_transform = unit_transform;
        for instance in &mut self.instances {
            instance.renderer.update_frame_at_time(
                queue,
                view_projection,
                unit_transform * instance.local_transform,
                lighting,
                time_seconds,
            );
        }
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
}

impl WorldRenderer for UnitRenderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        for instance in &self.instances {
            instance.renderer.render_phase(phase, pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};
    use pipeline::database::hw1::Visual;
    use pipeline::database::hw1::visual::{
        Asset, Component, Logic, LogicEntry, Model as VisualModel,
    };

    use super::{
        UnitAttachmentKind, UnitAttachmentTrigger, UnitLoadError, attachment_transform,
        canonical_animation_path, canonical_model_path, model_asset_path,
    };

    #[test]
    fn shipped_attachment_kinds_are_classified_strictly() {
        for (authored, expected) in [
            ("ModelRef", UnitAttachmentKind::ModelReference),
            ("ModelFile", UnitAttachmentKind::Model),
            ("ParticleFile", UnitAttachmentKind::Particle),
            ("TerrainEffect", UnitAttachmentKind::TerrainEffect),
            ("LightFile", UnitAttachmentKind::Light),
        ] {
            assert_eq!(
                UnitAttachmentKind::from_authored(authored).unwrap(),
                expected
            );
        }
        assert!(matches!(
            UnitAttachmentKind::from_authored("UnknownAttachment"),
            Err(UnitLoadError::UnsupportedAttachmentType(value))
                if value == "UnknownAttachment"
        ));
    }

    #[test]
    fn animation_attachments_preserve_authored_trigger_lifetime() {
        assert!(UnitAttachmentTrigger::Persistent.matches_animation(None));
        assert!(UnitAttachmentTrigger::Persistent.matches_animation(Some("Idle")));

        let trigger = UnitAttachmentTrigger::Animation("Death".to_owned());
        assert!(trigger.matches_animation(Some("death")));
        assert!(!trigger.matches_animation(Some("Idle")));
        assert!(!trigger.matches_animation(None));
    }

    #[test]
    fn model_paths_are_resolved_from_art() {
        assert_eq!(
            canonical_model_path("unsc/vehicle/warthog_01/wheel_01"),
            "art\\unsc\\vehicle\\warthog_01\\wheel_01"
        );
        assert_eq!(
            canonical_model_path("art\\unsc\\vehicle\\warthog_01\\wheel_01"),
            "art\\unsc\\vehicle\\warthog_01\\wheel_01"
        );
        assert_eq!(
            canonical_animation_path("unsc/vehicle/warthog_01/idle_01"),
            "art\\unsc\\vehicle\\warthog_01\\idle_01"
        );
    }

    #[test]
    fn attachment_aligns_child_from_bone_to_parent_to_bone() {
        let to_bone = Mat4::from_translation(Vec3::new(10.0, 2.0, -3.0));
        let from_bone = Mat4::from_translation(Vec3::new(4.0, 1.0, -1.0));
        let transform = attachment_transform(Some(to_bone), Some(from_bone));
        let aligned = transform.transform_point3(from_bone.transform_point3(Vec3::ZERO));
        let expected = to_bone.transform_point3(Vec3::ZERO);
        assert!(aligned.abs_diff_eq(expected, 1.0e-5));
    }

    #[test]
    fn scenario_variation_index_selects_and_clamps_model_asset() {
        let model = VisualModel {
            name: "Default".to_owned(),
            component: Some(Component {
                logic: Some(Logic {
                    logic_type: "Variation".to_owned(),
                    entries: vec![
                        LogicEntry {
                            asset: Some(model_asset("crate_01")),
                            ..LogicEntry::default()
                        },
                        LogicEntry {
                            asset: Some(model_asset("crate_02")),
                            ..LogicEntry::default()
                        },
                    ],
                }),
                ..Component::default()
            }),
            ..VisualModel::default()
        };
        let visual = Visual {
            default_model: Some("Default".to_owned()),
            models: vec![model],
        };
        let model = &visual.models[0];
        assert_eq!(model_asset_path(&visual, model, Some(0)), Some("crate_01"));
        assert_eq!(model_asset_path(&visual, model, Some(1)), Some("crate_02"));
        assert_eq!(model_asset_path(&visual, model, Some(99)), Some("crate_02"));
    }

    fn model_asset(path: &str) -> Asset {
        Asset {
            asset_type: "Model".to_owned(),
            file: Some(path.to_owned()),
            ..Asset::default()
        }
    }
}
