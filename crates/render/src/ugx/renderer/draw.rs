use glam::Mat4;

use super::{BlendMode, GpuSection, Renderer};
use crate::{RenderPhase, WorldRenderer};

impl Renderer {
    /// Draws all sections using the oracle blend order.
    pub fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        pass.set_bind_group(2, &self.shared.shadow_bind_group, &[0]);
        for blend in BlendMode::DRAW_ORDER {
            for section in self.visible_sections() {
                let material = &self.gpu_model.materials[section.material_index];
                if material.blend != blend {
                    continue;
                }
                let pipelines = if uses_fade_pipeline(material.blend, self.opacity) {
                    &self.shared.fade_pipelines
                } else {
                    &self.shared.pipelines
                };
                pass.set_pipeline(&pipelines[material.pipeline_index]);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
                pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..section.index_count, 0, 0..1);
            }
        }
    }

    /// Draws this model as a camera-relative sky background.
    ///
    /// Sky visuals retain their authored UGX materials, but use a far-plane,
    /// depth-read-only pipeline so they cannot occlude world geometry.
    pub fn render_sky<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        pass.set_bind_group(2, &self.shared.shadow_bind_group, &[0]);
        for blend in BlendMode::DRAW_ORDER {
            for section in self.visible_sections() {
                let material = &self.gpu_model.materials[section.material_index];
                if material.blend != blend {
                    continue;
                }
                pass.set_pipeline(&self.shared.sky_pipelines[material.pipeline_index]);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
                pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..section.index_count, 0, 0..1);
            }
        }
    }

    /// Draws sections with authored distortion maps into the signed
    /// screen-space offset target.
    pub fn render_distortion<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        for section in self.visible_sections() {
            let material = &self.gpu_model.materials[section.material_index];
            if !material.has_distortion {
                continue;
            }
            pass.set_pipeline(&self.shared.distortion_pipelines[material.two_sided_pipeline_index]);
            pass.set_bind_group(1, &material.bind_group, &[]);
            pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
            pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..section.index_count, 0, 0..1);
        }
    }

    /// Draws shadow-casting sections into one directional cascade.
    ///
    /// The cascade index follows the oracle's 8x, 4x, 2x, and 1x projection
    /// scale order. Out-of-range indices are ignored.
    pub fn render_shadow<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>, cascade: usize) {
        let Some(pipeline_base) = cascade.checked_mul(2) else {
            return;
        };
        if pipeline_base + 1 >= self.shared.shadow_pipelines.len() {
            return;
        }

        pass.set_bind_group(0, &self.scene_bind_group, &[]);
        for section in self.visible_sections() {
            let material = &self.gpu_model.materials[section.material_index];
            if !material.casts_shadows {
                continue;
            }
            pass.set_pipeline(
                &self.shared.shadow_pipelines[pipeline_base + material.two_sided_pipeline_index],
            );
            pass.set_bind_group(1, &material.bind_group, &[]);
            pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
            pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..section.index_count, 0, 0..1);
        }
    }

    /// Draws shadow-casting sections into one local spot or omni atlas pass.
    pub fn render_local_shadow<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        pass_index: usize,
    ) {
        let Ok(pass_index) = u32::try_from(pass_index) else {
            return;
        };
        let Some(offset) = self.shared.local_shadow_stride.checked_mul(pass_index) else {
            return;
        };
        if self.shared.local_shadow_stride == 0 {
            return;
        }

        render_pass.set_bind_group(0, &self.scene_bind_group, &[]);
        render_pass.set_bind_group(2, &self.shared.shadow_bind_group, &[offset]);
        for section in self.visible_sections() {
            let material = &self.gpu_model.materials[section.material_index];
            if !material.casts_shadows {
                continue;
            }
            render_pass.set_pipeline(
                &self.shared.local_shadow_pipelines[material.two_sided_pipeline_index],
            );
            render_pass.set_bind_group(1, &material.bind_group, &[]);
            render_pass.set_vertex_buffer(0, section.vertex_buffer.slice(..));
            render_pass.set_index_buffer(section.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..section.index_count, 0, 0..1);
        }
    }

    /// Returns the current model-to-world transform.
    #[must_use]
    pub fn model_transform(&self) -> Mat4 {
        self.model_transform
    }

    fn visible_sections(&self) -> impl Iterator<Item = &GpuSection> {
        self.gpu_model
            .sections
            .iter()
            .zip(&self.visible_sections)
            .filter_map(|(section, visible)| visible.then_some(section))
    }
}

fn uses_fade_pipeline(blend: BlendMode, opacity: f32) -> bool {
    opacity < 1.0 && matches!(blend, BlendMode::Opaque | BlendMode::AlphaTest)
}

impl WorldRenderer for Renderer {
    fn render_phase<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        match phase {
            RenderPhase::Sky => self.render_sky(pass),
            RenderPhase::World => self.render(pass),
            RenderPhase::Distortion => self.render_distortion(pass),
            RenderPhase::Shadow { cascade } => self.render_shadow(pass, cascade),
            RenderPhase::LocalShadow { pass: pass_index } => {
                self.render_local_shadow(pass, pass_index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::uses_fade_pipeline;
    use crate::ugx::BlendMode;

    #[test]
    fn only_fading_depth_writers_switch_to_source_over_pipeline() {
        assert!(!uses_fade_pipeline(BlendMode::Opaque, 1.0));
        assert!(uses_fade_pipeline(BlendMode::Opaque, 0.75));
        assert!(uses_fade_pipeline(BlendMode::AlphaTest, 0.75));
        assert!(!uses_fade_pipeline(BlendMode::Over, 0.75));
        assert!(!uses_fade_pipeline(BlendMode::Additive, 0.75));
    }
}
