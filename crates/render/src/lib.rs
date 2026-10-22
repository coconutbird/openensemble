//! `OpenEnsemble` Renderer
//!
//! Rendering subsystem using wgpu (pure Rust).
//! [`app`] provides the window loop with egui integration. [`input`] provides the
//! platform-neutral input of the application. [`world_view`] draws a loaded scenario.
//! [`picking`] converts between screen and world positions. [`selection_decals`] and
//! [`hp_bars`] draw the selection and hit-point overlays of the game client.

pub mod app;
pub mod environment;
pub mod gpu;
pub mod hp_bars;
pub mod input;
pub mod light_effect;
pub mod light_volume;
pub mod lighting;
pub mod local_shadow;
pub mod particle;
pub mod picking;
pub mod postprocess;
pub mod selection_decals;
pub mod terrain;
pub mod terrain_effect;
pub mod ugx;
pub mod ui;
mod world;
pub mod world_view;

pub use world::{RenderPhase, WorldRenderer};

// Re-export wgpu for applications that need direct GPU access
pub use wgpu;
