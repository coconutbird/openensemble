use glam::{Mat4, Vec3};
use pipeline::database::hw1::Visual;
use pipeline::database::hw1::visual::{Attachment, Model as VisualModel};
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::uax::Reader as UaxReader;

use super::animation::AnimationPose;
use super::model::ModelPose;
use super::{LoadError, Model, Renderer};
use crate::terrain::LightingParams;

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
    /// A component animation was absent from the active asset stack.
    #[error("UAX animation for visual model '{component}' was not found: {path}")]
    AnimationNotFound {
        /// Named component in the visual graph.
        component: String,
        /// Resolved game asset path.
        path: String,
    },
    /// A component animation could not be decoded or sampled.
    #[error("failed to load animation for visual model '{component}' from '{path}': {reason}")]
    Animation {
        /// Named component in the visual graph.
        component: String,
        /// Resolved game asset path.
        path: String,
        /// Parser or pose-sampling diagnostic.
        reason: String,
    },
    /// The model-reference graph contains a cycle or is unreasonably deep.
    #[error("visual attachment graph exceeds {MAX_ATTACHMENT_DEPTH} levels")]
    AttachmentDepthExceeded,
}

#[derive(Debug)]
struct UnitInstance {
    name: String,
    model: Model,
    pose: ModelPose,
    local_transform: Mat4,
}

