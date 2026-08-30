//! Shared skeletal-animation sampling used by simulation and presentation.

mod animation;
mod attachment;
mod single_bone;
mod skeleton;

pub use animation::{AnimationPose, blended_local_transform};
pub use attachment::{attachment_transform, transformed_attachment_transform};
pub use single_bone::premultiplied_orientation_transform;
pub use skeleton::{Skeleton, SkeletonPose, upper_body_weights};
