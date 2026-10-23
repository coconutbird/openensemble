//! Retail visibility map (`BVisibleMap`): per-team reference counts on the sim data tiles.
//!
//! `BVisibleMap_initMap` (`0x1405BE260`) allocates one 12-byte tile for each sim data tile of
//! the square `gTerrainSimDataTileCount` grid. A tile has a visibility word, a blocked word and
//! one 9-bit reference count per team. Bit `1 << team` of the visibility word is "explored"
//! (black map off) and bit `0x10000 << team` is "visible" (fog off). A count of 511 is a black
//! tile, 0 is a fogged tile and 1 to 509 are the number of sight circles that see the tile
//! (`BVisibleMap_exploreTile` `0x1405BEBD0`, `BVisibleMap_unexploreTile` `0x1405BED10`,
//! `BVisibleMap_resetBlackMapTile` `0x1405BEE20`). The tile index is `x * axis + z`.
//!
//! The blocked word belongs to the LOS blockers of trigger effect 418 (`BWorld_createBlocker`
//! `0x1405C72A0`). No shipped script uses that effect, so this port has no blocked word.

use super::spans::circle;
use crate::player::{MAX_TEAMS, TeamId};
use crate::sync::SyncChecksum;
use glam::Vec3;
use num_traits::ToPrimitive;

/// Reference count of a tile that the team has never explored (`cBlackValue`).
const BLACK: u16 = 511;
/// Reference count of an explored tile that no sight circle sees (`cFogValue`).
const FOG: u16 = 0;
/// Largest count before the 9-bit field would reach the black value (`cSentinelValue`).
const SENTINEL: u16 = BLACK - 1;

/// Retail state of one tile for one team (`BVisibleMap::checkMask`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileVisibility {
    /// The team has never explored the tile (black map).
    Unexplored,
    /// The team explored the tile, but no sight circle sees it now.
    Fogged,
    /// A sight circle of the team sees the tile now.
    Visible,
}

/// Read-only retail visibility grid of the sim data tiles.
#[derive(Debug, Clone)]
pub struct VisibilityGrid {
    axis: usize,
    tile_scale: f32,
    reciprocal_tile_scale: f32,
    visibility: Vec<u32>,
    references: Vec<[u16; MAX_TEAMS]>,
    unexplored: [usize; MAX_TEAMS],
}

impl VisibilityGrid {
    /// Make a black grid of `axis` by `axis` tiles of `tile_scale` world units.
    pub(crate) fn new(axis: usize, tile_scale: f32) -> Self {
        let tiles = axis * axis;
        Self {
            axis,
            tile_scale,
            reciprocal_tile_scale: tile_scale.recip(),
            visibility: vec![0; tiles],
            references: vec![[BLACK; MAX_TEAMS]; tiles],
            unexplored: [tiles; MAX_TEAMS],
        }
    }

    /// Number of tiles on each side of the square grid.
    #[must_use]
    pub const fn axis(&self) -> usize {
        self.axis
    }

    /// World size of one tile.
    #[must_use]
    pub const fn tile_scale(&self) -> f32 {
        self.tile_scale
    }

    /// Visibility word of a tile: bit `team` is explored, bit `16 + team` is visible.
    #[must_use]
    pub fn visibility_bits(&self, x: usize, z: usize) -> Option<u32> {
        self.index(x, z).map(|index| self.visibility[index])
    }

    /// Retail state of a tile for a team.
    #[must_use]
    pub fn tile_visibility(&self, team_id: TeamId, x: usize, z: usize) -> Option<TileVisibility> {
        let bits = self.visibility_bits(x, z)?;
        let team = usize::from(team_id);
        if team >= MAX_TEAMS || bits & explored_bit(team) == 0 {
            Some(TileVisibility::Unexplored)
        } else if bits & visible_bit(team) == 0 {
            Some(TileVisibility::Fogged)
        } else {
            Some(TileVisibility::Visible)
        }
    }

    /// Tile under a world position, with the retail truncation toward zero.
    #[must_use]
    pub fn tile_at(&self, position: Vec3) -> Option<(usize, usize)> {
        let x = usize::try_from((position.x * self.reciprocal_tile_scale).to_i32()?).ok()?;
        let z = usize::try_from((position.z * self.reciprocal_tile_scale).to_i32()?).ok()?;
        (x < self.axis && z < self.axis).then_some((x, z))
    }

    /// `BVisibleMap::isPositionVisibleToTeam`: a sight circle of the team sees the tile.
    #[must_use]
    pub fn is_position_visible(&self, team_id: TeamId, position: Vec3) -> bool {
        self.position_has(position, visible_bit(usize::from(team_id)))
    }

    /// `BVisibleMap::isPositionExploredToTeam`: the team explored the tile.
    #[must_use]
    pub fn is_position_explored(&self, team_id: TeamId, position: Vec3) -> bool {
        self.position_has(position, explored_bit(usize::from(team_id)))
    }

    /// Whether the team has explored every tile.
    #[must_use]
    pub fn is_fully_explored(&self, team_id: TeamId) -> bool {
        self.unexplored
            .get(usize::from(team_id))
            .is_some_and(|count| *count == 0)
    }

