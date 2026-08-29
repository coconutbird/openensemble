//! Sim-owned camera-shake programs consumed by renderer-local adapters.

use std::collections::BTreeMap;
use std::time::Duration;

use super::super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// Retail camera-shake state at the current simulation time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraShake {
    revision: u32,
    strength: f32,
    conservation_factor: f32,
}

impl CameraShake {
    /// Monotonic identity used to reset renderer-local accumulated shake.
    #[must_use]
    pub const fn revision(self) -> u32 {
        self.revision
    }

    /// Current strength after retail's quadratic trail-off curve.
    #[must_use]
    pub const fn strength(self) -> f32 {
        self.strength
    }

    /// Retail centering factor applied to the previous accumulated offset.
    #[must_use]
    pub const fn conservation_factor(self) -> f32 {
        self.conservation_factor
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct CameraShakeState {
    next_revision: u32,
    players: BTreeMap<PlayerId, CameraShakeProgram>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CameraShakeProgram {
    revision: u32,
    started_at_ms: u32,
    duration_ms: u32,
    strength: f32,
    trail_off_ms: u32,
    conservation_factor: f32,
}

impl World {
    /// Return one player's current shake envelope for renderer projection.
    #[must_use]
    pub fn camera_shake(&self, player_id: PlayerId) -> Option<CameraShake> {
        self.presentation_control
            .camera_shakes
            .players
            .get(&player_id)
            .and_then(|program| program.sample(self.game_time_ms))
    }

    pub(crate) fn start_camera_shake(
        &mut self,
        player_id: PlayerId,
        duration_ms: u32,
        strength: f32,
        trail_off_ms: u32,
        conservation_factor: f32,
    ) {
        if player_id == 0 || self.get_player(player_id).is_none() {
            return;
        }
        let revision = self.presentation_control.camera_shakes.allocate_revision();
        self.presentation_control.camera_shakes.players.insert(
            player_id,
            CameraShakeProgram {
                revision,
                started_at_ms: self.game_time_ms,
                duration_ms,
                strength: strength.max(0.0),
                trail_off_ms,
                conservation_factor: conservation_factor.clamp(0.0, 1.0),
            },
        );
    }

    pub(crate) fn update_camera_shakes(&mut self) {
        let game_time_ms = self.game_time_ms;
        self.presentation_control
            .camera_shakes
            .players
            .retain(|_, program| !program.completed(game_time_ms));
    }
}

impl CameraShakeState {
    fn allocate_revision(&mut self) -> u32 {
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        self.next_revision
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.next_revision);
        checksum.hash_u32(u32::try_from(self.players.len()).unwrap_or(u32::MAX));
        for (player_id, program) in &self.players {
            checksum.hash_u32(u32::from(*player_id));
            program.hash_state(checksum);
        }
    }
}

impl CameraShakeProgram {
    fn sample(self, game_time_ms: u32) -> Option<CameraShake> {
        if self.completed(game_time_ms) {
            return None;
        }
        let elapsed_ms = game_time_ms.wrapping_sub(self.started_at_ms);
        let strength = if elapsed_ms <= self.duration_ms {
            self.strength
        } else if self.trail_off_ms == 0 {
            0.0
        } else {
            let remaining_ms = self
                .duration_ms
                .saturating_add(self.trail_off_ms)
                .saturating_sub(elapsed_ms);
            let remaining_seconds = Duration::from_millis(u64::from(remaining_ms)).as_secs_f32();
            let trail_off_seconds =
                Duration::from_millis(u64::from(self.trail_off_ms)).as_secs_f32();
            let multiplier = remaining_seconds / trail_off_seconds;
            self.strength * multiplier * multiplier
        };
        Some(CameraShake {
            revision: self.revision,
            strength,
            conservation_factor: self.conservation_factor,
        })
    }

    fn completed(self, game_time_ms: u32) -> bool {
        game_time_ms.wrapping_sub(self.started_at_ms)
            > self.duration_ms.saturating_add(self.trail_off_ms)
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.revision);
        checksum.hash_u32(self.started_at_ms);
        checksum.hash_u32(self.duration_ms);
        checksum.hash_f32(self.strength);
        checksum.hash_u32(self.trail_off_ms);
        checksum.hash_f32(self.conservation_factor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shake_holds_then_uses_retail_quadratic_trail_off() {
        let mut world = World::new();
        world.init_players(1);
        world.start_camera_shake(1, 500, 2.0, 400, 0.5);
        assert_eq!(
            world.camera_shake(1).unwrap().strength().to_bits(),
            2.0_f32.to_bits()
        );
        world.advance_time(500);
        assert_eq!(
            world.camera_shake(1).unwrap().strength().to_bits(),
            2.0_f32.to_bits()
        );
        world.advance_time(200);
        assert_eq!(
            world.camera_shake(1).unwrap().strength().to_bits(),
            0.5_f32.to_bits()
        );
        world.advance_time(200);
        assert_eq!(
            world.camera_shake(1).unwrap().strength().to_bits(),
            0.0_f32.to_bits()
        );
        world.advance_time(1);
        assert!(world.camera_shake(1).is_none());
    }

    #[test]
    fn replacing_a_shake_allocates_a_new_revision() {
        let mut world = World::new();
        world.init_players(1);
        world.start_camera_shake(1, 10, 1.0, 0, 0.5);
        let first = world.camera_shake(1).unwrap().revision();
        world.start_camera_shake(1, 10, 1.0, 0, 0.5);
        assert!(world.camera_shake(1).unwrap().revision() > first);
    }
}
