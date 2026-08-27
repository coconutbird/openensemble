//! `OpenEnsemble` Core
//!
//! Core types, traits, and utilities for the `OpenEnsemble` engine.
//! This crate provides foundational abstractions used across all other engine crates.

pub mod app;
pub mod error;
pub mod math;

pub use egui;
pub use glam;

/// Re-export common types
pub mod prelude {
    pub use crate::app::{
        Application, FrameContext, GamepadButton, GamepadState, Input, KeyCode, WindowConfig,
    };
    pub use crate::error::{Error, Result};
    pub use crate::math::*;
    pub use egui;
    pub use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
}
