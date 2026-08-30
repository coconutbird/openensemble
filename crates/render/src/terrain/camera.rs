//! Camera for terrain rendering.

use glam::{Mat4, Vec2, Vec3};
use num_traits::ToPrimitive;
use std::time::Duration;

use sim::{CameraDirective, CameraShake, PlayerId, PlayerPresentationState, World};

const DEFAULT_CAMERA_ZOOM: f32 = 300.0;
const DEFAULT_SHAKE_TRAIL_OFF_SECONDS: f32 = 0.4;
const DEFAULT_SHAKE_CONSERVATION_FACTOR: f32 = 0.5;
const LOCAL_SHAKE_REVISION_BIT: u64 = 1_u64 << 63;

/// Fly camera for navigating terrain.
///
/// Uses yaw/pitch rotation with WASD movement.
/// Yaw is horizontal rotation, pitch is vertical.
#[derive(Clone)]
pub struct Camera {
    /// Camera position in world space.
    pub position: Vec3,
    /// Horizontal rotation in radians.
    pub yaw: f32,
    /// Vertical rotation in radians.
    pub pitch: f32,
    /// Field of view in radians.
    pub fov: f32,
    /// Near clipping plane.
    pub near: f32,
    /// Far clipping plane.
    pub far: f32,
    /// Movement speed in units per second.
    pub speed: f32,
    /// Mouse sensitivity.
    pub sensitivity: f32,
    shake_offset: Vec3,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(500.0, 200.0, 500.0),
            yaw: -std::f32::consts::FRAC_PI_4,
            pitch: -0.3,
            fov: 60.0_f32.to_radians(),
            near: 1.0,
            far: 10000.0,
            speed: 100.0,
            sensitivity: 0.002,
            shake_offset: Vec3::ZERO,
        }
    }
}

impl Camera {
    /// Create a new camera at the given position.
    #[must_use]
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }

    /// Get the forward direction vector.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    /// Get the right direction vector.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// Get the up direction vector.
    #[must_use]
    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize()
    }

    /// Get the view matrix.
    #[must_use]
    pub fn view_matrix(&self) -> Mat4 {
        let eye = self.position + self.shake_offset;
        Mat4::look_at_rh(eye, eye + self.forward(), Vec3::Y)
    }

    /// Get the projection matrix for the given aspect ratio.
    #[must_use]
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov, aspect, self.near, self.far)
    }

    /// Get the combined view-projection matrix.
    #[must_use]
    pub fn view_projection_matrix(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// Move the camera forward/backward.
    pub fn move_forward(&mut self, amount: f32) {
        self.position += self.forward() * amount;
    }

    /// Move the camera right/left.
    pub fn move_right(&mut self, amount: f32) {
        self.position += self.right() * amount;
    }

    /// Move the camera up/down.
    pub fn move_up(&mut self, amount: f32) {
        self.position += Vec3::Y * amount;
    }

    /// Rotate the camera by mouse delta.
    pub fn rotate(&mut self, delta_x: f32, delta_y: f32) {
        self.yaw += delta_x * self.sensitivity;
        self.pitch -= delta_y * self.sensitivity;
        // Clamp pitch to avoid gimbal lock
        self.pitch = self.pitch.clamp(-1.5, 1.5);
    }

    /// Position the camera to view the given terrain bounds.
    pub fn look_at_terrain(&mut self, center: Vec3, size: Vec3) {
        let max_dim = size.x.max(size.z);
        let height = size.y.max(50.0);
        self.position = center + Vec3::new(max_dim * 0.5, height * 2.0, max_dim * 0.5);
        self.yaw = -std::f32::consts::FRAC_PI_4;
        self.pitch = -0.4;
    }
}

/// Renderer-owned adapter for one-shot camera directives from the simulation.
///
/// Persistent permissions remain in [`PlayerPresentationState`]. Hover point,
/// zoom distance, and the applied revision are local UI state and never feed
/// gameplay back into the simulation.
#[derive(Clone, Debug)]
pub struct SimulationCameraAdapter {
    applied_revision: u32,
    applied_shake_revision: u64,
    next_local_shake_revision: u32,
    local_shake: Option<LocalCameraShake>,
    sampled_shake_time_ms: Option<u32>,
    accumulated_shake: Vec2,
    hover_point: Option<Vec3>,
    hover_height_offset: f32,
    zoom_distance: f32,
}

impl Default for SimulationCameraAdapter {
    fn default() -> Self {
        Self {
            applied_revision: 0,
            applied_shake_revision: 0,
            next_local_shake_revision: 0,
            local_shake: None,
            sampled_shake_time_ms: None,
            accumulated_shake: Vec2::ZERO,
            hover_point: None,
            hover_height_offset: 0.0,
            zoom_distance: DEFAULT_CAMERA_ZOOM,
        }
    }
}

