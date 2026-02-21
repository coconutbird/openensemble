//! Camera input handling for the terrain viewer.

use render::terrain::Camera;
use xcore::app::{GamepadButton, Input, KeyCode};

/// Extension trait for Camera to add input handling.
pub trait CameraInput {
    fn update(&mut self, input: &Input, dt: f32);
}

impl CameraInput for Camera {
    fn update(&mut self, input: &Input, dt: f32) {
        // Speed modifier: LShift on keyboard, or left trigger on gamepad
        let speed = if input.is_key_held(KeyCode::LShift) || input.gamepad.left_trigger > 0.5 {
            self.speed * 3.0
        } else {
            self.speed
        };

        // Keyboard movement
        if input.is_key_held(KeyCode::W) {
            self.move_forward(speed * dt);
        }
        if input.is_key_held(KeyCode::S) {
            self.move_forward(-speed * dt);
        }
        if input.is_key_held(KeyCode::A) {
            self.move_right(-speed * dt);
        }
        if input.is_key_held(KeyCode::D) {
            self.move_right(speed * dt);
        }
        if input.is_key_held(KeyCode::Space) {
            self.move_up(speed * dt);
        }
        if input.is_key_held(KeyCode::LCtrl) {
            self.move_up(-speed * dt);
        }

        // Keyboard look (arrow keys)
        if input.is_key_held(KeyCode::Left) {
            self.yaw -= 1.5 * dt;
        }
        if input.is_key_held(KeyCode::Right) {
            self.yaw += 1.5 * dt;
        }
        if input.is_key_held(KeyCode::Up) {
            self.pitch = (self.pitch + 1.0 * dt).clamp(-1.5, 1.5);
        }
        if input.is_key_held(KeyCode::Down) {
            self.pitch = (self.pitch - 1.0 * dt).clamp(-1.5, 1.5);
        }

        // Gamepad controls (Xbox controller)
        if input.gamepad.connected {
            // Left stick: movement (forward/back, strafe left/right)
            let move_x = input.gamepad.left_stick_x;
            let move_y = input.gamepad.left_stick_y; // Up on stick = forward

            if move_y.abs() > 0.0 {
                self.move_forward(move_y * speed * dt);
            }
            if move_x.abs() > 0.0 {
                self.move_right(move_x * speed * dt);
            }

            // Right stick: look (yaw/pitch)
            let look_x = input.gamepad.right_stick_x;
            let look_y = input.gamepad.right_stick_y;
            let look_sensitivity = 2.0;

            if look_x.abs() > 0.0 {
                self.yaw += look_x * look_sensitivity * dt;
            }
            if look_y.abs() > 0.0 {
                // Up on stick = look up (increase pitch)
                self.pitch = (self.pitch + look_y * look_sensitivity * dt).clamp(-1.5, 1.5);
            }

            // Bumpers: up/down movement
            if input.gamepad.is_button_held(GamepadButton::RightBumper) {
                self.move_up(speed * dt);
            }
            if input.gamepad.is_button_held(GamepadButton::LeftBumper) {
                self.move_up(-speed * dt);
            }
        }
    }
}
