//! RTS camera of the client, with the retail camera values.
//!
//! The retail user camera (`BUser`) looks at a hover point on the terrain from a zoom distance.
//! The yaw turns the view around the vertical axis. The pitch is the angle between the view
//! direction and the straight-down direction. The zoom sets the pitch.
//!
//! The values come from `startup\game.cfg` in `root.era`. The engine reads them in these
//! functions:
//!
//! - `BUser_resetCameraDefaults` (`0x140598C00`): `CameraPitch`, `CameraYaw`, the zoom and pitch
//!   limits, and the start pitch from the start zoom.
//! - `BUser_syncCameraState` (`0x140598D20`): `CameraFOV`.
//! - `BUser_applyCameraYaw` (`0x140599140`): the yaw direction `(sin yaw, 0, cos yaw)` and the eye
//!   at `hover - zoom * forward`.
//! - `BUser_updateCameraControls` (`0x140580220`): `CameraRotateRate`, `CameraZoomRate`, their
//!   fast factors, and the pitch from the zoom.
//! - `BUser_updateScrolling` (`0x14057D9A0`): `ScrollMaxSpeed`, `ScrollFastSpeed`,
//!   `ScrollRateBase` and `ScrollRateGrow`.

use glam::{Vec2, Vec3};
use render::terrain::Camera;
use sim::world::WorldBounds;

/// `CameraYaw`: the start yaw, in degrees.
pub(crate) const YAW_DEGREES: f32 = 317.2;
/// `CameraFOV`: the field of view, in degrees.
pub(crate) const FOV_DEGREES: f32 = 44.9;
/// `CameraPitchMin`: the pitch at the largest zoom, in degrees from straight down.
pub(crate) const PITCH_MIN_DEGREES: f32 = 34.0;
/// `CameraPitchMax`: the pitch at the smallest zoom, in degrees from straight down.
pub(crate) const PITCH_MAX_DEGREES: f32 = 54.0;
/// `CameraZoomMin`: the smallest distance from the hover point to the eye.
pub(crate) const ZOOM_MIN: f32 = 60.0;
/// `CameraZoom`: the start distance from the hover point to the eye.
pub(crate) const ZOOM_DEFAULT: f32 = 85.0;
/// `CameraZoomMax`: the largest distance from the hover point to the eye.
pub(crate) const ZOOM_MAX: f32 = 115.0;
/// `CameraZoomRate`: the zoom speed, in world units for each second.
pub(crate) const ZOOM_RATE: f32 = 80.0;
/// `CameraZoomFastFactor`: the zoom speed multiplier of the fast modifier.
pub(crate) const ZOOM_FAST_FACTOR: f32 = 2.0;
/// `CameraRotateRate`: the yaw speed, in degrees for each second.
pub(crate) const ROTATE_RATE: f32 = 160.0;
/// `CameraRotateFastFactor`: the yaw speed multiplier of the fast modifier.
pub(crate) const ROTATE_FAST_FACTOR: f32 = 1.0;
/// `ScrollMaxSpeed`: the scroll speed, in world units for each second.
pub(crate) const SCROLL_MAX_SPEED: f32 = 100.0;
/// `ScrollFastSpeed`: the scroll speed with the fast modifier, in world units for each second.
pub(crate) const SCROLL_FAST_SPEED: f32 = 400.0;
/// `ScrollRateBase`: the speed fraction when a scroll starts.
pub(crate) const SCROLL_RATE_BASE: f32 = 0.1;
/// `ScrollRateGrow`: the increase of the speed fraction for each second of scroll.
pub(crate) const SCROLL_RATE_GROW: f32 = 1.5;

/// The rate at which the hover height moves to the terrain height, for each second.
///
/// This value is not a retail value. It prevents a jump of the view when the hover point
/// crosses a cliff.
const HOVER_HEIGHT_FOLLOW_RATE: f32 = 5.0;

