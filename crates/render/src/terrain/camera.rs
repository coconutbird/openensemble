//! Camera for terrain rendering.

use glam::{Mat4, Vec3};

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
