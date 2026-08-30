//! Bind hierarchy and posed bone transforms shared across runtime systems.

mod track_masks;

use glam::Mat4;
use pipeline::ugx::UgxGeom;

use crate::{AnimationPose, blended_local_transform};

pub use track_masks::upper_body_weights;

#[derive(Clone, Debug, PartialEq)]
struct Bone {
    name: String,
    parent_index: Option<usize>,
    bind_local: Mat4,
    bind_to_model: Mat4,
}

/// Decoded bind hierarchy for one UGX model component.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    bones: Vec<Bone>,
    upper_body_weights: Vec<f32>,
}

/// Local and model-space transforms for one sampled skeleton.
#[derive(Clone, Debug, Default)]
pub struct SkeletonPose {
    local_transforms: Vec<Mat4>,
    bone_to_model: Vec<Mat4>,
}

impl Skeleton {
    /// Build the same bind hierarchy selected by the retail-model renderer.
    #[must_use]
    pub fn from_geometry(geometry: &UgxGeom) -> Self {
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
        let bind_to_model = bones
            .iter()
            .map(|bone| bone.bind_to_model)
            .collect::<Vec<_>>();
        for (index, bone) in bones.iter_mut().enumerate() {
            bone.bind_local = bone
                .parent_index
                .filter(|&parent| parent < index)
                .map_or(bind_to_model[index], |parent| {
                    bind_to_model[parent].inverse() * bind_to_model[index]
                });
        }
        Self {
            bones,
            upper_body_weights: upper_body_weights(&geometry.granny_bones),
        }
    }

    /// Number of decoded bones in component order.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bones.len()
    }

    /// Whether this component has no decoded bones.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    /// Return a named bind-pose transform into component-model space.
    #[must_use]
    pub fn bind_bone_to_model(&self, name: &str) -> Option<Mat4> {
        self.bone(name).map(|bone| bone.bind_to_model)
    }

    /// Return a named bind-pose local transform.
    #[must_use]
    pub fn bind_local_transform(&self, name: &str) -> Option<Mat4> {
        self.bone(name).map(|bone| bone.bind_local)
    }

    /// Sample action and movement tracks using the authored upper-body mask.
    #[must_use]
    pub fn pose_tracks(
        &self,
        action: Option<&AnimationPose>,
        movement: Option<&AnimationPose>,
    ) -> SkeletonPose {
        let mut local_transforms = Vec::with_capacity(self.bones.len());
        let mut bone_to_model = Vec::with_capacity(self.bones.len());
        for (index, bone) in self.bones.iter().enumerate() {
            let action = action.and_then(|pose| pose.local_transform(&bone.name));
            let movement = movement.and_then(|pose| pose.local_transform(&bone.name));
            let local = movement.map_or_else(
                || action.unwrap_or(bone.bind_local),
                |movement| {
                    blended_local_transform(
                        action.unwrap_or(bone.bind_local),
                        movement,
                        self.upper_body_weights
                            .get(index)
                            .copied()
                            .unwrap_or_default(),
                    )
                },
            );
            local_transforms.push(local);
            let model = bone
                .parent_index
                .filter(|&parent| parent < index)
                .map_or(local, |parent| bone_to_model[parent] * local);
            bone_to_model.push(model);
        }
        SkeletonPose {
            local_transforms,
            bone_to_model,
        }
    }

    /// Sample one action clip over the bind pose.
    #[must_use]
    pub fn pose(&self, action: Option<&AnimationPose>) -> SkeletonPose {
        self.pose_tracks(action, None)
    }

    /// Resolve a named bone from a sampled pose into component-model space.
    #[must_use]
    pub fn posed_bone_to_model(&self, pose: &SkeletonPose, name: &str) -> Option<Mat4> {
        self.bone_index(name)
            .and_then(|index| pose.bone_to_model.get(index).copied())
    }

    /// Return an ancestor's posed parent/local frames and its transform to a descendant.
    #[must_use]
    pub fn posed_ancestor_frames(
        &self,
        pose: &SkeletonPose,
        ancestor: &str,
        descendant: &str,
    ) -> Option<(Mat4, Mat4, Mat4)> {
        let ancestor_index = self.bone_index(ancestor)?;
        let descendant_index = self.bone_index(descendant)?;
        if !self.is_ancestor(ancestor_index, descendant_index) {
            return None;
        }
        let bone = self.bones.get(ancestor_index)?;
        let parent = bone
            .parent_index
            .and_then(|index| pose.bone_to_model.get(index).copied())
            .unwrap_or(Mat4::IDENTITY);
        let local = pose.local_transforms.get(ancestor_index).copied()?;
        let bone_to_model = pose.bone_to_model.get(ancestor_index).copied()?;
        let descendant_to_model = pose.bone_to_model.get(descendant_index).copied()?;
        let determinant = bone_to_model.determinant();
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return None;
        }
        Some((parent, local, bone_to_model.inverse() * descendant_to_model))
    }

    fn is_ancestor(&self, ancestor: usize, mut descendant: usize) -> bool {
        loop {
            if descendant == ancestor {
                return true;
            }
            let Some(parent) = self
                .bones
                .get(descendant)
                .and_then(|bone| bone.parent_index)
                .filter(|&parent| parent < descendant)
            else {
                return false;
            };
            descendant = parent;
        }
    }

    fn bone(&self, name: &str) -> Option<&Bone> {
        self.bones
            .iter()
            .find(|bone| bone.name.eq_ignore_ascii_case(name))
    }

    fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(name))
    }
}