/// A decoded unit assembled from a visual's recursive UGX attachments.
///
/// Halo Wars visual files describe wheels, turrets, passengers, and similar
/// parts as separate models. Their placement is driven by a parent `tobone`
/// and optional child `frombone`, rather than by one flattened mesh.
#[derive(Debug)]
pub struct Unit {
    instances: Vec<UnitInstance>,
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
        let default_model = visual
            .default_model
            .as_deref()
            .ok_or(UnitLoadError::MissingDefaultModel)?;
        let instances = load_named_model(source, visual, default_model, None, Mat4::IDENTITY, 0)?;
        let (bounds_min, bounds_max) = unit_bounds(&instances);
        Ok(Self {
            instances,
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
}

fn load_named_model(
    source: &mut AssetSource<StdFileProvider>,
    visual: &Visual,
    name: &str,
    parent: Option<(&Model, &ModelPose, &Attachment)>,
    parent_transform: Mat4,
    depth: usize,
) -> Result<Vec<UnitInstance>, UnitLoadError> {
    if depth >= MAX_ATTACHMENT_DEPTH {
        return Err(UnitLoadError::AttachmentDepthExceeded);
    }
    let definition = visual
        .models
        .iter()
        .find(|model| model.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| UnitLoadError::ModelReferenceNotFound(name.to_owned()))?;
    let path = model_asset_path(definition)
        .ok_or_else(|| UnitLoadError::ModelAssetMissing(definition.name.clone()))?;
    load_model_definition(
        source,
        visual,
        definition,
        path,
        parent,
        parent_transform,
        depth,
    )
}

fn load_model_definition(
    source: &mut AssetSource<StdFileProvider>,
    visual: &Visual,
    definition: &VisualModel,
    path: &str,
    parent: Option<(&Model, &ModelPose, &Attachment)>,
    parent_transform: Mat4,
    depth: usize,
) -> Result<Vec<UnitInstance>, UnitLoadError> {
    let canonical_path = canonical_model_path(path);
    let model = Model::load(source, &canonical_path).map_err(|source| UnitLoadError::Model {
        component: definition.name.clone(),
        path: canonical_path,
        source,
    })?;
    let animation = load_start_animation(source, definition)?;
    let pose = model.pose(animation.as_ref());
    let local_transform = parent.map_or(
        parent_transform,
        |(parent_model, parent_pose, attachment)| {
            parent_transform
                * visual_attachment_transform(parent_model, parent_pose, &model, &pose, attachment)
        },
    );

    let mut children = Vec::new();
    if let Some(component) = &definition.component {
        for attachment in &component.attachments {
            if attachment.attach_type.eq_ignore_ascii_case("ModelRef") {
                children.extend(load_named_model(
                    source,
                    visual,
                    &attachment.name,
                    Some((&model, &pose, attachment)),
                    local_transform,
                    depth + 1,
                )?);
            } else if attachment.attach_type.eq_ignore_ascii_case("ModelFile") {
                children.extend(load_model_file(
                    source,
                    &model,
                    &pose,
                    attachment,
                    local_transform,
                    depth + 1,
                )?);
            }
        }
    }

    let mut instances = Vec::with_capacity(children.len() + 1);
    instances.push(UnitInstance {
        name: definition.name.clone(),
        model,
        pose,
        local_transform,
    });
    instances.extend(children);
    Ok(instances)
}

fn load_model_file(
    source: &mut AssetSource<StdFileProvider>,
    parent_model: &Model,
    parent_pose: &ModelPose,
    attachment: &Attachment,
    parent_transform: Mat4,
    depth: usize,
) -> Result<Vec<UnitInstance>, UnitLoadError> {
    if depth >= MAX_ATTACHMENT_DEPTH {
        return Err(UnitLoadError::AttachmentDepthExceeded);
    }
    let canonical_path = canonical_model_path(&attachment.name);
    let model = Model::load(source, &canonical_path).map_err(|source| UnitLoadError::Model {
        component: attachment.name.clone(),
        path: canonical_path,
        source,
    })?;
    let pose = model.pose(None);
    let local_transform = parent_transform
        * visual_attachment_transform(parent_model, parent_pose, &model, &pose, attachment);
    Ok(vec![UnitInstance {
        name: attachment.name.clone(),
        model,
        pose,
        local_transform,
    }])
}

fn model_asset_path(model: &VisualModel) -> Option<&str> {
    model.component.as_ref()?.assets.iter().find_map(|asset| {
        asset
            .asset_type
            .eq_ignore_ascii_case("Model")
            .then_some(asset.file.as_deref())
            .flatten()
    })
}

fn load_start_animation(
    source: &mut AssetSource<StdFileProvider>,
    model: &VisualModel,
) -> Result<Option<AnimationPose>, UnitLoadError> {
    let Some(path) = model
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
        })
    else {
        return Ok(None);
    };
    let canonical_path = canonical_animation_path(path);
    let bytes = source
        .resolve_with_fallback(&canonical_path, &[".uax"])
        .ok_or_else(|| UnitLoadError::AnimationNotFound {
            component: model.name.clone(),
            path: canonical_path.clone(),
        })?;
    let animation = UaxReader::read(&bytes).map_err(|error| UnitLoadError::Animation {
        component: model.name.clone(),
        path: canonical_path.clone(),
        reason: error.to_string(),
    })?;
    AnimationPose::at_start(&animation)
        .map(Some)
        .map_err(|error| UnitLoadError::Animation {
            component: model.name.clone(),
            path: canonical_path,
            reason: error.to_string(),
        })
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
        let instances = unit
            .instances
            .iter()
            .map(|instance| {
                let model_transform = unit_transform * instance.local_transform;
                RenderedUnitInstance {
                    local_transform: instance.local_transform,
                    renderer: {
                        let renderer = Renderer::new(
                            device,
                            queue,
                            surface_format,
                            &instance.model,
                            model_transform,
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
        self.unit_transform = unit_transform;
        for instance in &mut self.instances {
            instance.renderer.update_frame(
                queue,
                view_projection,
                unit_transform * instance.local_transform,
                lighting,
            );
        }
    }

    /// Draws the full recursive unit graph.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        for instance in &self.instances {
            instance.renderer.render(pass);
        }
    }

    /// Returns the current unit-to-world transform.
    #[must_use]
    pub fn unit_transform(&self) -> Mat4 {
        self.unit_transform
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};

    use super::{attachment_transform, canonical_animation_path, canonical_model_path};

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
}
