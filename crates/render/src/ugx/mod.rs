//! GPU rendering for Ensemble UGX models.
//!
//! The implementation translates the legacy PC parametric shader family used
//! by Halo Wars: matrix-palette skinning, tangent-space normal reconstruction,
//! legacy material maps, reciprocal specular response, SH fill lighting, and
//! the four material blend modes.

mod animation;
mod model;
mod renderer;
mod unit;

pub use model::{BlendMode, LoadError, Model};
pub use renderer::Renderer;
pub use unit::{Unit, UnitLoadError, UnitRenderer};
