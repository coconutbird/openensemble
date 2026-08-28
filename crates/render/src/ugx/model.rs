use pipeline::ddx::{DataFormat, DdxTexture};
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::ugx::types::material::{BlendType, material_flags};
use pipeline::ugx::{
    LegacyMaterialData, MapType, Reader, Section as SourceSection, UgxGeom, UnpackedVertex,
};

use glam::Mat4;

use super::animation::AnimationPose;
use crate::environment::EnvironmentMap;

/// Errors produced while resolving or decoding a UGX model.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The requested model was absent from the active asset stack.
    #[error("UGX asset not found: {0}")]
    ModelNotFound(String),
    /// The UGX container could not be parsed.
    #[error("failed to parse UGX asset '{path}': {reason}")]
    Parse {
        /// Resolved game path.
        path: String,
        /// Parser diagnostic.
        reason: String,
    },
    /// A section referenced an invalid range or vertex.
    #[error("invalid UGX section {section}: {reason}")]
    InvalidSection {
        /// Zero-based section index.
        section: usize,
        /// Validation diagnostic.
        reason: String,
    },
}

/// Legacy UGX blend modes in draw order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendMode {
    /// Opaque rendering; the original uses alpha-to-coverage when MSAA is active.
    Opaque,
    /// Alpha-tested rendering with depth writes.
    AlphaTest,
    /// Source-over alpha blending.
    Over,
    /// Source-alpha additive blending.
    Additive,
}

impl BlendMode {
    pub(super) const DRAW_ORDER: [Self; 4] =
        [Self::Opaque, Self::AlphaTest, Self::Over, Self::Additive];

    fn from_legacy(raw: u8) -> Self {
        match BlendType::from_raw(raw) {
            BlendType::AlphaToCoverage => Self::Opaque,
            BlendType::Additive => Self::Additive,
            BlendType::Over => Self::Over,
            BlendType::AlphaTest => Self::AlphaTest,
        }
    }