impl SkeletonPose {
    /// Local transform of a bone by component index.
    #[must_use]
    pub fn local_transform(&self, index: usize) -> Option<Mat4> {
        self.local_transforms.get(index).copied()
    }

    /// Model-space transform of a bone by component index.
    #[must_use]
    pub fn bone_to_model(&self, index: usize) -> Option<Mat4> {
        self.bone_to_model.get(index).copied()
    }
}

fn bind_pose_bone(name: &str, parent_index: i32, model_to_bone_rows: &[[f32; 4]; 4]) -> Bone {
    let inverse_bind = Mat4::from_cols_array_2d(model_to_bone_rows);
    let determinant = inverse_bind.determinant();
    let bind_to_model = if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
        log::warn!("Ignoring singular UGX bind matrix for bone '{name}'");
        Mat4::IDENTITY
    } else {
        inverse_bind.inverse()
    };
    Bone {
        name: name.to_owned(),
        parent_index: usize::try_from(parent_index).ok(),
        bind_local: Mat4::IDENTITY,
        bind_to_model,
    }
}

#[cfg(test)]
mod tests {
    use glam::{Mat4, Vec3};
    use pipeline::ugx::types::{Bone as UgxBone, Matrix4x4};
    use pipeline::ugx::{AABB, GeometryFlags, Sphere, UgxGeom};

    use super::Skeleton;

    #[test]
    fn child_bind_transform_is_relative_to_its_parent() {
        let root_to_model = Mat4::from_translation(Vec3::X * 2.0);
        let child_to_model = Mat4::from_translation(Vec3::new(2.0, 3.0, 0.0));
        let geometry = UgxGeom {
            bounding_sphere: Sphere::default(),
            bounds: AABB::default(),
            materials: Vec::new(),
            bones: vec![
                bone("root", -1, root_to_model),
                bone("child", 0, child_to_model),
            ],
            granny_bones: Vec::new(),
            granny_meshes: Vec::new(),
            skeleton_lod_type: 0,
            bone_bounds: Vec::new(),
            sections: Vec::new(),
            vertex_buffer: Vec::new(),
            index_buffer: Vec::new(),
            accessories: Vec::new(),
            valid_accessories: Vec::new(),
            rigid_only: false,
            rigid_bone_index: -1,
            max_instances: 1,
            instance_index_multiplier: 0,
            large_geom_bone_index: i16::MAX,
            flags: GeometryFlags::default(),
            aabb_tree: None,
        };
        let skeleton = Skeleton::from_geometry(&geometry);
        let pose = skeleton.pose(None);

        assert!(
            skeleton
                .posed_bone_to_model(&pose, "child")
                .unwrap()
                .w_axis
                .truncate()
                .abs_diff_eq(child_to_model.w_axis.truncate(), 1.0e-6)
        );
    }

    fn bone(name: &str, parent_index: i32, bone_to_model: Mat4) -> UgxBone {
        UgxBone {
            name: name.to_owned(),
            parent_index,
            model_to_bone: Matrix4x4 {
                rows: bone_to_model.inverse().to_cols_array_2d(),
            },
        }
    }
}
