//! `OpenEnsemble` Renderer
//!
//! Rendering subsystem using wgpu (pure Rust).
//! [`app`] provides the window loop with egui integration. [`input`] provides the
//! platform-neutral input of the application. [`world_view`] draws a loaded scenario.

pub mod app;
pub mod environment;
pub mod gpu;
pub mod input;
pub mod light_effect;
pub mod light_volume;
pub mod lighting;
pub mod local_shadow;
pub mod particle;
pub mod postprocess;
pub mod terrain;
pub mod terrain_effect;
pub mod ugx;
pub mod ui;
mod world;
pub mod world_view;

pub use world::{RenderPhase, WorldRenderer};

// Re-export wgpu for applications that need direct GPU access
pub use wgpu;