impl SimulationCameraAdapter {
    /// Seed renderer-local hover/zoom state after loading a terrain scene.
    pub fn reset(&mut self, camera: &mut Camera, hover_point: Vec3) {
        self.applied_revision = 0;
        self.local_shake = None;
        self.clear_shake(camera);
        self.hover_point = hover_point.is_finite().then_some(hover_point);
        self.hover_height_offset = 0.0;
        let zoom_distance = camera.position.distance(hover_point);
        self.zoom_distance = if zoom_distance.is_finite() {
            zoom_distance.max(1.0)
        } else {
            DEFAULT_CAMERA_ZOOM
        };
    }

    /// Project the latest directive for `player_id` into the renderer camera.
    ///
    /// The returned permissions are read directly by the input adapter. A
    /// directive is applied once per synchronized revision.
    pub fn synchronize(
        &mut self,
        camera: &mut Camera,
        world: &World,
        player_id: PlayerId,
    ) -> PlayerPresentationState {
        let render_time_seconds = Duration::from_millis(u64::from(world.game_time())).as_secs_f32();
        self.synchronize_at_time(camera, world, player_id, render_time_seconds)
    }

    /// Project sim directives plus renderer-owned animation shakes at render time.
    pub fn synchronize_at_time(
        &mut self,
        camera: &mut Camera,
        world: &World,
        player_id: PlayerId,
        render_time_seconds: f32,
    ) -> PlayerPresentationState {
        let state = world.player_presentation_state(player_id);
        if let Some(directive) = state
            .camera_directive
            .filter(|directive| directive.revision != self.applied_revision)
        {
            self.apply_directive(camera, directive);
        }
        self.apply_shake(
            camera,
            world.camera_shake(player_id),
            world.game_time(),
            render_time_seconds,
        );
        state
    }

    /// Begin a renderer-local visual-animation shake with retail defaults.
    pub fn begin_animation_shake(
        &mut self,
        duration_seconds: f32,
        strength: f32,
        render_time_seconds: f32,
    ) {
        if !duration_seconds.is_finite()
            || !strength.is_finite()
            || !render_time_seconds.is_finite()
        {
            return;
        }
        self.next_local_shake_revision = self.next_local_shake_revision.wrapping_add(1).max(1);
        self.local_shake = Some(LocalCameraShake {
            revision: self.next_local_shake_revision,
            started_at_seconds: render_time_seconds,
            duration_seconds: duration_seconds.max(0.0),
            strength: strength.max(0.0),
        });
    }

    fn apply_directive(&mut self, camera: &mut Camera, directive: CameraDirective) {
        if directive.revision == self.applied_revision {
            return;
        }
        if let Some(direction) = directive.direction {
            apply_retail_v4_yaw(camera, direction);
        }
        if let Some(offset) = directive.hover_height_offset {
            self.hover_height_offset = offset;
        }
        if let Some(hover_point) = directive.location {
            let target = hover_point + Vec3::Y * self.hover_height_offset;
            camera.position = target - camera.forward() * self.zoom_distance;
            self.hover_point = Some(hover_point);
        } else if directive.hover_height_offset.is_some()
            && let Some(hover_point) = self.hover_point
        {
            let target = hover_point + Vec3::Y * self.hover_height_offset;
            camera.position = target - camera.forward() * self.zoom_distance;
        }
        self.applied_revision = directive.revision;
    }

    fn apply_shake(
        &mut self,
        camera: &mut Camera,
        simulation_shake: Option<CameraShake>,
        game_time_ms: u32,
        render_time_seconds: f32,
    ) {
        let local = self
            .local_shake
            .and_then(|shake| shake.sample(render_time_seconds));
        if self.local_shake.is_some() && local.is_none() {
            self.local_shake = None;
        }
        let shake = local.or_else(|| {
            simulation_shake.map(|shake| CameraShakeSample {
                identity: u64::from(shake.revision()),
                strength: shake.strength(),
                conservation_factor: shake.conservation_factor(),
                sample_time_ms: game_time_ms,
            })
        });
        let Some(shake) = shake else {
            self.clear_shake(camera);
            return;
        };
        if shake.identity != self.applied_shake_revision {
            self.applied_shake_revision = shake.identity;
            self.sampled_shake_time_ms = None;
            self.accumulated_shake = Vec2::ZERO;
        }
        if self.sampled_shake_time_ms == Some(shake.sample_time_ms) {
            return;
        }
        let revision = u32::try_from(shake.identity).unwrap_or_else(|_| {
            u32::try_from(shake.identity & u64::from(u32::MAX)).unwrap_or(u32::MAX)
        });
        let random = shake_sample(revision, shake.sample_time_ms) * shake.strength;
        let correction = self.accumulated_shake * shake.conservation_factor;
        self.accumulated_shake += random - correction;
        let world_forward =
            Vec3::new(camera.forward().x, 0.0, camera.forward().z).normalize_or_zero();
        camera.shake_offset =
            camera.right() * self.accumulated_shake.x + world_forward * self.accumulated_shake.y;
        self.sampled_shake_time_ms = Some(shake.sample_time_ms);
    }

