//! Per-particle forces, motion, magnets, and terrain collision.

use glam::{EulerRot, Mat4, Quat, Vec3};

use super::emitter::RuntimeParticle;
use super::random::RetailRandom;
use crate::particle::{ParticleMagnetKind, ParticleRuntimeDefinition};

pub(super) fn integrate_particle(
    particle: &mut RuntimeParticle,
    definition: &ParticleRuntimeDefinition,
    duration: f32,
    end_time: f32,
) {
    let random = particle.random_values;
    let alpha = particle.life_alpha(end_time);
    let acceleration = definition.timing.acceleration.sample(random[2]);
    let mut force = particle.velocity * acceleration;
    if definition.force.use_internal_gravity() {
        force.y += definition.force.internal_gravity.sample(random[1]);
    }
    if definition.force.use_internal_wind() {
        let delay = definition
            .force
            .internal_wind_delay
            .sample(random[3])
            .max(0.0);
        if end_time - particle.birth_time >= delay {
            let direction = definition.force.internal_wind_direction.sample(random[1]);
            let speed = definition.force.internal_wind_speed.sample(random[2]);
            let (sin, cos) = direction.to_radians().sin_cos();
            force += Vec3::new(sin, 0.0, cos) * speed;
        }
    }
    particle.velocity += force * duration;
    let speed = definition.speed.sample_with_progression_random(
        alpha,
        [random[0], random[2], random[0], random[3]],
        particle.progression_random,
    );
    particle.position += particle.velocity * Vec3::from_array(speed) * duration;
    particle.rotation += particle.angular_velocity * duration;
}

pub(super) fn apply_magnets(
    particle: &mut RuntimeParticle,
    definition: &ParticleRuntimeDefinition,
    transform: Mat4,
    duration: f32,
    random: &mut RetailRandom,
    random_table: &[f32],
) {
    for magnet in &definition.magnets {
        let rotation = magnet_rotation(magnet.rotation);
        let (position, axis) = if definition.timing.tied_to_emitter() {
            (Vec3::from_array(magnet.offset), rotation * Vec3::Y)
        } else {
            (
                transform.transform_point3(Vec3::from_array(magnet.offset)),
                transform.transform_vector3(rotation * Vec3::Y),
            )
        };
        let direction = match magnet.kind {
            ParticleMagnetKind::Sphere => position - particle.position,
            ParticleMagnetKind::Cylinder => {
                cylinder_direction(particle.position, position, axis, magnet.height)
                    .unwrap_or(Vec3::NAN)
            }
        };
        let distance = direction.length();
        if !distance.is_finite() || distance > magnet.radius || magnet.radius <= f32::EPSILON {
            continue;
        }
        let influence = (distance / magnet.radius).clamp(0.0, 1.0);
        particle.velocity += direction.normalize_or_zero() * magnet.force * influence * duration;
        if magnet.kind == ParticleMagnetKind::Cylinder {
            let tangent = axis
                .normalize_or_zero()
                .cross(direction)
                .normalize_or_zero();
            particle.velocity += tangent * magnet.rotational_force * duration;
        }
        particle.velocity *= (1.0 - magnet.dampening * duration).max(0.0);
        apply_turbulence(particle, magnet.turbulence, duration, random, random_table);
    }
}

pub(super) fn collide_with_terrain(
    particle: &mut RuntimeParticle,
    definition: &ParticleRuntimeDefinition,
    transform: Mat4,
    terrain_height: &mut impl FnMut(Vec3) -> Option<f32>,
    tied: bool,
) {
    let world_position = if tied {
        transform.transform_point3(particle.position)
    } else {
        particle.position
    };
    let Some(height) = terrain_height(world_position) else {
        return;
    };
    let floor = height + definition.timing.collision_offset;
    if world_position.y >= floor {
        return;
    }
    let mut corrected = world_position;
    corrected.y = floor;
    let mut world_velocity = if tied {
        transform.transform_vector3(particle.velocity)
    } else {
        particle.velocity
    };
    world_velocity.y = -world_velocity.y;
    let loss = definition
        .timing
        .collision_energy_loss
        .sample(particle.random_values[0]);
    world_velocity *= (1.0 - loss).max(0.0);
    if tied {
        let inverse = transform.inverse();
        particle.position = inverse.transform_point3(corrected);
        particle.velocity = inverse.transform_vector3(world_velocity);
    } else {
        particle.position = corrected;
        particle.velocity = world_velocity;
    }
}

fn apply_turbulence(
    particle: &mut RuntimeParticle,
    turbulence: f32,
    duration: f32,
    random: &mut RetailRandom,
    random_table: &[f32],
) {
    if turbulence.abs() <= f32::EPSILON {
        return;
    }
    let index = random.index(random_table.len() - 2);
    let direction = Vec3::new(
        random_table[index],
        random_table[index + 1],
        random_table[index + 2],
    );
    particle.velocity += direction * turbulence * duration;
}

fn cylinder_direction(point: Vec3, base: Vec3, axis: Vec3, height: f32) -> Option<Vec3> {
    let line = axis.normalize_or_zero() * height;
    let denominator = line.length_squared();
    if denominator <= f32::EPSILON {
        return None;
    }
    let alpha = (point - base).dot(line) / denominator;
    if !(0.0..=1.0).contains(&alpha) {
        return None;
    }
    Some(base + line * alpha - point)
}

fn magnet_rotation(rotation: [f32; 3]) -> Quat {
    let [pitch, yaw, roll] = rotation.map(f32::to_radians);
    Quat::from_euler(EulerRot::XYZ, pitch, yaw, roll)
}
