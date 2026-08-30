//! Runtime state and scheduling for one authored emitter.

use std::{mem, sync::LazyLock};

use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;

use super::appearance::{SegmentDescriptor, particle_instance, segment_instance};
use super::motion::{apply_magnets, collide_with_terrain, integrate_particle};
use super::random::RetailRandom;
use super::shape::{emission_direction, emission_position};
use super::{ParticleEmitterState, ParticleNestedEvent, ParticleRenderContext};
use crate::particle::{
    ParticleEmitter, ParticleEmitterKind, ParticleGeometry, ParticleInstance, ParticleMaterial,
    ParticleTextureDefinition, ParticleTrailEmission, ParticleTrailUv,
};

const RANDOM_TABLE_SIZE: usize = 4096;
const MAX_UPDATE_STEP: f32 = 0.1;
const MAX_BEAM_TESSELLATION: u32 = 100;

static RANDOM_FLOATS: LazyLock<[f32; RANDOM_TABLE_SIZE]> = LazyLock::new(|| {
    let mut random = RetailRandom::from_reference_seed();
    std::array::from_fn(|_| random.range_f32(-1.0, 1.0))
});

#[derive(Clone, Debug)]
pub(super) struct RuntimeParticle {
    pub(super) id: u64,
    pub(super) birth_time: f32,
    pub(super) death_time: f32,
    pub(super) age_seconds: f32,
    pub(super) position: Vec3,
    pub(super) velocity: Vec3,
    pub(super) rotation: f32,
    pub(super) angular_velocity: f32,
    pub(super) up_axis: Vec3,
    pub(super) texture_layers: [u32; 4],
    pub(super) random_values: [f32; 4],
    pub(super) progression_random: f32,
}

impl RuntimeParticle {
    pub(super) fn life_alpha(&self, current_time: f32) -> f32 {
        let lifetime = self.death_time - self.birth_time;
        if lifetime <= f32::EPSILON {
            return 1.0;
        }
        ((current_time - self.birth_time) / lifetime).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy)]
struct ParticleSpawn {
    position: Vec3,
    velocity: Vec3,
    rotation: f32,
    angular_velocity: f32,
    up_axis: Vec3,
    texture_layers: [u32; 4],
    random_values: [f32; 4],
}

/// Deterministic state for one decoded PFX emitter.
#[derive(Clone, Debug)]
pub struct ParticleEmitterRuntime {
    emitter: ParticleEmitter,
    state: ParticleEmitterState,
    time: f32,
    activity_remaining: f32,
    emission_interval: f32,
    next_emission_wait: f32,
    transform: Mat4,
    secondary_transform: Mat4,
    particles: Vec<RuntimeParticle>,
    random: RetailRandom,
    nested_events: Vec<ParticleNestedEvent>,
    next_particle_id: u64,
}

impl ParticleEmitterRuntime {
    /// Initializes one emitter, including authored prewarming.
    #[must_use]
    pub fn new(emitter: ParticleEmitter, seed: u32, transform: Mat4) -> Self {
        let transform = runtime_transform(&emitter, transform);
        let mut random = RetailRandom::from_seed(seed);
        let prewarm = sample_time(emitter.runtime.timing.initial_update, &mut random);
        let rate = emitter
            .runtime
            .timing
            .emission_rate
            .sample(random.range_f32(-1.0, 1.0));
        let emission_interval = if rate > f32::EPSILON {
            rate.recip()
        } else {
            0.0
        };
        let (state, activity_remaining) = initial_activity(&emitter, &mut random);
        let mut runtime = Self {
            emitter,
            state,
            time: -prewarm,
            activity_remaining,
            emission_interval,
            next_emission_wait: 0.0,
            transform,
            secondary_transform: transform,
            particles: Vec::new(),
            random,
            nested_events: Vec::new(),
            next_particle_id: 0,
        };
        if runtime.is_trail() {
            runtime.spawn_control(runtime.time, transform);
        }
        if prewarm > f32::EPSILON {
            runtime.update(prewarm, transform, transform);
        }
        runtime.time = runtime.time.max(0.0);
        runtime
    }

    /// Advances timing and motion, splitting long frames into stable steps.
    pub fn update(&mut self, delta_seconds: f32, transform: Mat4, secondary_transform: Mat4) {
        self.update_with_terrain(delta_seconds, transform, secondary_transform, |_| None);
    }

