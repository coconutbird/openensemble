//! Retail synchronized projectile-velocity perturbation state.

use crate::gameplay::{ProjectileInitialPerturbance, ProjectilePerturbanceProfile};
use crate::random::SimRandom;
use crate::sync::SyncChecksum;
use glam::Vec3;

const VECTOR_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone)]
pub(super) struct ProjectilePerturbance {
    vector: Vec3,
    chance: f32,
    velocity: f32,
    min_time: f32,
    max_time: f32,
    duration: f32,
    timer: f32,
    initial: Option<ProjectileInitialPerturbance>,
    initial_pending: bool,
    active: bool,
}

impl ProjectilePerturbance {
    pub(super) fn from_profile(profile: ProjectilePerturbanceProfile) -> Option<Self> {
        (profile.initial.is_some() || profile.chance > 0.0).then_some(Self {
            vector: Vec3::ZERO,
            chance: profile.chance,
            velocity: profile.velocity,
            min_time: profile.min_time,
            max_time: profile.max_time,
            duration: 0.0,
            timer: 0.0,
            initial: profile.initial,
            initial_pending: profile.initial.is_some(),
            active: false,
        })
    }

    pub(super) const fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn update(&mut self, elapsed: f32) -> Vec3 {
        self.timer += elapsed;
        let done = self.timer >= self.duration;
        if done {
            self.timer = self.duration;
        }
        let perturbance = if self.duration > VECTOR_EPSILON {
            self.vector * (std::f32::consts::PI * self.timer / self.duration).sin()
        } else {
            Vec3::ZERO
        };
        if done {
            self.active = false;
            self.initial_pending = false;
        }
        perturbance
    }

    pub(super) fn attempt_start(&mut self, roll: f32, velocity_ratio: f32, rng: &mut SimRandom) {
        if !self.initial_pending && roll > self.chance {
            return;
        }
        let (velocity, min_time, max_time) = self.initial.map_or(
            (self.velocity * velocity_ratio, self.min_time, self.max_time),
            |initial| {
                if self.initial_pending {
                    (initial.velocity, initial.min_time, initial.max_time)
                } else {
                    (self.velocity * velocity_ratio, self.min_time, self.max_time)
                }
            },
        );
        self.timer = 0.0;
        self.duration = min_time + (max_time - min_time) * rng.distribution();
        let vector = Vec3::new(
            rng.range_float(-1.0, 1.0),
            rng.range_float(-1.0, 1.0),
            rng.range_float(-1.0, 1.0),
        );
        self.vector = vector.normalize_or_zero() * velocity;
        self.active = true;
    }

    pub(super) fn disable_after_fuel(&mut self) {
        self.active = false;
        self.chance = 0.0;
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_vec3(self.vector.x, self.vector.y, self.vector.z);
        checksum.hash_f32(self.chance);
        checksum.hash_f32(self.velocity);
        checksum.hash_f32(self.min_time);
        checksum.hash_f32(self.max_time);
        checksum.hash_f32(self.duration);
        checksum.hash_f32(self.timer);
        if let Some(initial) = self.initial {
            checksum.hash_u32(1);
            checksum.hash_f32(initial.velocity);
            checksum.hash_f32(initial.min_time);
            checksum.hash_f32(initial.max_time);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::from(self.initial_pending));
        checksum.hash_u32(u32::from(self.active));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recurring_profile(chance: f32) -> ProjectilePerturbanceProfile {
        ProjectilePerturbanceProfile {
            chance,
            velocity: 12.0,
            min_time: 0.2,
            max_time: 0.4,
            initial: None,
        }
    }

    #[test]
    fn successful_start_consumes_duration_and_three_vector_draws() {
        let mut state = ProjectilePerturbance::from_profile(recurring_profile(1.0)).unwrap();
        let mut actual = SimRandom::new();
        let attempt = actual.distribution();
        state.attempt_start(attempt, 0.5, &mut actual);

        let mut expected = SimRandom::new();
        for _ in 0..5 {
            let _draw = expected.distribution();
        }
        assert_eq!(actual.seed(), expected.seed());
        assert!(state.is_active());
        assert!((state.vector.length() - 6.0).abs() < 0.000_1);
        assert!((0.2..=0.4).contains(&state.duration));
    }

    #[test]
    fn perturbation_follows_retail_half_sine_and_finishes_at_zero() {
        let mut state = ProjectilePerturbance::from_profile(ProjectilePerturbanceProfile {
            chance: 0.0,
            velocity: 0.0,
            min_time: 0.0,
            max_time: 0.0,
            initial: Some(ProjectileInitialPerturbance {
                velocity: 10.0,
                min_time: 1.0,
                max_time: 1.0,
            }),
        })
        .unwrap();
        let mut rng = SimRandom::new();
        let attempt = rng.distribution();
        state.attempt_start(attempt, 1.0, &mut rng);

        let midpoint = state.update(0.5);
        assert!((midpoint.length() - 10.0).abs() < 0.000_1);
        assert!(state.is_active());
        let endpoint = state.update(0.5);
        assert!(endpoint.length() < 0.000_1);
        assert!(!state.is_active());
    }
}
