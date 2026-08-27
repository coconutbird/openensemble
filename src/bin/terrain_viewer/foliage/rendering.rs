//! Foliage rendering logic.

use super::FoliageResources;
use crate::types::FoliageQNChunk;
use render::wgpu;

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
    let Some(params_bind_group) = &foliage.params_bind_group else {
        return;
    };

    render_pass.set_pipeline(&foliage.pipeline);
    render_pass.set_bind_group(0, camera_bind_group, &[]);
    draw_foliage(render_pass, foliage, params_bind_group);
}

/// Append foliage casters to every terrain cascade, preserving terrain depth.
pub fn render_foliage_shadow(
    encoder: &mut wgpu::CommandEncoder,
    foliage: &FoliageResources,
    shadow: &crate::shadow::ShadowResources,
) {
    if !foliage.config.enabled || foliage.draw_calls.is_empty() {
        return;
    }
    let Some(params_bind_group) = &foliage.shadow_params_bind_group else {
        return;
    };
    let cascade_count = usize::try_from(crate::shadow::SHADOW_CASCADE_COUNT)
        .expect("shadow cascade count must fit usize");
    for cascade in 0..cascade_count {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Foliage Shadow Cascade"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: shadow.cascade_shadow_view(cascade),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: shadow.cascade_depth_view(cascade),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&foliage.shadow_pipeline);
        pass.set_bind_group(0, shadow.cascade_camera_bind_group(cascade), &[]);
        draw_foliage(&mut pass, foliage, params_bind_group);
    }
}