    /// Advances the emitter and resolves optional terrain collision heights.
    /// The callback receives a world-space particle position.
    pub fn update_with_terrain(
        &mut self,
        delta_seconds: f32,
        transform: Mat4,
        secondary_transform: Mat4,
        mut terrain_height: impl FnMut(Vec3) -> Option<f32>,
    ) {
        let delta_seconds = finite_nonnegative(delta_seconds);
        let destination = runtime_transform(&self.emitter, transform);
        let secondary = runtime_transform(&self.emitter, secondary_transform);
        if delta_seconds <= f32::EPSILON {
            self.transform = destination;
            self.secondary_transform = secondary;
            return;
        }
        let start = self.transform;
        let secondary_start = self.secondary_transform;
        let mut elapsed = 0.0;
        while elapsed < delta_seconds {
            let step = (delta_seconds - elapsed).min(MAX_UPDATE_STEP);
            let alpha0 = elapsed / delta_seconds;
            let alpha1 = (elapsed + step) / delta_seconds;
            let step_start = interpolate_transform(start, destination, alpha0);
            let step_end = interpolate_transform(start, destination, alpha1);
            let secondary_end = interpolate_transform(secondary_start, secondary, alpha1);
            self.advance_step(step, step_start, step_end, &mut terrain_height);
            self.secondary_transform = secondary_end;
            elapsed += step;
        }
        self.transform = destination;
        self.secondary_transform = secondary;
    }

    /// Resolves all currently visible particles into renderer instances.
    #[must_use]
    pub fn instances(
        &self,
        material: &ParticleMaterial,
        context: ParticleRenderContext,
    ) -> Vec<ParticleInstance> {
        let mut instances = if self.is_beam() {
            self.beam_instances(material, context)
        } else if self.is_trail() {
            self.trail_instances(material, context)
        } else {
            self.regular_instances(material, context)
        };
        if self.emitter.sort_particles && !self.is_trail() && !self.is_beam() {
            let camera = Vec3::from_array(context.camera_position);
            instances.sort_by(|left, right| {
                let left_distance = Vec3::from_array(left.position).distance_squared(camera);
                let right_distance = Vec3::from_array(right.position).distance_squared(camera);
                right_distance.total_cmp(&left_distance)
            });
        }
        instances
    }

