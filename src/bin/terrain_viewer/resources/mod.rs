//! GPU resource creation for terrain rendering.
//!
//! This module is split into sub-modules for organization:
//! - `textures` — texture array and atlas creation
//! - `buffers` — storage and uniform buffer creation
//! - `pipelines` — render pipeline, bind group, and compositor setup

mod buffers;
mod pipelines;
mod textures;
