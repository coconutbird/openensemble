//! Foliage rendering logic.

use super::FoliageResources;
use render::{RenderPhase, WorldRenderer, wgpu};

fn draw_foliage<'a>(
    render_pass: &mut wgpu::RenderPass<'a>,
    foliage: &'a FoliageResources,
    params_bind_group: &'a wgpu::BindGroup,
) {
    for draw in &foliage.draw_calls {
        let Some(set_res) = foliage.set_resources.get(draw.set_index) else {
            continue;
        };
        if draw.num_active_blades == 0 {
            continue;
        }
        render_pass.set_bind_group(1, params_bind_group, &[draw.dynamic_offset]);
        render_pass.set_bind_group(2, &set_res.material_bind_group, &[]);
        render_pass.draw(0..draw.num_verts_per_blade, 0..draw.num_active_blades);
    }
}

impl WorldRenderer for FoliageResources {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        if !self.config.enabled || self.draw_calls.is_empty() {
            return;
        }
        let (pipeline, camera, params) = match phase {
            RenderPhase::World => (
                &self.pipeline,
                &self.camera_bind_group,
                self.params_bind_group.as_ref(),
            ),
            RenderPhase::Shadow { cascade } => {
                let Some(camera) = self.shadow_camera_bind_groups.get(cascade) else {
                    return;
                };
                (
                    &self.shadow_pipeline,
                    camera,
                    self.shadow_params_bind_group.as_ref(),
                )
            }
            RenderPhase::Sky | RenderPhase::Distortion | RenderPhase::LocalShadow { .. } => return,
        };
        let Some(params) = params else { return };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, camera, &[]);
        draw_foliage(pass, self, params);
    }
}