    fn clear_shake(&mut self, camera: &mut Camera) {
        self.applied_shake_revision = 0;
        self.sampled_shake_time_ms = None;
        self.accumulated_shake = Vec2::ZERO;
        camera.shake_offset = Vec3::ZERO;
    }
}

#[derive(Clone, Copy, Debug)]
struct LocalCameraShake {
    revision: u32,
    started_at_seconds: f32,
    duration_seconds: f32,
    strength: f32,
}

impl LocalCameraShake {
    fn sample(self, render_time_seconds: f32) -> Option<CameraShakeSample> {
        if !render_time_seconds.is_finite() {
            return None;
        }
        let elapsed = (render_time_seconds - self.started_at_seconds).max(0.0);
        let total = self.duration_seconds + DEFAULT_SHAKE_TRAIL_OFF_SECONDS;
        if elapsed > total {
            return None;
        }
        let strength = if elapsed <= self.duration_seconds {
            self.strength
        } else {
            let remaining = (total - elapsed).max(0.0);
            let endpoint_epsilon = f32::EPSILON * render_time_seconds.abs().max(1.0);
            if remaining <= endpoint_epsilon {
                0.0
            } else {
                let multiplier = remaining / DEFAULT_SHAKE_TRAIL_OFF_SECONDS;
                self.strength * multiplier * multiplier
            }
        };
        Some(CameraShakeSample {
            identity: LOCAL_SHAKE_REVISION_BIT | u64::from(self.revision),
            strength,
            conservation_factor: DEFAULT_SHAKE_CONSERVATION_FACTOR,
            sample_time_ms: render_time_milliseconds(render_time_seconds),
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct CameraShakeSample {
    identity: u64,
    strength: f32,
    conservation_factor: f32,
    sample_time_ms: u32,
}

fn render_time_milliseconds(render_time_seconds: f32) -> u32 {
    (render_time_seconds.max(0.0) * 1_000.0)
        .to_u32()
        .unwrap_or(u32::MAX)
}

fn shake_sample(revision: u32, game_time_ms: u32) -> Vec2 {
    Vec2::new(
        hash_to_signed_float(revision ^ game_time_ms.rotate_left(11)),
        hash_to_signed_float(revision.rotate_left(17) ^ game_time_ms),
    )
}

fn hash_to_signed_float(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    let sample = u16::try_from(value >> 16).unwrap_or(u16::MAX);
    f32::from(sample) / f32::from(u16::MAX) * 2.0 - 1.0
}

fn apply_retail_v4_yaw(camera: &mut Camera, direction: Vec3) {
    let Some(current) = Vec3::new(camera.forward().x, 0.0, camera.forward().z).try_normalize()
    else {
        return;
    };
    let Some(target) = Vec3::new(direction.x, 0.0, direction.z).try_normalize() else {
        return;
    };
    let angle = current.dot(target).clamp(-1.0, 1.0).acos();
    camera.yaw -= angle;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON);
    }

    #[test]
    fn simulation_directive_is_one_shot_and_preserves_renderer_zoom() {
        let mut camera = Camera::default();
        let mut adapter = SimulationCameraAdapter::default();
        adapter.reset(&mut camera, Vec3::ZERO);
        let zoom = camera.position.length();
        let directive = CameraDirective {
            revision: 1,
            location: Some(Vec3::new(20.0, 5.0, 30.0)),
            direction: Some(Vec3::X),
            hover_height_offset: Some(3.0),
        };

        adapter.apply_directive(&mut camera, directive);
        let hover = Vec3::new(20.0, 8.0, 30.0);
        assert!((camera.position.distance(hover) - zoom).abs() < 0.001);
        let first_position = camera.position;
        adapter.apply_directive(&mut camera, directive);
        assert_eq!(camera.position, first_position);
    }

    #[test]
    fn shake_sampling_is_bounded_and_changes_over_time() {
        let first = shake_sample(1, 100);
        let second = shake_sample(1, 101);
        assert_ne!(first, second);
        for component in [first.x, first.y, second.x, second.y] {
            assert!((-1.0..=1.0).contains(&component));
        }
    }

    #[test]
    fn local_shake_holds_then_uses_retail_quadratic_trailoff() {
        let shake = LocalCameraShake {
            revision: 7,
            started_at_seconds: 10.0,
            duration_seconds: 0.5,
            strength: 4.0,
        };

        let held = shake.sample(10.25).expect("held shake");
        assert_near(held.strength, 4.0);
        assert_near(held.conservation_factor, 0.5);
        let trailing = shake.sample(10.7).expect("trailing shake");
        assert!((trailing.strength - 1.0).abs() < 0.0001);
        assert_near(shake.sample(10.9).expect("trail endpoint").strength, 0.0);
        assert!(shake.sample(10.901).is_none());
    }

    #[test]
    fn render_time_conversion_saturates_without_panicking() {
        assert_eq!(render_time_milliseconds(-1.0), 0);
        assert_eq!(render_time_milliseconds(f32::MAX), u32::MAX);
    }
}
