//! Presentation-only animation effects consumed by the terrain viewer.

use render::lighting::LocalLightView;
use render::wgpu;

use super::TerrainViewer;

impl TerrainViewer {
    pub(super) fn update_attached_lights(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: LocalLightView,
    ) {
        self.local_lights.specular_power = self
            .lightset
            .as_ref()
            .map_or(16.0, |lightset| lightset.terrain_specular_power);
        let intensity_scale = self
            .lightset
            .as_ref()
            .map_or(1.3, |lightset| lightset.lgt_intensity_scale);
        let mut buffered_lights = Vec::new();
        let mut shadow_requests = Vec::new();
        let mut camera_shakes = Vec::new();
        let mut terrain_alpha_updates = Vec::new();
        if let (Some(renderer), Some(simulation)) = (&mut self.ugx_scene_renderer, &self.simulation)
        {
            renderer.update_attached_lights_at_time(
                queue,
                &simulation.world,
                self.render_time_seconds,
                view,
                intensity_scale,
                &mut self.local_lights,
            );
            buffered_lights.extend_from_slice(renderer.buffered_local_lights());
            shadow_requests.extend_from_slice(renderer.local_shadow_requests());
            camera_shakes = renderer.take_animation_camera_shakes();
            terrain_alpha_updates = renderer.take_animation_terrain_alpha();
        } else {
            self.local_lights.clear();
        }
        for shake in camera_shakes {
            // The terrain viewer does not yet expose entity selection. Retail
            // suppresses the two shipped checkSelected tags unless their owner
            // is selected, so conservatively suppress them here too.
            if shake.check_selected()
                || view
                    .project_sphere(shake.position().to_array(), 1.0)
                    .is_none()
            {
                continue;
            }
            self.camera_adapter.begin_animation_shake(
                shake.duration_seconds(),
                shake.strength(),
                self.render_time_seconds,
            );
        }
        if let Some(gpu) = &mut self.gpu {
            for update in terrain_alpha_updates {
                gpu.dynamic_terrain_alpha.apply(queue, update);
            }
            gpu.local_shadows
                .update(queue, &mut self.local_lights, &shadow_requests);
            gpu.light_volume.update(device, queue, &buffered_lights);
        }
    }
}
