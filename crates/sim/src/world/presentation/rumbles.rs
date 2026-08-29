//! Authoritative trigger rumble requests for platform presentation adapters.

use std::collections::BTreeMap;
use std::time::Duration;

use super::super::World;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// One gamepad motor's retail rumble type and strength.
#[derive(Debug, Clone, PartialEq)]
pub struct RumbleMotor {
    rumble_type: Option<String>,
    strength: f32,
}

impl RumbleMotor {
    pub(crate) fn new(rumble_type: Option<String>, strength: f32) -> Self {
        Self {
            rumble_type,
            strength,
        }
    }

    /// Retail rumble type name, or `None` when this motor is disabled.
    #[must_use]
    pub fn rumble_type(&self) -> Option<&str> {
        self.rumble_type.as_deref()
    }

    /// Authored motor strength.
    #[must_use]
    pub const fn strength(&self) -> f32 {
        self.strength
    }
}

/// One active sim-owned rumble request.
#[derive(Debug, Clone, PartialEq)]
pub struct RumbleRequest {
    id: i32,
    player_id: PlayerId,
    left: RumbleMotor,
    right: RumbleMotor,
    duration_seconds: f32,
    looped: bool,
    pattern: Option<String>,
    started_at_ms: u32,
}

impl RumbleRequest {
    /// Trigger-visible request ID used by retail's stop effect.
    #[must_use]
    pub const fn id(&self) -> i32 {
        self.id
    }

    /// Player whose local gamepad receives the request.
    #[must_use]
    pub const fn player_id(&self) -> PlayerId {
        self.player_id
    }

    /// Left motor settings used when no valid named pattern is available.
    #[must_use]
    pub const fn left(&self) -> &RumbleMotor {
        &self.left
    }

    /// Right motor settings used when no valid named pattern is available.
    #[must_use]
    pub const fn right(&self) -> &RumbleMotor {
        &self.right
    }

    /// Authored non-pattern duration in seconds.
    #[must_use]
    pub const fn duration_seconds(&self) -> f32 {
        self.duration_seconds
    }

    /// Whether the request loops until explicitly stopped.
    #[must_use]
    pub const fn looped(&self) -> bool {
        self.looped
    }

    /// Optional named rumble pattern resolved by the platform adapter.
    #[must_use]
    pub fn pattern(&self) -> Option<&str> {
        self.pattern.as_deref()
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct RumbleState {
    next_id: i32,
    active: BTreeMap<i32, RumbleRequest>,
}

impl World {
    /// Iterate one player's active rumble requests in stable ID order.
    pub fn rumble_requests(&self, player_id: PlayerId) -> impl Iterator<Item = &RumbleRequest> {
        self.presentation_control
            .rumbles
            .active
            .values()
            .filter(move |request| request.player_id == player_id)
    }

    /// Look up one active rumble request by trigger-visible ID.
    #[must_use]
    pub fn rumble_request(&self, id: i32) -> Option<&RumbleRequest> {
        self.presentation_control.rumbles.active.get(&id)
    }

    pub(crate) fn start_rumble(
        &mut self,
        player_id: PlayerId,
        left: RumbleMotor,
        right: RumbleMotor,
        duration_seconds: f32,
        looped: bool,
        pattern: Option<String>,
    ) -> Option<i32> {
        if player_id == 0 || self.get_player(player_id).is_none() || !duration_seconds.is_finite() {
            return None;
        }
        let id = self.presentation_control.rumbles.allocate_id();
        self.presentation_control.rumbles.active.insert(
            id,
            RumbleRequest {
                id,
                player_id,
                left,
                right,
                duration_seconds,
                looped,
                pattern,
                started_at_ms: self.game_time_ms,
            },
        );
        Some(id)
    }

    pub(crate) fn stop_rumble(&mut self, player_id: PlayerId, id: i32) -> bool {
        if self
            .presentation_control
            .rumbles
            .active
            .get(&id)
            .is_none_or(|request| request.player_id != player_id)
        {
            return false;
        }
        self.presentation_control
            .rumbles
            .active
            .remove(&id)
            .is_some()
    }

    pub(crate) fn update_rumbles(&mut self) {
        let game_time_ms = self.game_time_ms;
        self.presentation_control
            .rumbles
            .active
            .retain(|_, request| !request.completed(game_time_ms));
    }
}

impl RumbleState {
    fn allocate_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(0);
        id
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.next_id);
        checksum.hash_u32(u32::try_from(self.active.len()).unwrap_or(u32::MAX));
        for request in self.active.values() {
            request.hash_state(checksum);
        }
    }
}

impl RumbleRequest {
    fn completed(&self, game_time_ms: u32) -> bool {
        if self.looped || self.pattern.is_some() {
            return false;
        }
        Duration::from_millis(u64::from(game_time_ms.wrapping_sub(self.started_at_ms)))
            .as_secs_f32()
            > self.duration_seconds.max(0.0)
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.id);
        checksum.hash_u32(u32::from(self.player_id));
        self.left.hash_state(checksum);
        self.right.hash_state(checksum);
        checksum.hash_f32(self.duration_seconds);
        checksum.hash_u32(u32::from(self.looped));
        hash_optional_string(checksum, self.pattern.as_deref());
        checksum.hash_u32(self.started_at_ms);
    }
}

impl RumbleMotor {
    fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_optional_string(checksum, self.rumble_type.as_deref());
        checksum.hash_f32(self.strength);
    }
}

fn hash_optional_string(checksum: &mut SyncChecksum, value: Option<&str>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(value.as_bytes());
    } else {
        checksum.hash_u32(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_expire_or_stop_without_leaking_between_players() {
        let mut world = World::new();
        world.init_players(2);
        let motor = || RumbleMotor::new(Some("Fixed".to_owned()), 0.75);
        let first = world
            .start_rumble(1, motor(), motor(), 1.0, false, None)
            .unwrap();
        let second = world
            .start_rumble(2, motor(), motor(), 1.0, true, None)
            .unwrap();
        assert_eq!(world.rumble_requests(1).count(), 1);
        assert!(!world.stop_rumble(1, second));
        assert!(world.stop_rumble(2, second));

        world.advance_time(1_000);
        assert!(world.rumble_request(first).is_some());
        world.advance_time(1);
        assert!(world.rumble_request(first).is_none());
    }
}
