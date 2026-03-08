//! Foliage rendering logic.

use super::FoliageResources;
use crate::types::FoliageQNChunk;
use render::wgpu;

/// Render foliage for visible chunks.
///
/// This renders all foliage within the view frustum, using instanced
/// rendering where each blade is an instance.
pub fn render_foliage<'a>(
    render_pass: &mut wgpu::RenderPass<'a>,
    foliage: &'a FoliageResources,
    camera_bind_group: &'a wgpu::BindGroup,
    _qn_chunks: &[FoliageQNChunk],
) {
    if !foliage.config.enabled || foliage.set_resources.is_empty() {
        return;
    }

    // Set pipeline and camera bind group
    render_pass.set_pipeline(&foliage.pipeline);
    render_pass.set_bind_group(0, camera_bind_group, &[]);

    // Set params bind group if available
    if let Some(params_bg) = &foliage.params_bind_group {
        render_pass.set_bind_group(1, params_bg, &[]);
    } else {
        // For now, skip rendering if params aren't set up
        // TODO: Create params bind group with heightmap
        log::debug!("Foliage: skipping render - params_bind_group not set");
        return;
    }

    // TODO: Implement per-chunk rendering based on QN data
    // For now, render a test pattern to verify the pipeline works
    for (set_idx, set_res) in foliage.set_resources.iter().enumerate() {
        // Set material bind group for this set
        render_pass.set_bind_group(2, &set_res.material_bind_group, &[]);

        // Render test blades across a larger area
        // Each "instance" represents a blade
        // 64x64 = 4096 blades covers one terrain chunk (64 grid cells)
        let verts_per_blade = set_res.num_verts_per_blade;
        let num_blades = 4096u32; // 64x64 grid of blades

        log::trace!(
            "Rendering foliage set {}: {} blades x {} verts = {} total verts",
            set_idx,
            num_blades,
            verts_per_blade,
            num_blades * verts_per_blade
        );
        render_pass.draw(0..verts_per_blade, 0..num_blades);
    }
}