    /// Returns the immutable authored emitter definition.
    #[must_use]
    pub fn definition(&self) -> &ParticleEmitter {
        &self.emitter
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub fn state(&self) -> ParticleEmitterState {
        self.state
    }

    /// Returns the current runtime time in seconds.
    #[must_use]
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Returns the number of live control or render particles.
    #[must_use]
    pub fn live_particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Drains nested-effect requests generated since the last call.
    #[must_use]
    pub fn take_nested_events(&mut self) -> Vec<ParticleNestedEvent> {
        mem::take(&mut self.nested_events)
    }

    /// Stops new emission while allowing live particles to expire.
    pub fn stop(&mut self) {
        if self.state != ParticleEmitterState::Killed {
            self.state = ParticleEmitterState::Stopped;
            self.finish_if_empty();
        }
    }

    /// Immediately clears live particles and nested effects.
    pub fn kill(&mut self) {
        if matches!(self.emitter.kind, ParticleEmitterKind::NestedEffect(_)) {
            self.nested_events
                .extend(
                    self.particles
                        .iter()
                        .map(|particle| ParticleNestedEvent::Release {
                            particle_id: particle.id,
                            kill_immediately: true,
                        }),
                );
        }
        self.particles.clear();
        self.state = ParticleEmitterState::Killed;
    }

    fn advance_step(
        &mut self,
        duration: f32,
        start: Mat4,
        end: Mat4,
        terrain_height: &mut impl FnMut(Vec3) -> Option<f32>,
    ) {
        let mut elapsed = 0.0;
        while elapsed < duration {
            if self.state == ParticleEmitterState::Killed {
                return;
            }
            if self.state == ParticleEmitterState::Stopped {
                self.advance_particles(duration - elapsed, end, terrain_height);
                self.time += duration - elapsed;
                break;
            }
            if self.activity_remaining <= f32::EPSILON && !self.continuously_active() {
                self.transition_activity();
                continue;
            }
            let available = duration - elapsed;
            let slice = if self.continuously_active() {
                available
            } else {
                available.min(self.activity_remaining)
            };
            let alpha0 = elapsed / duration;
            let alpha1 = (elapsed + slice) / duration;
            let slice_start = interpolate_transform(start, end, alpha0);
            let slice_end = interpolate_transform(start, end, alpha1);
            if self.state == ParticleEmitterState::Active {
                self.advance_active(slice, slice_start, slice_end, terrain_height);
            } else {
                self.advance_particles(slice, slice_end, terrain_height);
                self.time += slice;
            }
            if !self.continuously_active() {
                self.activity_remaining = (self.activity_remaining - slice).max(0.0);
            }
            elapsed += slice;
        }
        self.settle_activity();
        self.finish_if_empty();
    }

    fn advance_active(
        &mut self,
        duration: f32,
        start: Mat4,
        end: Mat4,
        terrain_height: &mut impl FnMut(Vec3) -> Option<f32>,
    ) {
        if self.is_beam() {
            if self.particles.is_empty() {
                self.spawn_control(self.time, start);
            }
            self.advance_particles(duration, end, terrain_height);
            self.time += duration;
            if self.particles.is_empty() && self.emitter.runtime.timing.looping() {
                self.spawn_control(self.time, end);
            }
        } else if self.is_trail()
            && self.emitter.runtime.timing.trail_emission == ParticleTrailEmission::ByLength
        {
            self.advance_trail_by_length(duration, start, end, terrain_height);
        } else {
            self.advance_scheduled(duration, start, end, terrain_height);
        }
    }

    fn advance_scheduled(
        &mut self,
        duration: f32,
        start: Mat4,
        end: Mat4,
        terrain_height: &mut impl FnMut(Vec3) -> Option<f32>,
    ) {
        if self.emission_interval <= f32::EPSILON {
            self.advance_particles(duration, end, terrain_height);
            self.time += duration;
            return;
        }
        let mut elapsed = 0.0;
        while elapsed < duration {
            let remaining = duration - elapsed;
            let wait = self.next_emission_wait.min(remaining);
            if wait > 0.0 {
                let alpha = (elapsed + wait) / duration;
                let at = interpolate_transform(start, end, alpha);
                self.advance_particles(wait, at, terrain_height);
                self.time += wait;
                self.next_emission_wait -= wait;
                elapsed += wait;
            }
            if self.next_emission_wait > f32::EPSILON || elapsed >= duration {
                continue;
            }
            let at = interpolate_transform(start, end, elapsed / duration);
            let spawned = if self.is_trail() {
                self.spawn_control(self.time, at)
            } else {
                self.spawn_regular(self.time, at)
            };
            if spawned {
                self.next_emission_wait = self.emission_interval;
            } else {
                self.next_emission_wait = duration - elapsed;
                let tail = duration - elapsed;
                self.advance_particles(tail, end, terrain_height);
                self.time += tail;
                break;
            }
        }
    }

    fn advance_trail_by_length(
        &mut self,
        duration: f32,
        start: Mat4,
        end: Mat4,
        terrain_height: &mut impl FnMut(Vec3) -> Option<f32>,
    ) {
        let start_time = self.time;
        self.advance_particles(duration, end, terrain_height);
        self.time += duration;
        let last = self.particles.last().map_or_else(
            || control_position(&self.emitter, start),
            |particle| particle.position,
        );
        let destination = control_position(&self.emitter, end);
        let segment_length = self.emitter.runtime.timing.trail_segment_length.max(1.0e-6);
        let requested = (last.distance(destination) / segment_length)
            .round()
            .to_usize()
            .unwrap_or(0);
        let available = self.available_particle_slots();
        let count = requested.min(available);
        if count == 0 {
            return;
        }
        let divisor = count.to_f32().unwrap_or(1.0);
        for index in 1..=count {
            let alpha = index.to_f32().unwrap_or(divisor) / divisor;
            let at = control_transform(&self.emitter, end, last.lerp(destination, alpha));
            let birth = duration.mul_add(alpha, start_time);
            self.spawn_control(birth, at);
        }
        self.refresh_particle_ages();
    }

    fn advance_particles(
        &mut self,
        duration: f32,
        transform: Mat4,
        terrain_height: &mut impl FnMut(Vec3) -> Option<f32>,
    ) {
        let end_time = self.time + duration;
        let moving = !self.is_trail() && !self.is_beam();
        if moving {
            self.update_motion(duration, end_time, transform, terrain_height);
        }
        for particle in &mut self.particles {
            particle.age_seconds = (end_time - particle.birth_time).max(0.0);
        }
        self.emit_nested_transforms(transform);
        self.remove_dead_particles(end_time);
    }

    fn update_motion(
        &mut self,
        duration: f32,
        end_time: f32,
        transform: Mat4,
        terrain_height: &mut impl FnMut(Vec3) -> Option<f32>,
    ) {
        let definition = &self.emitter.runtime;
        let tied = definition.timing.tied_to_emitter();
        let random_table = &RANDOM_FLOATS[..];
        let random = &mut self.random;
        for particle in &mut self.particles {
            apply_magnets(
                particle,
                definition,
                transform,
                duration,
                random,
                random_table,
            );
            integrate_particle(particle, definition, duration, end_time);
            if definition.timing.collision_detection_terrain() {
                collide_with_terrain(particle, definition, transform, terrain_height, tied);
            }
        }
    }

    fn spawn_regular(&mut self, birth_time: f32, transform: Mat4) -> bool {
        if self.available_particle_slots() == 0 {
            return false;
        }
        let index = self.random.index(RANDOM_TABLE_SIZE);
        let random_values = Self::particle_random_values(index);
        let timing = &self.emitter.runtime.timing;
        let lifetime = timing.particle_life.sample(random_values[0]).max(0.0);
        let mut position = emission_position(&self.emitter.runtime.shape, &mut self.random);
        let mut velocity = emission_direction(&self.emitter.runtime.shape, &mut self.random)
            * timing.velocity.sample(random_values[0]);
        let mut rotation = initial_rotation(&self.emitter, velocity, &mut self.random);
        let angular_velocity = initial_angular_velocity(&self.emitter, &mut self.random);
        if !timing.tied_to_emitter() {
            position = transform.transform_point3(position);
            velocity = transform.transform_vector3(velocity);
            rotation = motion_rotation(&self.emitter, velocity).unwrap_or(rotation);
        }
        let distance = timing.initial_distance.sample(random_values[0]);
        if distance.abs() > f32::EPSILON {
            position += velocity.normalize_or_zero() * distance;
        }
        let texture_layers = self.choose_texture_layers();
        let particle = self.make_particle(
            birth_time,
            lifetime,
            ParticleSpawn {
                position,
                velocity,
                rotation,
                angular_velocity,
                up_axis: Vec3::Y,
                texture_layers,
                random_values,
            },
        );
        self.emit_nested_spawn(&particle, transform);
        self.particles.push(particle);
        true
    }

    fn spawn_control(&mut self, birth_time: f32, transform: Mat4) -> bool {
        if self.available_particle_slots() == 0 {
            return false;
        }
        let index = self.random.index(RANDOM_TABLE_SIZE);
        let random_values = Self::particle_random_values(index);
        let lifetime = self
            .emitter
            .runtime
            .timing
            .particle_life
            .sample(random_values[0])
            .max(0.0);
        let position = control_position(&self.emitter, transform);
        let up_axis = if self.emitter.runtime.timing.tied_to_emitter() {
            Vec3::Y
        } else {
            transform.transform_vector3(Vec3::Y).normalize_or_zero()
        };
        let texture_layers = self.choose_texture_layers();
        let particle = self.make_particle(
            birth_time,
            lifetime,
            ParticleSpawn {
                position,
                velocity: Vec3::ZERO,
                rotation: 0.0,
                angular_velocity: 0.0,
                up_axis,
                texture_layers,
                random_values,
            },
        );
        self.particles.push(particle);
        true
    }

    fn make_particle(
        &mut self,
        birth_time: f32,
        lifetime: f32,
        spawn: ParticleSpawn,
    ) -> RuntimeParticle {
        let id = self.next_particle_id;
        self.next_particle_id = self.next_particle_id.wrapping_add(1);
        RuntimeParticle {
            id,
            birth_time,
            death_time: birth_time + lifetime,
            age_seconds: (self.time - birth_time).max(0.0),
            position: spawn.position,
            velocity: spawn.velocity,
            rotation: spawn.rotation,
            angular_velocity: spawn.angular_velocity,
            up_axis: spawn.up_axis,
            texture_layers: spawn.texture_layers,
            random_values: spawn.random_values,
            progression_random: spawn.random_values[0],
        }
    }

    fn transition_activity(&mut self) {
        let timing = &self.emitter.runtime.timing;
        match self.state {
            ParticleEmitterState::Dormant => {
                self.state = ParticleEmitterState::Active;
                self.activity_remaining = sample_time(timing.emission_time, &mut self.random);
            }
            ParticleEmitterState::Active if timing.looping() => {
                self.state = ParticleEmitterState::Dormant;
                self.activity_remaining = sample_time(timing.loop_delay, &mut self.random);
            }
            ParticleEmitterState::Active => self.state = ParticleEmitterState::Stopped,
            ParticleEmitterState::Stopped | ParticleEmitterState::Killed => {}
        }
    }

    fn remove_dead_particles(&mut self, current_time: f32) {
        let nested = matches!(self.emitter.kind, ParticleEmitterKind::NestedEffect(_));
        let events = &mut self.nested_events;
        self.particles.retain(|particle| {
            let live = current_time < particle.death_time;
            if !live && nested {
                events.push(ParticleNestedEvent::Release {
                    particle_id: particle.id,
                    kill_immediately: false,
                });
            }
            live
        });
    }

    fn emit_nested_spawn(&mut self, particle: &RuntimeParticle, transform: Mat4) {
        let ParticleEmitterKind::NestedEffect(path) = &self.emitter.kind else {
            return;
        };
        let world_position = if self.emitter.runtime.timing.tied_to_emitter() {
            transform.transform_point3(particle.position)
        } else {
            particle.position
        };
        self.nested_events.push(ParticleNestedEvent::Spawn {
            particle_id: particle.id,
            path: path.clone(),
            transform: Mat4::from_translation(world_position),
        });
    }

    fn emit_nested_transforms(&mut self, transform: Mat4) {
        if !matches!(self.emitter.kind, ParticleEmitterKind::NestedEffect(_)) {
            return;
        }
        let tied = self.emitter.runtime.timing.tied_to_emitter();
        self.nested_events
            .extend(self.particles.iter().map(|particle| {
                let position = if tied {
                    transform.transform_point3(particle.position)
                } else {
                    particle.position
                };
                ParticleNestedEvent::Transform {
                    particle_id: particle.id,
                    transform: Mat4::from_translation(position),
                }
            }));
    }

    fn choose_texture_layers(&mut self) -> [u32; 4] {
        let definitions = [
            &self.emitter.material.diffuse[0],
            &self.emitter.material.diffuse[1],
            &self.emitter.material.diffuse[2],
            &self.emitter.material.intensity,
        ];
        definitions.map(|definition| choose_texture_layer(definition, &mut self.random))
    }

    fn particle_random_values(index: usize) -> [f32; 4] {
        std::array::from_fn(|offset| RANDOM_FLOATS[(index + offset) % RANDOM_TABLE_SIZE])
    }

    fn regular_instances(
        &self,
        material: &ParticleMaterial,
        context: ParticleRenderContext,
    ) -> Vec<ParticleInstance> {
        self.particles
            .iter()
            .filter_map(|particle| {
                particle_instance(
                    &self.emitter,
                    particle,
                    self.time,
                    self.transform,
                    material,
                    context,
                )
            })
            .collect()
    }

    fn trail_instances(
        &self,
        material: &ParticleMaterial,
        context: ParticleRenderContext,
    ) -> Vec<ParticleInstance> {
        let segment_count = self.particles.len().saturating_sub(1);
        let divisor = segment_count.to_f32().unwrap_or(1.0).max(1.0);
        self.particles
            .windows(2)
            .enumerate()
            .filter_map(|(index, pair)| {
                let start = self.control_world_position(&pair[0]);
                let end = self.control_world_position(&pair[1]);
                let start_u = index.to_f32().unwrap_or(0.0) / divisor;
                let end_u = (index + 1).to_f32().unwrap_or(divisor) / divisor;
                let stretch = (self.emitter.runtime.timing.trail_uv == ParticleTrailUv::Stretch)
                    .then_some([start_u, end_u]);
                let mut instance = segment_instance(
                    &self.emitter,
                    &pair[0],
                    self.time,
                    SegmentDescriptor {
                        start,
                        end,
                        distance_alpha: None,
                        stretch,
                    },
                    material,
                    context,
                )?;
                if self.emitter.runtime.timing.tied_to_emitter() {
                    instance.up_axis = self
                        .transform
                        .transform_vector3(pair[0].up_axis)
                        .normalize_or_zero()
                        .to_array();
                }
                Some(instance)
            })
            .collect()
    }

    fn beam_instances(
        &self,
        material: &ParticleMaterial,
        context: ParticleRenderContext,
    ) -> Vec<ParticleInstance> {
        let Some(particle) = self.particles.first() else {
            return Vec::new();
        };
        let timing = &self.emitter.runtime.timing;
        let count = timing.beam_tessellation.clamp(2, MAX_BEAM_TESSELLATION);
        let divisor = (count - 1).to_f32().unwrap_or(1.0);
        let offset = Vec3::from_array(self.emitter.runtime.shape.offset);
        let start = self.transform.transform_point3(offset);
        let end = self.secondary_transform.transform_point3(offset);
        let tangent_start = self
            .transform
            .transform_vector3(Vec3::from_array(timing.beam_tangent_1));
        let tangent_end = self
            .secondary_transform
            .transform_vector3(Vec3::from_array(timing.beam_tangent_2));
        (0..count - 1)
            .filter_map(|index| {
                let alpha0 = index.to_f32().unwrap_or(0.0) / divisor;
                let alpha1 = (index + 1).to_f32().unwrap_or(divisor) / divisor;
                let p0 = beam_position(start, tangent_start, end, tangent_end, alpha0);
                let p1 = beam_position(start, tangent_start, end, tangent_end, alpha1);
                segment_instance(
                    &self.emitter,
                    particle,
                    self.time,
                    SegmentDescriptor {
                        start: p0,
                        end: p1,
                        distance_alpha: Some(alpha0.midpoint(alpha1)),
                        stretch: None,
                    },
                    material,
                    context,
                )
            })
            .collect()
    }

    fn control_world_position(&self, particle: &RuntimeParticle) -> Vec3 {
        if self.emitter.runtime.timing.tied_to_emitter() {
            self.transform.transform_point3(particle.position)
        } else {
            particle.position
        }
    }

    fn refresh_particle_ages(&mut self) {
        for particle in &mut self.particles {
            particle.age_seconds = (self.time - particle.birth_time).max(0.0);
        }
    }

    fn available_particle_slots(&self) -> usize {
        let maximum = usize::try_from(self.emitter.max_particles).unwrap_or(usize::MAX);
        maximum.saturating_sub(self.particles.len())
    }

    fn continuously_active(&self) -> bool {
        let timing = &self.emitter.runtime.timing;
        self.state == ParticleEmitterState::Active
            && timing.looping()
            && timing.loop_delay.value < f32::EPSILON
    }

    fn settle_activity(&mut self) {
        while self.activity_remaining <= f32::EPSILON
            && matches!(
                self.state,
                ParticleEmitterState::Dormant | ParticleEmitterState::Active
            )
            && !self.continuously_active()
        {
            self.transition_activity();
        }
    }

    fn finish_if_empty(&mut self) {
        if self.state == ParticleEmitterState::Stopped && self.particles.is_empty() {
            self.state = ParticleEmitterState::Killed;
        }
    }

    fn is_beam(&self) -> bool {
        matches!(
            self.emitter.kind,
            ParticleEmitterKind::Render(
                ParticleGeometry::Beam
                    | ParticleGeometry::BeamVertical
                    | ParticleGeometry::BeamHorizontal
            )
        )
    }

    fn is_trail(&self) -> bool {
        matches!(
            self.emitter.kind,
            ParticleEmitterKind::Render(ParticleGeometry::Trail | ParticleGeometry::TrailCross)
        )
    }
}

fn initial_activity(
    emitter: &ParticleEmitter,
    random: &mut RetailRandom,
) -> (ParticleEmitterState, f32) {
    let timing = &emitter.runtime.timing;
    let emits = timing.emission_time.value > f32::EPSILON
        || (timing.looping() && timing.start_delay.value < f32::EPSILON);
    if !emits {
        return (ParticleEmitterState::Stopped, 0.0);
    }
    if timing.start_delay.value > 0.0 {
        (
            ParticleEmitterState::Dormant,
            sample_time(timing.start_delay, random),
        )
    } else {
        (
            ParticleEmitterState::Active,
            sample_time(timing.emission_time, random),
        )
    }
}

fn choose_texture_layer(definition: &ParticleTextureDefinition, random: &mut RetailRandom) -> u32 {
    if definition.stages.len() <= 1 {
        return 0;
    }
    let total = definition
        .stages
        .iter()
        .map(|stage| stage.weight.max(0.0))
        .sum::<f32>();
    let index = if total <= f32::EPSILON {
        random.index(definition.stages.len())
    } else {
        let selector = random.range_f32(0.0, total);
        definition
            .stages
            .iter()
            .scan(0.0, |sum, stage| {
                *sum += stage.weight.max(0.0);
                Some(*sum)
            })
            .position(|sum| sum > selector)
            .unwrap_or(definition.stages.len() - 1)
    };
    u32::try_from(index).unwrap_or(u32::MAX)
}

fn initial_rotation(emitter: &ParticleEmitter, velocity: Vec3, random: &mut RetailRandom) -> f32 {
    if emitter.runtime.force.random_orientation() {
        return random.range_f32(0.0, std::f32::consts::TAU);
    }
    motion_rotation(emitter, velocity).unwrap_or(0.0)
}

fn motion_rotation(emitter: &ParticleEmitter, velocity: Vec3) -> Option<f32> {
    let aligns = matches!(
        emitter.kind,
        ParticleEmitterKind::Render(ParticleGeometry::UpFacing | ParticleGeometry::VelocityAligned)
    );
    let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
    (aligns && horizontal.length_squared() > 1.0e-6).then(|| -horizontal.x.atan2(horizontal.z))
}

fn initial_angular_velocity(emitter: &ParticleEmitter, random: &mut RetailRandom) -> f32 {
    let force = &emitter.runtime.force;
    if !force.tumble() {
        return 0.0;
    }
    let mut velocity = random
        .range_f32(force.min_angular_velocity, force.max_angular_velocity)
        .to_radians();
    if force.tumble_both_directions() && random.index(2) == 0 {
        velocity = -velocity;
    }
    velocity
}

fn control_position(emitter: &ParticleEmitter, transform: Mat4) -> Vec3 {
    let offset = Vec3::from_array(emitter.runtime.shape.offset);
    if emitter.runtime.timing.tied_to_emitter() {
        offset
    } else {
        transform.transform_point3(offset)
    }
}

fn control_transform(emitter: &ParticleEmitter, transform: Mat4, position: Vec3) -> Mat4 {
    if emitter.runtime.timing.tied_to_emitter() {
        return transform;
    }
    let offset = Vec3::from_array(emitter.runtime.shape.offset);
    with_translation(transform, position - transform.transform_vector3(offset))
}

fn sample_time(varying: crate::particle::ParticleVarying, random: &mut RetailRandom) -> f32 {
    varying.sample(random.range_f32(-1.0, 1.0)).max(0.0)
}

fn runtime_transform(emitter: &ParticleEmitter, transform: Mat4) -> Mat4 {
    if emitter.runtime.timing.ignore_rotation() {
        Mat4::from_translation(transform.w_axis.truncate())
    } else {
        transform
    }
}

fn interpolate_transform(start: Mat4, end: Mat4, alpha: f32) -> Mat4 {
    let (start_scale, start_rotation, start_translation) = start.to_scale_rotation_translation();
    let (end_scale, end_rotation, end_translation) = end.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(
        start_scale.lerp(end_scale, alpha),
        start_rotation.slerp(end_rotation, alpha),
        start_translation.lerp(end_translation, alpha),
    )
}

fn with_translation(transform: Mat4, translation: Vec3) -> Mat4 {
    Mat4::from_cols(
        transform.x_axis,
        transform.y_axis,
        transform.z_axis,
        translation.extend(1.0),
    )
}

fn hermite(start: Vec3, tangent_start: Vec3, end: Vec3, tangent_end: Vec3, alpha: f32) -> Vec3 {
    let alpha2 = alpha * alpha;
    let alpha3 = alpha2 * alpha;
    start * alpha3.mul_add(2.0, (-3.0_f32).mul_add(alpha2, 1.0))
        + tangent_start * alpha3.mul_add(1.0, -2.0 * alpha2 + alpha)
        + end * alpha3.mul_add(-2.0, 3.0 * alpha2)
        + tangent_end * (alpha3 - alpha2)
}

fn beam_position(
    start: Vec3,
    tangent_start: Vec3,
    end: Vec3,
    tangent_end: Vec3,
    alpha: f32,
) -> Vec3 {
    if tangent_start.length_squared() <= f32::EPSILON
        && tangent_end.length_squared() <= f32::EPSILON
    {
        start.lerp(end, alpha)
    } else {
        hermite(start, tangent_start, end, tangent_end, alpha)
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}
