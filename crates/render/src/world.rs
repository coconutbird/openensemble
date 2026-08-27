//! Common render-phase orchestration for world-space renderers.

use crate::wgpu;

/// A pass in the shared world rendering lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderPhase {
    /// Camera-relative background geometry.
    Sky,
    /// Lit world geometry, including authored transparent draw order.
    World,
    /// Signed screen-space distortion vectors.
    Distortion,
    /// One directional shadow-map cascade.
    Shadow {
        /// Zero-based cascade index.
        cascade: usize,
    },
}

/// A renderer that can contribute draws to the shared world lifecycle.
///
/// Implementors retain their specialized shaders, vertex layouts, and bind
/// groups. The trait unifies pass scheduling and draw dispatch; unsupported
/// phases are ignored by the implementation.
pub trait WorldRenderer {
    /// Records this renderer's draws for `phase` into an existing pass.
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>);
}

#[cfg(test)]
mod tests {
    use super::WorldRenderer;

    fn accepts_trait_object(_renderer: &dyn WorldRenderer) {}

    #[test]
    fn renderer_contract_is_object_safe() {
        let _ = accepts_trait_object;
    }
}