/// The client-owned RTS camera state.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RtsCamera {
    hover_point: Vec3,
    yaw_degrees: f32,
    pitch_degrees: f32,
    zoom: f32,
    scroll_rate: f32,
}

impl RtsCamera {
    /// Makes a camera with the retail start values that looks at `hover_point`.
    ///
    /// Like `BUser_resetCameraDefaults`, the start pitch goes up from `CameraPitchMin` to
    /// `CameraPitchMax` as the zoom goes up. A zoom change uses the opposite direction (see
    /// [`pitch_for_zoom`]).
    pub(crate) fn new(hover_point: Vec3) -> Self {
        let zoom_fraction = (ZOOM_DEFAULT - ZOOM_MIN) / (ZOOM_MAX - ZOOM_MIN);
        Self {
            hover_point,
            yaw_degrees: YAW_DEGREES,
            pitch_degrees: zoom_fraction * (PITCH_MAX_DEGREES - PITCH_MIN_DEGREES)
                + PITCH_MIN_DEGREES,
            zoom: ZOOM_DEFAULT,
            scroll_rate: SCROLL_RATE_BASE,
        }
    }

    /// Returns the point on the terrain that the camera looks at.
    pub(crate) fn hover_point(&self) -> Vec3 {
        self.hover_point
    }

    /// Returns the distance from the hover point to the eye.
    pub(crate) fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Returns the horizontal view direction.
    fn forward(&self) -> Vec3 {
        let yaw = self.yaw_degrees.to_radians();
        Vec3::new(yaw.sin(), 0.0, yaw.cos())
    }