    /// OR of the visibility words under a world-space box (`BObject_getNewVisibility`).
    ///
    /// The tile corners are clamped to the grid before the truncation, so a box off the map
    /// samples the nearest edge tiles.
    pub(super) fn bits_in_box(&self, minimum: Vec3, maximum: Vec3) -> u32 {
        let Some(last) = self.axis.checked_sub(1).and_then(|last| last.to_f32()) else {
            return 0;
        };
        let to_tile = |value: f32| {
            (value * self.reciprocal_tile_scale)
                .max(0.0)
                .min(last)
                .to_usize()
        };
        let (Some(x1), Some(z1), Some(x2), Some(z2)) = (
            to_tile(minimum.x),
            to_tile(minimum.z),
            to_tile(maximum.x),
            to_tile(maximum.z),
        ) else {
            return 0;
        };
        let mut bits = 0;
        for x in x1..=x2 {
            for z in z1..=z2 {
                bits |= self.visibility[x * self.axis + z];
            }
        }
        bits
    }

    fn position_has(&self, position: Vec3, mask: u32) -> bool {
        self.tile_at(position)
            .and_then(|(x, z)| self.visibility_bits(x, z))
            .is_some_and(|bits| bits & mask != 0)
    }

    fn index(&self, x: usize, z: usize) -> Option<usize> {
        (x < self.axis && z < self.axis).then(|| x * self.axis + z)
    }

    fn signed_index(&self, x: i64, z: i64) -> Option<usize> {
        self.index(usize::try_from(x).ok()?, usize::try_from(z).ok()?)
    }

    /// `BVisibleMap_exploreTile`: one more sight circle of the team sees the tile.
    fn explore(&mut self, index: usize, team: usize) {
        let count = self.references[index][team];
        if count == SENTINEL {
            return;
        }
        if count == BLACK {
            self.unexplored[team] -= 1;
            self.visibility[index] |= explored_bit(team) | visible_bit(team);
            self.references[index][team] = 1;
        } else {
            if count == FOG {
                self.visibility[index] |= visible_bit(team);
            }
            self.references[index][team] = count + 1;
        }
    }

    /// `BVisibleMap_unexploreTile`: one sight circle of the team no longer sees the tile.
    fn unexplore(&mut self, index: usize, team: usize) {
        let count = self.references[index][team];
        if count == FOG || count == BLACK {
            return;
        }
        self.references[index][team] = count - 1;
        if count == 1 {
            self.visibility[index] &= !visible_bit(team);
        }
    }

    /// `BVisibleMap_resetBlackMapTile`: a fogged tile becomes black again.
    fn reset_black(&mut self, index: usize, team: usize) {
        if self.references[index][team] != FOG {
            return;
        }
        self.references[index][team] = BLACK;
        self.unexplored[team] += 1;
        self.visibility[index] &= !(explored_bit(team) | visible_bit(team));
    }

    fn for_each_tile(
        &mut self,
        team_id: TeamId,
        mut operation: impl FnMut(&mut Self, usize, usize),
    ) {
        let team = usize::from(team_id);
        if team >= MAX_TEAMS {
            return;
        }
        for index in 0..self.visibility.len() {
            operation(self, index, team);
        }
    }

    /// `BVisibleMap_exploreEntireMap` (`0x1405BEB00`).
    pub(super) fn explore_all(&mut self, team_id: TeamId) {
        self.for_each_tile(team_id, Self::explore);
    }

    /// `BVisibleMap_unexploreEntireMap` (`0x1405BEB70`).
    pub(super) fn unexplore_all(&mut self, team_id: TeamId) {
        self.for_each_tile(team_id, Self::unexplore);
    }

    /// `BVisibleMap::updateEntireMap`: explore then unexplore each tile, so a black tile
    /// becomes fogged and the other tiles keep their count (`BWorld_clearBlackMap`).
    pub(super) fn explore_and_unexplore_all(&mut self, team_id: TeamId) {
        self.for_each_tile(team_id, |grid, index, team| {
            grid.explore(index, team);
            grid.unexplore(index, team);
        });
    }

    /// `BVisibleMap::resetBlackMap(team)`: every fogged tile becomes black.
    pub(super) fn reset_black_map(&mut self, team_id: TeamId) {
        self.for_each_tile(team_id, Self::reset_black);
    }

    /// `BVisibleMap_exploreCircularRegion` (`0x1405BF1D0`).
    pub(super) fn explore_circle(&mut self, x: i32, z: i32, radius: i32, team_id: TeamId) {
        self.for_each_circle_tile(x, z, radius, team_id, Self::explore);
    }

    /// `BVisibleMap_unexploreCircularRegion` (`0x1405BF2F0`).
    pub(super) fn unexplore_circle(&mut self, x: i32, z: i32, radius: i32, team_id: TeamId) {
        self.for_each_circle_tile(x, z, radius, team_id, Self::unexplore);
    }

    fn for_each_circle_tile(
        &mut self,
        x: i32,
        z: i32,
        radius: i32,
        team_id: TeamId,
        operation: fn(&mut Self, usize, usize),
    ) {
        let team = usize::from(team_id);
        if team >= MAX_TEAMS {
            return;
        }
        for span in circle(radius) {
            let column = i64::from(x) + i64::from(span.x);
            let start = i64::from(z) + i64::from(span.start_z);
            for row in start..start + i64::from(span.count) {
                if let Some(index) = self.signed_index(column, row) {
                    operation(self, index, team);
                }
            }
        }
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.axis).unwrap_or(u32::MAX));
        checksum.hash_f32(self.tile_scale);
        for &bits in &self.visibility {
            checksum.hash_u32(bits);
        }
    }
}

const fn explored_bit(team: usize) -> u32 {
    if team < MAX_TEAMS { 1 << team } else { 0 }
}

const fn visible_bit(team: usize) -> u32 {
    if team < MAX_TEAMS {
        0x1_0000 << team
    } else {
        0
    }
}

#[cfg(test)]
mod tests;