    pub(super) fn rank(self) -> u8 {
        match self {
            Self::Opaque => 0,
            Self::AlphaTest => 1,
            Self::Over => 2,
            Self::Additive => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Image {
    pub(super) asset_path: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pixels: Vec<u8>,
    pub(super) hdr_scale: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MapChannels {
    pub(super) diffuse: u32,
    pub(super) normal: u32,
    pub(super) gloss: u32,
    pub(super) opacity: u32,
    pub(super) xform: u32,
    pub(super) emissive: u32,
    pub(super) ao: u32,
    pub(super) environment_mask: u32,
    pub(super) emissive_xform: u32,
    pub(super) distortion: u32,
    pub(super) highlight: u32,
    pub(super) modulate: u32,
}

#[derive(Clone, Debug)]
pub(super) struct Material {
    pub(super) name: String,
    pub(super) diffuse: Option<Image>,
    pub(super) normal: Option<Image>,
    pub(super) gloss: Option<Image>,
    pub(super) opacity_map: Option<Image>,
    pub(super) xform: Option<Image>,
    pub(super) emissive: Option<Image>,
    pub(super) ao: Option<Image>,
    pub(super) environment: Option<EnvironmentMap>,
    pub(super) environment_mask: Option<Image>,
    pub(super) emissive_xform: Option<Image>,
    pub(super) distortion: Option<Image>,
    pub(super) highlight: Option<Image>,
    pub(super) modulate: Option<Image>,
    pub(super) channels: MapChannels,
    pub(super) uv_velocity: [[f32; 2]; MapType::NUM_TYPES],
    pub(super) specular_color: [f32; 3],
    pub(super) specular_power: f32,
    pub(super) opacity: f32,
    pub(super) blend: BlendMode,
    features: MaterialFeatures,
    pub(super) environment_reflectivity: f32,
    pub(super) environment_sharpness: f32,
    pub(super) environment_fresnel: f32,
    pub(super) environment_fresnel_power: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MaterialFeature(u32);

impl MaterialFeature {
    pub(super) const TWO_SIDED: Self = Self(1 << 0);
    pub(super) const CASTS_SHADOWS: Self = Self(1 << 1);
    pub(super) const RECEIVES_SHADOWS: Self = Self(1 << 2);
    pub(super) const COLOR_GLOSS: Self = Self(1 << 3);
    pub(super) const GLOBAL_ENVIRONMENT: Self = Self(1 << 4);
    pub(super) const TERRAIN_CONFORM: Self = Self(1 << 5);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MaterialFeatures(u32);

impl Default for MaterialFeatures {
    fn default() -> Self {
        Self(MaterialFeature::CASTS_SHADOWS.0 | MaterialFeature::RECEIVES_SHADOWS.0)
    }
}

impl MaterialFeatures {
    fn contains(self, feature: MaterialFeature) -> bool {
        self.0 & feature.0 != 0
    }

    fn set(&mut self, feature: MaterialFeature, enabled: bool) {
        if enabled {
            self.0 |= feature.0;
        } else {
            self.0 &= !feature.0;
        }
    }
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: "default".to_owned(),
            diffuse: None,
            normal: None,
            gloss: None,
            opacity_map: None,
            xform: None,
            emissive: None,
            ao: None,
            environment: None,
            environment_mask: None,
            emissive_xform: None,
            distortion: None,
            highlight: None,
            modulate: None,
            channels: MapChannels::default(),
            uv_velocity: [[0.0; 2]; MapType::NUM_TYPES],
            specular_color: [1.0; 3],
            specular_power: 10.0,
            opacity: 1.0,
            blend: BlendMode::Opaque,
            features: MaterialFeatures::default(),
            environment_reflectivity: 1.0,
            environment_sharpness: 1.0,
            environment_fresnel: 0.5,
            environment_fresnel_power: 4.0,
        }
    }
}

impl Material {
    pub(super) fn map_velocity(&self, map_type: MapType) -> [f32; 2] {
        self.uv_velocity[map_index(map_type)]
    }

    pub(super) fn has_feature(&self, feature: MaterialFeature) -> bool {
        self.features.contains(feature)
    }

    #[cfg(test)]
    pub(super) fn set_feature(&mut self, feature: MaterialFeature, enabled: bool) {
        self.features.set(feature, enabled);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    pub(super) position: [f32; 3],
    pub(super) normal: [f32; 3],
    pub(super) tangent: [f32; 4],
    pub(super) binormal: [f32; 4],
    pub(super) texcoord0: [f32; 2],
    pub(super) texcoord1: [f32; 2],
    pub(super) texcoord2: [f32; 2],
    pub(super) color: [f32; 4],
    pub(super) joints: [u32; 4],
    pub(super) weights: [f32; 4],
}

#[derive(Clone, Debug)]
pub(super) struct Section {
    pub(super) vertices: Vec<Vertex>,
    pub(super) indices: Vec<u16>,
    pub(super) index_count: u32,
    pub(super) material_index: usize,
}

#[derive(Clone, Debug)]
struct Bone {
    name: String,
    parent_index: Option<usize>,
    bind_local: Mat4,
    inverse_bind: Mat4,
    bind_world: Mat4,
}

#[derive(Clone, Debug)]
pub(super) struct ModelPose {
    bone_to_model: Vec<Mat4>,
    joint_matrices: Vec<Mat4>,
}

/// A decoded UGX model ready for upload to the renderer.
#[derive(Clone, Debug)]
pub struct Model {
    pub(super) asset_path: String,
    pub(super) bounds_min: [f32; 3],
    pub(super) bounds_max: [f32; 3],
    pub(super) sphere_center: [f32; 3],
    pub(super) sphere_radius: f32,
    pub(super) materials: Vec<Material>,
    pub(super) sections: Vec<Section>,
    pub(super) joint_count: usize,
    bones: Vec<Bone>,
}

impl Model {
    /// Resolves a model and all legacy material maps from an ERA-backed source.
    ///
    /// `path` may include or omit the `.ugx` extension.
    ///
    /// # Errors
    ///
    /// Returns an error when the model is missing, malformed, or contains
    /// invalid section ranges. Missing or malformed material maps use their
    /// shader fallback so one optional texture cannot suppress the geometry.
    pub fn load(source: &mut AssetSource<StdFileProvider>, path: &str) -> Result<Self, LoadError> {
        let bytes = source
            .resolve_with_fallback(path, &[".ugx"])
            .ok_or_else(|| LoadError::ModelNotFound(path.to_owned()))?;
        let geometry = Reader::read(&bytes).map_err(|error| LoadError::Parse {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
        Self::from_geometry(source, &geometry, path)
    }

    /// Returns the model-space lower bound.
    #[must_use]
    pub fn bounds_min(&self) -> [f32; 3] {
        self.bounds_min
    }

    /// Returns the model-space upper bound.
    #[must_use]
    pub fn bounds_max(&self) -> [f32; 3] {
        self.bounds_max
    }

    /// Returns the model-space bounding-sphere center and radius.
    #[must_use]
    pub fn bounding_sphere(&self) -> ([f32; 3], f32) {
        (self.sphere_center, self.sphere_radius)
    }

    /// Returns the number of mesh sections.
    #[must_use]
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    /// Returns the total triangle count across all sections.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.indices.len() / 3)
            .sum()
    }

    /// Returns the bind-pose transform from a named bone into model space.
    ///
    /// Visual attachments use this matrix for their `tobone` and `frombone`
    /// alignment, matching the matrices returned by the original unit
    /// renderer's `getBoneForRender` path.
    #[must_use]
    pub fn bone_to_model(&self, name: &str) -> Option<Mat4> {
        self.bones
            .iter()
            .find(|bone| bone.name.eq_ignore_ascii_case(name))
            .map(|bone| bone.bind_world)
    }

    pub(super) fn pose(&self, animation: Option<&AnimationPose>) -> ModelPose {
        let mut bone_to_model = Vec::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let local = animation
                .and_then(|pose| pose.local_transform(&bone.name))
                .unwrap_or(bone.bind_local);
            let world = bone
                .parent_index
                .filter(|&parent| parent < index)
                .map_or_else(
                    || {
                        if bone.parent_index.is_some() {
                            log::warn!(
                                "UGX bone '{}' has an invalid parent; treating it as a root",
                                bone.name
                            );
                        }
                        local
                    },
                    |parent| bone_to_model[parent] * local,
                );
            bone_to_model.push(world);
        }

        // UGX reserves palette slot zero for the model/root transform. Bone
        // indices embedded in vertices and section rigid indices are offset
        // by one in the original renderer.
        let mut joint_matrices = vec![Mat4::IDENTITY; self.joint_count];
        for (index, (bone, world)) in self.bones.iter().zip(&bone_to_model).enumerate() {
            if let Some(joint) = joint_matrices.get_mut(index + 1) {
                *joint = *world * bone.inverse_bind;
            }
        }
        ModelPose {
            bone_to_model,
            joint_matrices,
        }
    }

    pub(super) fn posed_bone_to_model(&self, pose: &ModelPose, name: &str) -> Option<Mat4> {
        self.bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(name))
            .and_then(|index| pose.bone_to_model.get(index).copied())
    }

    fn from_geometry(
        source: &mut AssetSource<StdFileProvider>,
        geometry: &UgxGeom,
        asset_path: &str,
    ) -> Result<Self, LoadError> {
        let mut materials = geometry
            .materials
            .iter()
            .map(|material| load_material(source, material))
            .collect::<Vec<_>>();
        if materials.is_empty() {
            materials.push(Material::default());
        }

        let mut maximum_joint = 0_usize;
        let sections = geometry
            .sections
            .iter()
            .enumerate()
            .map(|(section_index, section)| {
                validate_section(geometry, section, section_index)?;
                let unpacked =
                    geometry
                        .unpack_section_vertices(section_index)
                        .map_err(|error| LoadError::InvalidSection {
                            section: section_index,
                            reason: error.to_string(),
                        })?;
                let indices = geometry
                    .get_section_indices(section_index)
                    .map_err(|error| LoadError::InvalidSection {
                        section: section_index,
                        reason: error.to_string(),
                    })?;
                if let Some(&invalid) = indices
                    .iter()
                    .find(|&&index| usize::from(index) >= unpacked.len())
                {
                    return Err(LoadError::InvalidSection {
                        section: section_index,
                        reason: format!(
                            "index {invalid} exceeds {} section vertices",
                            unpacked.len()
                        ),
                    });
                }
                let vertices = unpacked
                    .iter()
                    .map(|vertex| convert_vertex(vertex, section, &mut maximum_joint))
                    .collect();
                let material_index = usize::try_from(section.material_index)
                    .ok()
                    .filter(|&index| index < materials.len())
                    .unwrap_or(0);
                let index_count =
                    u32::try_from(indices.len()).map_err(|_| LoadError::InvalidSection {
                        section: section_index,
                        reason: "index count exceeds the GPU u32 draw range".to_owned(),
                    })?;
                Ok(Section {
                    vertices,
                    indices,
                    index_count,
                    material_index,
                })
            })
            .collect::<Result<Vec<_>, LoadError>>()?;

        let skeleton_joint_count = geometry
            .bones
            .len()
            .max(geometry.granny_bones.len())
            .saturating_add(1);
        let joint_count = skeleton_joint_count
            .max(maximum_joint.saturating_add(1))
            .max(1);
        Ok(Self {
            asset_path: asset_path.replace('/', "\\").to_ascii_lowercase(),
            bounds_min: geometry.bounds.min,
            bounds_max: geometry.bounds.max,
            sphere_center: geometry.bounding_sphere.center,
            sphere_radius: geometry.bounding_sphere.radius,
            materials,
            sections,
            joint_count,
            bones: load_bind_pose_bones(geometry),
        })
    }
}

fn load_bind_pose_bones(geometry: &UgxGeom) -> Vec<Bone> {
    let mut bones = if geometry.granny_bones.is_empty() {
        geometry
            .bones
            .iter()
            .map(|bone| bind_pose_bone(&bone.name, bone.parent_index, &bone.model_to_bone.rows))
            .collect::<Vec<_>>()
    } else {
        geometry
            .granny_bones
            .iter()
            .map(|bone| {
                bind_pose_bone(
                    &bone.name,
                    bone.parent_index,
                    &bone.inverse_world_matrix.rows,
                )
            })
            .collect::<Vec<_>>()
    };
    let bind_world = bones.iter().map(|bone| bone.bind_world).collect::<Vec<_>>();
    for (index, bone) in bones.iter_mut().enumerate() {
        bone.bind_local = bone
            .parent_index
            .filter(|&parent| parent < index)
            .map_or(bind_world[index], |parent| {
                bind_world[parent].inverse() * bind_world[index]
            });
    }
    bones
}

fn bind_pose_bone(name: &str, parent_index: i32, model_to_bone_rows: &[[f32; 4]; 4]) -> Bone {
    // UGX/Granny matrices use row vectors. Supplying each source row as a
    // glam column transposes it into the column-vector convention used by
    // the renderer.
    let inverse_bind = Mat4::from_cols_array_2d(model_to_bone_rows);
    let determinant = inverse_bind.determinant();
    let (inverse_bind, bind_world) =
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            log::warn!("Ignoring singular UGX bind matrix for bone '{name}'");
            (Mat4::IDENTITY, Mat4::IDENTITY)
        } else {
            (inverse_bind, inverse_bind.inverse())
        };
    Bone {
        name: name.to_owned(),
        parent_index: usize::try_from(parent_index).ok(),
        bind_local: Mat4::IDENTITY,
        inverse_bind,
        bind_world,
    }
}

fn validate_section(
    geometry: &UgxGeom,
    section: &SourceSection,
    section_index: usize,
) -> Result<(), LoadError> {
    let fail = |reason| LoadError::InvalidSection {
        section: section_index,
        reason,
    };
    let vb_offset = usize::try_from(section.vb_offset)
        .map_err(|_| fail("negative vertex-buffer offset".to_owned()))?;
    let vb_bytes = usize::try_from(section.vb_bytes)
        .map_err(|_| fail("negative vertex-buffer byte count".to_owned()))?;
    let vb_end = vb_offset
        .checked_add(vb_bytes)
        .ok_or_else(|| fail("vertex-buffer range overflow".to_owned()))?;
    if vb_end > geometry.vertex_buffer.len() {
        return Err(fail(format!(
            "vertex-buffer range {vb_offset}..{vb_end} exceeds {} bytes",
            geometry.vertex_buffer.len()
        )));
    }
    let ib_offset = usize::try_from(section.ib_offset)
        .map_err(|_| fail("negative index-buffer offset".to_owned()))?;
    let triangle_count = usize::try_from(section.num_tris)
        .map_err(|_| fail("negative triangle count".to_owned()))?;
    let index_count = triangle_count
        .checked_mul(3)
        .ok_or_else(|| fail("index count overflow".to_owned()))?;
    let ib_end = ib_offset
        .checked_add(index_count)
        .ok_or_else(|| fail("index-buffer range overflow".to_owned()))?;
    if ib_end > geometry.index_buffer.len() {
        return Err(fail(format!(
            "index-buffer range {ib_offset}..{ib_end} exceeds {} indices",
            geometry.index_buffer.len()
        )));
    }
    usize::try_from(section.num_verts).map_err(|_| fail("negative vertex count".to_owned()))?;
    Ok(())
}

fn convert_vertex(
    source: &UnpackedVertex,
    section: &SourceSection,
    maximum_joint: &mut usize,
) -> Vertex {
    let mut joints = [0_u32; 4];
    let mut weights = source.bone_weights;
    let weight_sum: f32 = weights.iter().sum();
    if weight_sum > f32::EPSILON {
        for (slot, weight) in weights.iter_mut().enumerate() {
            *weight /= weight_sum;
            if *weight > 0.0 {
                let mapped = resolve_joint(source.bone_indices[slot], &section.bone_remap);
                joints[slot] = u32::from(mapped);
                *maximum_joint = (*maximum_joint).max(usize::from(mapped));
            }
        }
    } else {
        let rigid = u16::try_from(section.rigid_bone_index.saturating_add(1)).unwrap_or(0);
        joints[0] = u32::from(rigid);
        weights = [1.0, 0.0, 0.0, 0.0];
        *maximum_joint = (*maximum_joint).max(usize::from(rigid));
    }

    let tangent = normalized_basis(source.tangent, [1.0, 0.0, 0.0, 1.0]);
    let binormal = normalized_basis(source.binormal, [0.0, 0.0, 1.0, 1.0]);
    let color = if section
        .base_vert_packer
        .as_ref()
        .is_some_and(|packer| packer.pack_order.contains(['D', 'd']))
    {
        source.diffuse
    } else {
        [1.0; 4]
    };
    Vertex {
        position: source.position,
        normal: normalized_vec3(source.normal, [0.0, 1.0, 0.0]),
        tangent,
        binormal,
        texcoord0: source.texcoords[0],
        texcoord1: source.texcoords[1],
        texcoord2: source.texcoords[2],
        color,
        joints,
        weights,
    }
}

fn resolve_joint(local: u16, remap: &[u8]) -> u16 {
    remap
        .get(usize::from(local))
        .map_or(local, |&global| u16::from(global))
}

impl ModelPose {
    pub(super) fn joint_matrices(&self) -> &[Mat4] {
        &self.joint_matrices
    }
}

fn normalized_vec3(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let vector = glam::Vec3::from_array(value);
    if vector.length_squared() > 1.0e-12 {
        vector.normalize().to_array()
    } else {
        fallback
    }
}

fn normalized_basis(value: [f32; 4], fallback: [f32; 4]) -> [f32; 4] {
    let xyz = normalized_vec3(
        [value[0], value[1], value[2]],
        [fallback[0], fallback[1], fallback[2]],
    );
    let handedness = if value[3] == 0.0 {
        fallback[3]
    } else {
        value[3].signum()
    };
    [xyz[0], xyz[1], xyz[2], handedness]
}

fn load_material(
    source: &mut AssetSource<StdFileProvider>,
    material: &pipeline::ugx::Material,
) -> Material {
    let Some(legacy) = material.legacy() else {
        log::warn!(
            "UGX material '{}' uses the unsupported Hogan material system; using defaults",
            material.name
        );
        return Material {
            name: material.name.clone(),
            ..Material::default()
        };
    };
    let (diffuse, diffuse_channel) = load_map(source, legacy, MapType::Diffuse, &material.name);
    let (normal, normal_channel) = load_map(source, legacy, MapType::Normal, &material.name);
    let (gloss, gloss_channel) = load_map(source, legacy, MapType::Gloss, &material.name);
    let (opacity_map, opacity_channel) = load_map(source, legacy, MapType::Opacity, &material.name);
    let (xform, xform_channel) = load_map(source, legacy, MapType::XForm, &material.name);
    let (emissive, emissive_channel) = load_map(source, legacy, MapType::Emissive, &material.name);
    let (ao, ao_channel) = load_map(source, legacy, MapType::AO, &material.name);
    let environment = load_environment_map(source, legacy, &material.name);
    let (environment_mask, environment_mask_channel) =
        load_map(source, legacy, MapType::EnvMask, &material.name);
    let (emissive_xform, emissive_xform_channel) =
        load_map(source, legacy, MapType::EmXForm, &material.name);
    let (distortion, distortion_channel) =
        load_map(source, legacy, MapType::Distortion, &material.name);
    let (highlight, highlight_channel) =
        load_map(source, legacy, MapType::Highlight, &material.name);
    let (modulate, modulate_channel) = load_map(source, legacy, MapType::Modulate, &material.name);
    let opacity = if legacy.flags & material_flags::OPACITY_VALID != 0 {
        legacy.opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    Material {
        name: material.name.clone(),
        diffuse,
        normal,
        gloss,
        opacity_map,
        xform,
        emissive,
        ao,
        environment,
        environment_mask,
        emissive_xform,
        distortion,
        highlight,
        modulate,
        channels: MapChannels {
            diffuse: diffuse_channel,
            normal: normal_channel,
            gloss: gloss_channel,
            opacity: opacity_channel,
            xform: xform_channel,
            emissive: emissive_channel,
            ao: ao_channel,
            environment_mask: environment_mask_channel,
            emissive_xform: emissive_xform_channel,
            distortion: distortion_channel,
            highlight: highlight_channel,
            modulate: modulate_channel,
        },
        uv_velocity: legacy
            .uvw_velocity
            .map(|velocity| [velocity[0], velocity[1]]),
        specular_color: legacy.spec_color,
        specular_power: legacy.spec_power.max(0.0001),
        opacity,
        blend: BlendMode::from_legacy(legacy.blend_type),
        features: material_features(legacy.flags),
        environment_reflectivity: legacy.env_reflectivity.max(0.0),
        environment_sharpness: legacy.env_sharpness,
        environment_fresnel: legacy.env_fresnel.clamp(0.0, 1.0),
        environment_fresnel_power: legacy.env_fresnel_power.max(0.0001),
    }
}

fn load_environment_map(
    source: &mut AssetSource<StdFileProvider>,
    material: &LegacyMaterialData,
    material_name: &str,
) -> Option<EnvironmentMap> {
    let map = material.maps[map_index(MapType::Env)].first()?;
    match EnvironmentMap::load(source, &map.name) {
        Ok(environment) => Some(environment),
        Err(error) => {
            log::warn!(
                "UGX material '{material_name}' could not load environment map '{}'; using the global/black fallback: {error}",
                map.name,
            );
            None
        }
    }
}

fn material_features(flags: u32) -> MaterialFeatures {
    let mut features = MaterialFeatures::default();
    features.set(
        MaterialFeature::TWO_SIDED,
        flags & material_flags::TWO_SIDED != 0,
    );
    features.set(
        MaterialFeature::CASTS_SHADOWS,
        flags & material_flags::DISABLE_SHADOWS == 0,
    );
    features.set(
        MaterialFeature::RECEIVES_SHADOWS,
        flags & material_flags::DISABLE_SHADOW_RECEPTION == 0,
    );
    features.set(
        MaterialFeature::COLOR_GLOSS,
        flags & material_flags::COLOR_GLOSS != 0,
    );
    features.set(
        MaterialFeature::GLOBAL_ENVIRONMENT,
        flags & material_flags::GLOBAL_ENV != 0,
    );
    features.set(
        MaterialFeature::TERRAIN_CONFORM,
        flags & material_flags::TERRAIN_CONFORM != 0,
    );
    features
}

fn load_map(
    source: &mut AssetSource<StdFileProvider>,
    material: &LegacyMaterialData,
    map_type: MapType,
    material_name: &str,
) -> (Option<Image>, u32) {
    let Some(map) = material.maps[map_index(map_type)].first() else {
        return (None, 0);
    };
    let channel = u32::try_from(map.channel.max(0)).unwrap_or(0).min(2);
    let path = canonical_texture_path(&map.name);
    let Some(bytes) = source.resolve_with_fallback(&path, &[".ddx"]) else {
        log::warn!(
            "UGX material '{material_name}' map {map_type:?} was not found at '{path}'; using the shader fallback"
        );
        return (None, channel);
    };
    let texture = match DdxTexture::from_bytes(&bytes) {
        Ok(texture) => texture,
        Err(error) => {
            log::warn!(
                "UGX material '{material_name}' map {map_type:?} at '{path}' could not be parsed; using the shader fallback: {error}"
            );
            return (None, channel);
        }
    };
    let hdr_scale = texture.info.hdr_scale;
    let data_format = texture.info.data_format;
    let mut decoded = match texture.decode_to_rgba() {
        Ok(decoded) => decoded,
        Err(error) => {
            log::warn!(
                "UGX material '{material_name}' map {map_type:?} at '{path}' could not be decoded; using the shader fallback: {error}"
            );
            return (None, channel);
        }
    };
    if map_type == MapType::Normal && data_format == DataFormat::Dxt5N {
        let (pixels, _) = decoded.pixels.as_chunks_mut::<4>();
        for pixel in pixels {
            pixel[0] = pixel[3];
            pixel[2] = 255;
            pixel[3] = 255;
        }
    }
    (
        Some(Image {
            asset_path: path,
            width: decoded.width,
            height: decoded.height,
            pixels: decoded.pixels,
            hdr_scale,
        }),
        channel,
    )
}

fn map_index(map_type: MapType) -> usize {
    match map_type {
        MapType::Diffuse => 0,
        MapType::Normal => 1,
        MapType::Gloss => 2,
        MapType::Opacity => 3,
        MapType::XForm => 4,
        MapType::Emissive => 5,
        MapType::AO => 6,
        MapType::Env => 7,
        MapType::EnvMask => 8,
        MapType::EmXForm => 9,
        MapType::Distortion => 10,
        MapType::Highlight => 11,
        MapType::Modulate => 12,
    }
}

fn canonical_texture_path(name: &str) -> String {
    let normalized = name
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

#[cfg(test)]
mod tests {
    use super::{BlendMode, canonical_texture_path, normalized_basis, resolve_joint};

    #[test]
    fn texture_paths_are_resolved_from_the_art_root() {
        assert_eq!(
            canonical_texture_path("\\unsc/vehicle/warthog_01/body_df"),
            "art\\unsc\\vehicle\\warthog_01\\body_df"
        );
        assert_eq!(
            canonical_texture_path("art\\unsc\\vehicle\\body_df.ddx"),
            "art\\unsc\\vehicle\\body_df.ddx"
        );
    }

    #[test]
    fn section_local_joints_use_the_bone_remap() {
        assert_eq!(resolve_joint(1, &[7, 12, 19]), 12);
        assert_eq!(resolve_joint(4, &[]), 4);
        assert_eq!(resolve_joint(4, &[7, 12]), 4);
    }

    #[test]
    fn zero_length_basis_uses_a_stable_fallback() {
        let actual = normalized_basis([0.0; 4], [1.0, 0.0, 0.0, 1.0]);
        let expected = [1.0, 0.0, 0.0, 1.0];
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| (actual - expected).abs() < f32::EPSILON)
        );
    }

    #[test]
    fn legacy_blend_values_map_to_the_oracle_passes() {
        assert_eq!(BlendMode::from_legacy(0), BlendMode::Opaque);
        assert_eq!(BlendMode::from_legacy(1), BlendMode::Additive);
        assert_eq!(BlendMode::from_legacy(2), BlendMode::Over);
        assert_eq!(BlendMode::from_legacy(3), BlendMode::AlphaTest);
        assert_eq!(BlendMode::from_legacy(255), BlendMode::Opaque);
    }
}
