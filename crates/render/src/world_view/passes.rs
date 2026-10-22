//! Render passes of the `WorldView`.

use super::WorldView;
use crate::{RenderPhase, WorldRenderer, wgpu};

impl WorldView {
    /// Records the shadow passes, the scene passes and the tone mapping with a given depth.
    pub(super) fn encode_with_depth(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        self.render_shadow_pass(encoder);
        self.render_local_shadow_passes(encoder);
        self.render_terrain_pass(encoder, depth_view);
        self.render_particle_pass(encoder, depth_view);
        self.render_distortion_pass(encoder, depth_view);
        self.tone_map.encode(encoder, output);
        self.render_hp_bar_pass(encoder, output);
    }

    fn render_units<'pass>(&'pass self, phase: RenderPhase, pass: &mut wgpu::RenderPass<'pass>) {
        if let Some(renderer) = &self.units {
            renderer.render_phase(phase, pass);
        }
    }

    fn render_shadow_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        if self
            .lightset
            .as_ref()
            .is_some_and(|lightset| !lightset.sun_shadows)
        {
            return;
        }
        let shadow = &self.shadow;
        for cascade in 0..shadow.cascade_count() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("World Directional Shadow Cascade"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: shadow.cascade_shadow_view(cascade),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: shadow.cascade_depth_view(cascade),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            let phase = RenderPhase::Shadow { cascade };
            shadow.render_phase(phase, &mut pass);
            if let Some(foliage) = &self.foliage {
                foliage.render_phase(phase, &mut pass);
            }
            self.render_units(phase, &mut pass);
        }
    }

    fn render_local_shadow_passes(&self, encoder: &mut wgpu::CommandEncoder) {
        let local_shadows = &self.terrain.local_shadows;
        if local_shadows.passes().is_empty() {
            return;
        }

        local_shadows.encode_clears(encoder);
        for (pass_index, descriptor) in local_shadows.passes().iter().copied().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("World Local Shadow Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: local_shadows.layer_view(descriptor.layer()),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            let [x, y, width, height] = descriptor.viewport();
            pass.set_viewport(
                f32::from(x),
                f32::from(y),
                f32::from(width),
                f32::from(height),
                0.0,
                1.0,
            );
            pass.set_scissor_rect(
                u32::from(x),
                u32::from(y),
                u32::from(width),
                u32::from(height),
            );
            self.shadow
                .render_local_shadow(local_shadows, pass_index, &mut pass);
            self.render_units(RenderPhase::LocalShadow { pass: pass_index }, &mut pass);
        }
    }

    /// Draws the sky, the terrain, the units, the foliage, the roads and the selection decals.
    fn render_terrain_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Terrain Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.tone_map.scene_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        if self.details
            && let Some(renderer) = &self.sky
        {
            renderer.render_phase(RenderPhase::Sky, &mut render_pass);
        }
        self.terrain
            .render_phase(RenderPhase::World, &mut render_pass);

        if self.details {
            self.render_units(RenderPhase::World, &mut render_pass);
            if let Some(foliage) = &self.foliage {
                foliage.render_phase(RenderPhase::World, &mut render_pass);
            }
            if let Some(roads) = &self.roads {
                roads.render_phase(RenderPhase::World, &mut render_pass);
            }
        }
        // The selection decals are client UI, so they draw without the scene details too.
        self.selection_decals.render(&mut render_pass);
    }

    /// Draws the hit-point bars over the tone-mapped output, without a depth test.
    fn render_hp_bar_pass(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let Some(hp_bars) = self.hp_bars.as_ref().filter(|hp_bars| hp_bars.has_bars()) else {
            return;
        };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("HP Bar Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        hp_bars.render(&mut render_pass);
    }

    /// Draws the particles over the lit world. The depth is read-only, so the particles can
    /// sample it for soft fading.
    fn render_particle_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
    ) {
        if !self.details {
            return;
        }
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Particle Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.tone_map.scene_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        self.render_units(RenderPhase::Particles, &mut render_pass);
    }

    /// Draws the signed screen-space distortion vectors. The depth is read-only.
    fn render_distortion_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("UGX Distortion Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.tone_map.distortion_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        if self.details {
            self.render_units(RenderPhase::Distortion, &mut render_pass);
        }
    }
}
