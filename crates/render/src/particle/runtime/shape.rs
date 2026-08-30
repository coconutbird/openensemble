//! Initial particle position and trajectory sampling.

use std::f32::consts::{PI, TAU};

use glam::{EulerRot, Quat, Vec3};

use super::random::RetailRandom;
use crate::particle::{ParticleEmitterShape, ParticleEmitterShapeKind};

pub(super) fn emission_position(shape: &ParticleEmitterShape, random: &mut RetailRandom) -> Vec3 {
    let local = match shape.kind {
        ParticleEmitterShapeKind::Point => Vec3::ZERO,
        ParticleEmitterShapeKind::Box => box_position(shape, random),
        ParticleEmitterShapeKind::Cylinder => cylinder_position(shape, random),
        ParticleEmitterShapeKind::Sphere => sphere_position(shape, random, false),
        ParticleEmitterShapeKind::HalfSphere => sphere_position(shape, random, true),
        ParticleEmitterShapeKind::Rectangle => rectangle_position(shape, random),
        ParticleEmitterShapeKind::Circle => circle_position(shape, random),
    };
    shape_rotation(shape) * local + Vec3::from_array(shape.offset)
}

pub(super) fn emission_direction(shape: &ParticleEmitterShape, random: &mut RetailRandom) -> Vec3 {
    let inner = (shape.trajectory_inner_angle * 0.5).to_radians();
    let outer = (shape.trajectory_outer_angle * 0.5).to_radians();
    let polar = random.range_f32(inner, outer.max(inner));
    let azimuth = random.range_f32(0.0, TAU);
    let (sin_polar, cos_polar) = polar.min(PI - f32::EPSILON).sin_cos();
    let (sin_azimuth, cos_azimuth) = azimuth.sin_cos();
    let direction = Vec3::new(sin_polar * cos_azimuth, cos_polar, sin_polar * sin_azimuth);
    (shape_rotation(shape) * direction).normalize_or_zero()
}

fn box_position(shape: &ParticleEmitterShape, random: &mut RetailRandom) -> Vec3 {
    let size = Vec3::from_array(shape.size).abs();
    if !shape.emit_from_surface {
        return Vec3::new(
            random.range_f32(-size.x, size.x),
            random.range_f32(-size.y, size.y),
            random.range_f32(-size.z, size.z),
        );
    }
    let face_areas = [size.x * size.z, size.y * size.z, size.x * size.y];
    let total = face_areas.iter().sum::<f32>();
    if total <= f32::EPSILON {
        return Vec3::ZERO;
    }
    let mut selector = random.range_f32(0.0, total);
    let negative = random.index(2) == 0;
    if selector < face_areas[0] {
        let y = if negative { -size.y } else { size.y };
        return Vec3::new(random_axis(size.x, random), y, random_axis(size.z, random));
    }
    selector -= face_areas[0];
    if selector < face_areas[1] {
        let x = if negative { -size.x } else { size.x };
        return Vec3::new(x, random_axis(size.y, random), random_axis(size.z, random));
    }
    let z = if negative { -size.z } else { size.z };
    Vec3::new(random_axis(size.x, random), random_axis(size.y, random), z)
}

fn cylinder_position(shape: &ParticleEmitterShape, random: &mut RetailRandom) -> Vec3 {
    let radius = shape.size[0].abs();
    let inner = shape.emit_from_surface_radius.abs().min(radius);
    let radial = if shape.emit_from_surface {
        radius
    } else {
        random
            .unit_f32()
            .mul_add(radius * radius - inner * inner, inner * inner)
            .sqrt()
    };
    let angle = random.range_f32(-PI, PI);
    let (sin, cos) = angle.sin_cos();
    Vec3::new(
        radial * cos,
        random.range_f32(0.0, shape.size[1].abs()),
        radial * sin,
    )
}

fn sphere_position(shape: &ParticleEmitterShape, random: &mut RetailRandom, half: bool) -> Vec3 {
    let y = if half {
        random.unit_f32()
    } else {
        random.range_f32(-1.0, 1.0)
    };
    let angle = random.range_f32(-PI, PI);
    let radial = (1.0 - y * y).max(0.0).sqrt();
    let (sin, cos) = angle.sin_cos();
    let radius = if shape.emit_from_surface {
        1.0
    } else {
        random.unit_f32().cbrt()
    };
    Vec3::new(radial * cos, y, radial * sin) * Vec3::from_array(shape.size).abs() * radius
}

fn rectangle_position(shape: &ParticleEmitterShape, random: &mut RetailRandom) -> Vec3 {
    let x_size = shape.size[0].abs();
    let z_size = shape.size[1].abs();
    let total = x_size + z_size;
    if total <= f32::EPSILON {
        return Vec3::ZERO;
    }
    let negative = random.index(2) == 0;
    if random.range_f32(0.0, total) < z_size {
        let x = if negative { -x_size } else { x_size };
        Vec3::new(x, 0.0, random_axis(z_size, random))
    } else {
        let z = if negative { -z_size } else { z_size };
        Vec3::new(random_axis(x_size, random), 0.0, z)
    }
}

fn circle_position(shape: &ParticleEmitterShape, random: &mut RetailRandom) -> Vec3 {
    let outer = shape.size[0].abs();
    let inner = shape.emit_from_surface_radius.abs().min(outer);
    let radius = if shape.emit_from_surface {
        outer
    } else {
        random
            .unit_f32()
            .mul_add(outer * outer - inner * inner, inner * inner)
            .sqrt()
    };
    let angle = random.range_f32(-PI, PI);
    let (sin, cos) = angle.sin_cos();
    Vec3::new(radius * cos, 0.0, radius * sin)
}

fn shape_rotation(shape: &ParticleEmitterShape) -> Quat {
    let [pitch, yaw, bank] = shape.trajectory_rotation.map(f32::to_radians);
    Quat::from_euler(EulerRot::XYZ, pitch, yaw, bank)
}

fn random_axis(half_extent: f32, random: &mut RetailRandom) -> f32 {
    random.range_f32(-half_extent, half_extent)
}
