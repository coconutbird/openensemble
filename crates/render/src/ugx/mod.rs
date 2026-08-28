//! GPU rendering for Ensemble UGX models.
//!
//! The implementation translates the legacy PC parametric shader family used
//! by Halo Wars: matrix-palette skinning, tangent-space normal reconstruction,
//! legacy material maps, reciprocal specular response, SH fill lighting, and
//! the four material blend modes.

mod animation;
mod model;
mod renderer;
mod scene;
mod unit;

pub use crate::terrain::TerrainHeightfield;
pub use model::{BlendMode, LoadError, Model};
pub use renderer::{Renderer, RendererResources, WorldBindings};
pub use scene::{
    ScenarioPositionAxes, UnitPlacement, UnitPlacementOrigin, UnitScene, UnitSceneIssue,
    UnitSceneRenderer, scenario_object_direction_to_world, scenario_object_position_to_world,
};
pub use unit::{
    Unit, UnitAttachment, UnitAttachmentKind, UnitAttachmentTrigger, UnitLoadError, UnitRenderer,
};
