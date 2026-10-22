//! Per-frame GPU state updates of the `WorldView`.

use num_traits::ToPrimitive;
use zerocopy::IntoBytes;

use super::{WorldFrame, WorldView, WorldViewSettings};
use crate::lighting::LocalLightView;
use crate::particle::ParticleScene;
use crate::postprocess::ToneMapSettings;
use crate::terrain::{LightingParams, TerrainParams};
use crate::ugx::AnimationCameraShake;
use crate::wgpu;

impl WorldView {
    /// Updates the GPU state of one frame. Call `encode` after this call.
    ///
    /// Returns the camera shakes that the unit animations started in this frame.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &WorldFrame<'_>,
    ) -> Vec<AnimationCameraShake> {
        let camera = &frame.camera;
        let settings = &frame.settings;
        self.update_compositor_lod(camera.position.to_array(), settings);
        let camera_shakes = self.update_attached_lights(
            device,
            queue,
            camera.light_view(),
            frame.world,
            frame.time_seconds,
        );
        self.update_terrain_uniforms(queue, camera.view_projection, settings);
        let lighting = self.update_lighting(queue, frame, settings.bump_power);
        let (light_volume_rows, particle_light_scale) = self.particle_light_volume();
        let particle_scene = ParticleScene::new(
            camera.view_projection,
            camera.world_to_view,
            camera.position.to_array(),
            camera.viewport,
            camera.depth_unproject,
        )
        .with_light_volume(light_volume_rows, particle_light_scale);
        self.update_ugx(device, queue, frame, &lighting, &particle_scene);
        if let (Some([start, end]), Some(foliage)) = (settings.foliage_fade, &mut self.foliage) {
            foliage.set_fade_distances(queue, start, end);
        }
        self.compositor.composite_all_dirty(
            &self.compositor_bind_group,
            queue,
            &self.chunk_layer_counts,
            &self.chunk_decal_layer_counts,
            device,
            self.compositor_debug_mode,
        );
        let tone_settings = if settings.scenario_tone_map {
            self.lightset
                .as_ref()
                .map_or_else(ToneMapSettings::default, ToneMapSettings::from)
        } else {
            ToneMapSettings::default()
        };
        self.tone_map.update(queue, tone_settings);
        self.clear_color = scene_clear_color(&lighting);
        self.details = settings.details;
        camera_shakes
    }

    /// Marks all chunks dirty when the compositor mode changes, then updates the chunk LOD.
    fn update_compositor_lod(
        &mut self,
        camera_position: [f32; 3],
        settings: &WorldViewSettings<'_>,
    ) {
        if settings.compositor_debug_mode != self.compositor_debug_mode {
            self.compositor_debug_mode = settings.compositor_debug_mode;
            self.compositor.mark_all_dirty();
        }
        let Some(lod_config) = settings.lod else {
            return;
        };
        if self.chunk_centers.is_empty() {
            return;
        }
        if self
            .compositor
            .update_lod(camera_position, &self.chunk_centers, lod_config)
        {
            log::debug!(
                "LOD updated: {} dirty chunks",
                self.compositor.dirty_chunk_count()
            );
        }
    }

    fn update_attached_lights(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: LocalLightView,
        world: Option<&sim::World>,
        time_seconds: f32,
    ) -> Vec<AnimationCameraShake> {
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
        if let (Some(renderer), Some(world)) = (&mut self.units, world) {
            renderer.update_attached_lights_at_time(
                queue,
                world,
                time_seconds,
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
        for update in terrain_alpha_updates {
            self.terrain.dynamic_terrain_alpha.apply(queue, update);
        }
        self.terrain
            .local_shadows
            .update(queue, &mut self.local_lights, &shadow_requests);
        self.terrain
            .light_volume
            .update(device, queue, &buffered_lights);
        camera_shakes
    }

    fn update_terrain_uniforms(
        &self,
        queue: &wgpu::Queue,
        view_projection: glam::Mat4,
        settings: &WorldViewSettings<'_>,
    ) {
        let terrain = &self.terrain;
        queue.write_buffer(
            &terrain.camera_buffer,
            0,
            view_projection.to_cols_array().as_bytes(),
        );

        let params = TerrainParams {
            terrain_size: terrain.terrain_size,
            chunk_count: terrain.chunk_grid.dimensions_f32(),
            texture_tile_scale: terrain.tile_scale,
            debug_mode: settings
                .debug_mode
                .to_f32()
                .expect("debug mode must fit f32"),
            bump_power: settings.bump_power,
            padding: 0.0,
        };
        queue.write_buffer(&terrain.params_buffer, 0, params.as_bytes());
    }

    fn update_lighting(
        &mut self,
        queue: &wgpu::Queue,
        frame: &WorldFrame<'_>,
        bump_power: f32,
    ) -> LightingParams {
        let camera_position = frame.camera.position;
        let terrain = &self.terrain;
        let camera = [camera_position.x, camera_position.y, camera_position.z];
        let mut params = self.lightset.as_ref().map_or_else(
            || LightingParams {
                world_camera_pos: [camera[0], camera[1], camera[2], 0.0],
                ..Default::default()
            },
            |lightset| LightingParams::from_lightset(lightset, camera),
        );
        if let Some(world) = frame.world {
            params.apply_simulation_state(world);
        }
        let light_direction = glam::Vec3::from_array([
            params.dir_light_vec[0],
            params.dir_light_vec[1],
            params.dir_light_vec[2],
        ])
        .normalize_or_zero();
        let shadows_requested = self
            .lightset
            .as_ref()
            .is_none_or(|lightset| lightset.sun_shadows);
        let mut shadow_columns = [[1.0_f32, 0.0, 0.0, 0.0]; 4];
        let mut shadow_enabled = 0.0;

        if shadows_requested && light_direction != glam::Vec3::ZERO {
            let terrain_center = glam::Vec3::new(
                terrain.terrain_size[0] * 0.5,
                50.0,
                terrain.terrain_size[1] * 0.5,
            );
            let terrain_size =
                glam::Vec3::new(terrain.terrain_size[0], 100.0, terrain.terrain_size[1]);
            shadow_columns = self
                .shadow
                .compute_light_vp(light_direction, terrain_center, terrain_size)
                .to_cols_array_2d();
            self.shadow.update_light_vp(queue);
            shadow_enabled = 1.0;
        }

        params.shadow_vp_col0 = shadow_columns[0];
        params.shadow_vp_col1 = shadow_columns[1];
        params.shadow_vp_col2 = shadow_columns[2];
        params.shadow_vp_col3 = shadow_columns[3];
        params.shadow_params[2] = shadow_enabled;
        self.local_lights.apply_to_lighting(&mut params);
        terrain.light_volume.apply_to_lighting(&mut params);
        terrain.local_lights.update(queue, &self.local_lights);
        if let Some(lighting_buffer) = &terrain.lighting_buffer {
            queue.write_buffer(lighting_buffer, 0, params.as_bytes());
        }
        if let Some(foliage) = &mut self.foliage {
            foliage.update_frame(queue, &params, frame.time_seconds);
        }
        if let Some(roads) = &mut self.roads {
            roads.update_frame(queue, &params, bump_power);
        }
        params
    }

    fn particle_light_volume(&self) -> ([[f32; 4]; 3], f32) {
        let rows = self.terrain.light_volume.world_to_volume_rows();
        let intensity = self
            .lightset
            .as_ref()
            .map_or(1.4, |lightset| lightset.lgt_particle_intensity_scale);
        (rows, intensity)
    }

    fn update_ugx(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &WorldFrame<'_>,
        lighting: &LightingParams,
        particle_scene: &ParticleScene,
    ) {
        if let Some(renderer) = &mut self.sky {
            let camera = glam::Vec3::from_array([
                lighting.world_camera_pos[0],
                lighting.world_camera_pos[1],
                lighting.world_camera_pos[2],
            ]);
            renderer.update_frame_at_time(
                queue,
                frame.camera.view_projection,
                glam::Mat4::from_translation(camera),
                lighting,
                frame.time_seconds,
            );
        }
        if let (Some(renderer), Some(world)) = (&mut self.units, frame.world) {
            renderer.update_from_world_with_particles_at_time(
                device,
                queue,
                world,
                lighting,
                frame.time_seconds,
                particle_scene,
            );
        }
    }
}

fn scene_clear_color(lighting: &LightingParams) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(lighting.fog_color[0]),
        g: f64::from(lighting.fog_color[1]),
        b: f64::from(lighting.fog_color[2]),
        a: 1.0,
    }
}
