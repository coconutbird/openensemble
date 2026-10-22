//! Math utilities and type aliases

pub use glam::{IVec2, IVec3, Mat4, Quat, UVec2, UVec3, Vec2, Vec3, Vec4};
use num_traits::ToPrimitive;

/// A 2D rectangle with position and size
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge position.
    pub x: f32,
    /// Top edge position.
    pub y: f32,
    /// Width along the X axis.
    pub width: f32,
    /// Height along the Y axis.
    pub height: f32,
}

impl Rect {
    /// Creates a rectangle from its position and size.
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns `true` if `point` is inside the rectangle; edges count as inside.
    #[must_use]
    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }
}

/// Color represented as RGBA floats (0.0 - 1.0)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red channel (0.0 - 1.0).
    pub r: f32,
    /// Green channel (0.0 - 1.0).
    pub g: f32,
    /// Blue channel (0.0 - 1.0).
    pub b: f32,
    /// Alpha channel (0.0 - 1.0); 1.0 is opaque.
    pub a: f32,
}

impl Color {
    /// Opaque white.
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// Opaque black.
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque red.
    pub const RED: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque green.
    pub const GREEN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    /// Opaque blue.
    pub const BLUE: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    /// Creates a color from float channels (0.0 - 1.0).
    #[must_use]
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a color from 8-bit channels (0 - 255).
    #[must_use]
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a: f32::from(a) / 255.0,
        }
    }

    /// Converts the color to 8-bit RGBA channels; values out of range clamp.
    #[must_use]
    pub fn to_rgba8(&self) -> [u8; 4] {
        [
            float_to_u8(self.r * 255.0),
            float_to_u8(self.g * 255.0),
            float_to_u8(self.b * 255.0),
            float_to_u8(self.a * 255.0),
        ]
    }
}

fn float_to_u8(value: f32) -> u8 {
    if value.is_nan() {
        return 0;
    }

    value
        .clamp(0.0, f32::from(u8::MAX))
        .trunc()
        .to_u8()
        .expect("a clamped, finite u8 value must convert successfully")
}
