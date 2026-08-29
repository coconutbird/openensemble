//! Camera for terrain rendering.

use glam::{Mat4, Vec3};

use sim::{CameraDirective, PlayerId, PlayerPresentationState, World};

const DEFAULT_CAMERA_ZOOM: f32 = 300.0;

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
        Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y)
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
    hover_point: Option<Vec3>,
    hover_height_offset: f32,
    zoom_distance: f32,
}

impl Default for SimulationCameraAdapter {
    fn default() -> Self {
        Self {
            applied_revision: 0,
            hover_point: None,
            hover_height_offset: 0.0,
            zoom_distance: DEFAULT_CAMERA_ZOOM,
        }
    }
}

impl SimulationCameraAdapter {
    /// Seed renderer-local hover/zoom state after loading a terrain scene.
    pub fn reset(&mut self, camera: &Camera, hover_point: Vec3) {
        self.applied_revision = 0;
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
        let state = world.player_presentation_state(player_id);
        if let Some(directive) = state
            .camera_directive
            .filter(|directive| directive.revision != self.applied_revision)
        {
            self.apply_directive(camera, directive);
        }
        state
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

    #[test]
    fn simulation_directive_is_one_shot_and_preserves_renderer_zoom() {
        let mut camera = Camera::default();
        let mut adapter = SimulationCameraAdapter::default();
        adapter.reset(&camera, Vec3::ZERO);
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
}
