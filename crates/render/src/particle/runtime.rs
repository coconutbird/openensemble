//! Deterministic authored PFX emitter execution.

use glam::Mat4;

use super::ParticleEffect;

mod appearance;
mod emitter;
mod motion;
mod random;
mod shape;

#[cfg(test)]
mod tests;

pub use emitter::ParticleEmitterRuntime;

/// Lifecycle state of one authored emitter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParticleEmitterState {
    /// Waiting for a start or loop delay to expire.
    Dormant,
    /// Emitting particles during an active interval.
    Active,
    /// No longer emitting, but retaining live particles until they expire.
    Stopped,
    /// Fully released with no live particles.
    Killed,
}

/// Per-frame colors and opacity supplied by the owning render scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleRenderContext {
    /// Owning-player RGBA tint.
    pub player_color: [f32; 4],
    /// Active light-set sun RGBA tint.
    pub sun_color: [f32; 4],
    /// Runtime emitter-opacity multiplier.
    pub emitter_opacity: f32,
    /// Camera position used for optional back-to-front sorting.
    pub camera_position: [f32; 3],
}

impl Default for ParticleRenderContext {
    fn default() -> Self {
        Self {
            player_color: [1.0; 4],
            sun_color: [1.0; 4],
            emitter_opacity: 1.0,
            camera_position: [0.0; 3],
        }
    }
}

/// Request produced by an authored nested-PFX emitter.
#[derive(Clone, Debug, PartialEq)]
pub enum ParticleNestedEvent {
    /// Starts a child effect at the supplied world transform.
    Spawn {
        /// Stable particle identifier within the parent emitter.
        particle_id: u64,
        /// Authored child PFX path.
        path: String,
        /// Initial child world transform.
        transform: Mat4,
    },
    /// Moves a live child effect with its parent particle.
    Transform {
        /// Stable particle identifier within the parent emitter.
        particle_id: u64,
        /// Updated child world transform.
        transform: Mat4,
    },
    /// Releases a child effect after its parent particle dies.
    Release {
        /// Stable particle identifier within the parent emitter.
        particle_id: u64,
        /// Whether the child should be removed without completing its particles.
        kill_immediately: bool,
    },
}

/// Runtime collection for every emitter in one decoded effect.
#[derive(Clone, Debug)]
pub struct ParticleEffectRuntime {
    emitters: Vec<ParticleEmitterRuntime>,
}

impl ParticleEffectRuntime {
    /// Creates all emitter runtimes at a common world transform.
    #[must_use]
    pub fn new(effect: &ParticleEffect, seed: u32, transform: Mat4) -> Self {
        let emitters = effect
            .emitters
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, emitter)| {
                let stream = u32::try_from(index).unwrap_or(u32::MAX);
                let emitter_seed = seed ^ stream.wrapping_mul(0x9e37_79b9);
                ParticleEmitterRuntime::new(emitter, emitter_seed, transform)
            })
            .collect();
        Self { emitters }
    }

    /// Advances all emitters and uses the primary transform for beam endpoints.
    pub fn update(&mut self, delta_seconds: f32, transform: Mat4) {
        self.update_with_secondary(delta_seconds, transform, transform);
    }

    /// Advances all emitters with independent authoritative beam endpoints.
    pub fn update_with_secondary(
        &mut self,
        delta_seconds: f32,
        transform: Mat4,
        secondary_transform: Mat4,
    ) {
        for emitter in &mut self.emitters {
            emitter.update(delta_seconds, transform, secondary_transform);
        }
    }

    /// Returns emitter runtimes in authored order.
    #[must_use]
    pub fn emitters(&self) -> &[ParticleEmitterRuntime] {
        &self.emitters
    }

    /// Returns mutable emitter runtimes in authored order.
    #[must_use]
    pub fn emitters_mut(&mut self) -> &mut [ParticleEmitterRuntime] {
        &mut self.emitters
    }

    /// Stops new emission while allowing existing particles to finish.
    pub fn stop(&mut self) {
        for emitter in &mut self.emitters {
            emitter.stop();
        }
    }

    /// Immediately clears every emitter and child effect.
    pub fn kill(&mut self) {
        for emitter in &mut self.emitters {
            emitter.kill();
        }
    }
}
