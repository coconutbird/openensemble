//! Foliage rendering logic.

use super::FoliageResources;
use crate::types::FoliageQNChunk;
use render::wgpu;

/// Render foliage using pre-built per-chunk draw calls.
///
/// Each draw call renders only the active blades parsed from the original
/// index buffers, using a dynamic uniform offset for chunk position and
/// a blade map texture for per-blade grid position and type lookup.
pub fn render_foliage<'a>(
    render_pass: &mut wgpu::RenderPass<'a>,
    foliage: &'a FoliageResources,
    camera_bind_group: &'a wgpu::BindGroup,
    _qn_chunks: &[FoliageQNChunk],
) {
    if !foliage.config.enabled || foliage.draw_calls.is_empty() {
        return;
    }

    let Some(params_bg) = &foliage.params_bind_group else {
        return;
    };

    render_pass.set_pipeline(&foliage.pipeline);
    render_pass.set_bind_group(0, camera_bind_group, &[]);

    for draw in &foliage.draw_calls {
        let Some(set_res) = foliage.set_resources.get(draw.set_index) else {
            continue;
        };

        if draw.num_active_blades == 0 {
            continue;
        }

        // Bind params with dynamic offset for this chunk
        render_pass.set_bind_group(1, params_bg, &[draw.dynamic_offset]);
        // Bind material + blade geometry for this set
        render_pass.set_bind_group(2, &set_res.material_bind_group, &[]);

        // Render only the active blades (not all 4096)
        render_pass.draw(0..draw.num_verts_per_blade, 0..draw.num_active_blades);
    }
}