    /// Returns the horizontal direction to the right of the view.
    fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y)
    }

    /// Returns the view direction from the eye to the hover point.
    fn view_direction(&self) -> Vec3 {
        let pitch = self.pitch_degrees.to_radians();
        self.forward() * pitch.sin() - Vec3::Y * pitch.cos()
    }

    /// Moves the hover point. `direction.x` goes to the right and `direction.y` goes forward.
    ///
    /// Like `BUser_updateScrolling`, the speed fraction starts at `ScrollRateBase` and grows by
    /// `ScrollRateGrow` each second to 1. A zero `direction` stops the scroll and resets the
    /// speed fraction. Keyboard and screen edge input have the full magnitude, so the function
    /// uses only the direction.
    pub(crate) fn scroll(&mut self, direction: Vec2, fast: bool, dt_seconds: f32) {
        let Some(direction) = direction.try_normalize() else {
            self.scroll_rate = SCROLL_RATE_BASE;
            return;
        };
        self.scroll_rate = (self.scroll_rate + SCROLL_RATE_GROW * dt_seconds).min(1.0);
        let speed = if fast {
            SCROLL_FAST_SPEED
        } else {
            SCROLL_MAX_SPEED
        };
        let distance = self.scroll_rate * speed * dt_seconds;
        self.hover_point += (self.right() * direction.x + self.forward() * direction.y) * distance;
    }

    /// Turns the view at `CameraRotateRate`. A positive `amount` turns the view to the right.
    pub(crate) fn rotate(&mut self, amount: f32, fast: bool, dt_seconds: f32) {
        let factor = if fast { ROTATE_FAST_FACTOR } else { 1.0 };
        self.rotate_degrees(amount * ROTATE_RATE * factor * dt_seconds);
    }

    /// Turns the view by `degrees`. A positive value turns the view to the right.
    pub(crate) fn rotate_degrees(&mut self, degrees: f32) {
        // The yaw direction is (sin yaw, 0, cos yaw), so a smaller yaw turns it to the right.
        self.yaw_degrees = (self.yaw_degrees - degrees).rem_euclid(360.0);
    }

    /// Changes the zoom at `CameraZoomRate`. A positive `amount` moves the eye nearer.
    pub(crate) fn zoom_by(&mut self, amount: f32, fast: bool, dt_seconds: f32) {
        let factor = if fast { ZOOM_FAST_FACTOR } else { 1.0 };
        let zoom = (self.zoom - amount * ZOOM_RATE * factor * dt_seconds).clamp(ZOOM_MIN, ZOOM_MAX);
        let change = zoom - self.zoom;
        if change == 0.0 {
            return;
        }
        self.zoom = zoom;
        self.pitch_degrees = pitch_for_zoom(zoom);
    }

    /// Resets the view like the `ResetCamera` path of `BUser_syncCameraState` (`0x140598D20`):
    /// with `zoom`, the pitch and the zoom go back to the start values; with `yaw`, the yaw goes
    /// back to `CameraYaw`. The hover point stays.
    pub(crate) fn reset_view(&mut self, yaw: bool, zoom: bool) {
        let start = Self::new(self.hover_point);
        if zoom {
            self.pitch_degrees = start.pitch_degrees;
            self.zoom = start.zoom;
        }
        if yaw {
            self.yaw_degrees = start.yaw_degrees;
        }
    }

    /// Moves the hover height to the terrain height under the hover point.
    pub(crate) fn follow_terrain(&mut self, terrain_height: Option<f32>, dt_seconds: f32) {
        let Some(height) = terrain_height.filter(|height| height.is_finite()) else {
            return;
        };
        let blend = (HOVER_HEIGHT_FOLLOW_RATE * dt_seconds).min(1.0);
        self.hover_point.y += (height - self.hover_point.y) * blend;
    }

    /// Puts the hover point at the terrain height without a transition.
    pub(crate) fn snap_to_terrain(&mut self, terrain_height: Option<f32>) {
        self.follow_terrain(terrain_height, f32::INFINITY);
    }

    /// Keeps the hover point in the horizontal `bounds`.
    pub(crate) fn clamp_to(&mut self, bounds: WorldBounds) {
        self.hover_point.x = self.hover_point.x.clamp(bounds.min_x(), bounds.max_x());
        self.hover_point.z = self.hover_point.z.clamp(bounds.min_z(), bounds.max_z());
    }

    /// Writes the eye position, the view direction and the field of view to `camera`.
    pub(crate) fn apply_to(&self, camera: &mut Camera) {
        let direction = self.view_direction();
        camera.position = self.hover_point - direction * self.zoom;
        camera.yaw = direction.z.atan2(direction.x);
        camera.pitch = direction.y.clamp(-1.0, 1.0).asin();
        camera.fov = FOV_DEGREES.to_radians();
    }

    /// Takes the yaw and the hover point from `camera` after a simulation camera directive.
    ///
    /// `SimulationCameraAdapter` puts the eye at `adapter_zoom` from the directive point. The
    /// directive does not change the pitch or the zoom of this camera.
    pub(crate) fn take_directive(&mut self, camera: &Camera, adapter_zoom: f32) {
        let forward = camera.forward();
        if let Some(horizontal) = Vec2::new(forward.x, forward.z).try_normalize() {
            self.yaw_degrees = horizontal
                .x
                .atan2(horizontal.y)
                .to_degrees()
                .rem_euclid(360.0);
        }
        self.hover_point = camera.position + forward * adapter_zoom;
    }
}

/// Returns the pitch for `zoom`, like the zoom input of `BUser_updateCameraControls`.
///
/// The pitch goes from `CameraPitchMax` at `CameraZoomMin` to `CameraPitchMin` at
/// `CameraZoomMax`, so the view looks more down when the eye is far away.
pub(crate) fn pitch_for_zoom(zoom: f32) -> f32 {
    let zoom_fraction = (zoom - ZOOM_MIN) / (ZOOM_MAX - ZOOM_MIN);
    ((1.0 - zoom_fraction) * (PITCH_MAX_DEGREES - PITCH_MIN_DEGREES) + PITCH_MIN_DEGREES)
        .clamp(PITCH_MIN_DEGREES, PITCH_MAX_DEGREES)
}

#[cfg(test)]
mod tests;
