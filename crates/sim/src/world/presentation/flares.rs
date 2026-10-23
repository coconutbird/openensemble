//! Trigger minimap flares (`BAlertManager` flare alerts) for UI adapters.

use std::collections::BTreeMap;

use glam::Vec3;

use super::super::World;
use crate::player::{PlayerId, TeamRelation};
use crate::sync::SyncChecksum;

/// The time in milliseconds that a flare alert shows its icon (`created + 6000`).
const FLARE_VISIBLE_MS: u32 = 6_000;
/// The time in milliseconds after which a flare alert expires (`created + 30000`).
const FLARE_EXPIRE_MS: u32 = 30_000;

/// One flare alert of a player (`BAlertManager_createFlareAlert` `0x14017B5F0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapFlare {
    location: Vec3,
    from_player: i32,
    created_at_ms: u32,
}

impl MinimapFlare {
    /// The flare location.
    #[must_use]
    pub const fn location(self) -> Vec3 {
        self.location
    }

    /// The player that the flare comes from. A spoofed flare can name any player number.
    #[must_use]
    pub const fn from_player(self) -> i32 {
        self.from_player
    }

    /// Game time in milliseconds when the flare started.
    #[must_use]
    pub const fn created_at_ms(self) -> u32 {
        self.created_at_ms
    }

    /// Game time until which the flare icon shows.
    #[must_use]
    pub const fn visible_until_ms(self) -> u32 {
        self.created_at_ms.wrapping_add(FLARE_VISIBLE_MS)
    }

    /// Game time at which the alert expires.
    #[must_use]
    pub const fn expires_at_ms(self) -> u32 {
        self.created_at_ms.wrapping_add(FLARE_EXPIRE_MS)
    }

    const fn expired(self, now_ms: u32) -> bool {
        now_ms.wrapping_sub(self.created_at_ms) >= FLARE_EXPIRE_MS
    }
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct FlareState {
    players: BTreeMap<PlayerId, Vec<MinimapFlare>>,
}

impl World {
    /// Iterate the flare alerts of a player that did not expire, oldest first.
    pub fn minimap_flares(&self, player_id: PlayerId) -> impl Iterator<Item = MinimapFlare> + '_ {
        let now_ms = self.game_time_ms;
        self.presentation_control
            .flares
            .players
            .get(&player_id)
            .into_iter()
            .flatten()
            .copied()
            .filter(move |flare| !flare.expired(now_ms))
    }

    /// `FlareMinimapNormal` (135): a flare for each player whose team is allied to the team of
    /// the source player, Gaia included. An invalid source player gives no flare.
    pub(crate) fn flare_minimap_allies(&mut self, location: Vec3, from_player: i32) {
        let Some(source_team) = u8::try_from(from_player)
            .ok()
            .and_then(|player_id| self.get_player(player_id))
            .map(|player| player.team_id)
        else {
            return;
        };
        let targets: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|player| self.team_relation(player.team_id, source_team) == TeamRelation::Ally)
            .map(|player| player.id)
            .collect();
        for player_id in targets {
            self.add_minimap_flare(player_id, location, from_player);
        }
    }

    /// Add a flare alert to a valid player and drop the expired alerts of the player.
    pub(crate) fn add_minimap_flare(
        &mut self,
        player_id: PlayerId,
        location: Vec3,
        from_player: i32,
    ) {
        if self.get_player(player_id).is_none() {
            return;
        }
        let now_ms = self.game_time_ms;
        let flares = self
            .presentation_control
            .flares
            .players
            .entry(player_id)
            .or_default();
        flares.retain(|flare| !flare.expired(now_ms));
        flares.push(MinimapFlare {
            location,
            from_player,
            created_at_ms: now_ms,
        });
    }
}

impl FlareState {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.players.len()).unwrap_or(u32::MAX));
        for (player_id, flares) in &self.players {
            checksum.hash_u32(u32::from(*player_id));
            checksum.hash_u32(u32::try_from(flares.len()).unwrap_or(u32::MAX));
            for flare in flares {
                checksum.hash_vec3(flare.location.x, flare.location.y, flare.location.z);
                checksum.hash_i32(flare.from_player);
                checksum.hash_u32(flare.created_at_ms);
            }
        }
    }
}
