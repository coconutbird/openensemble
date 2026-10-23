//! Sight circles: the retail LOS marks of units and revealers on the visibility map.
//!
//! Each object update of retail `BObject::update` (2008 `object.cpp:349-367`) calls
//! `BObject_markLOSOn` (`0x14029CD50`), `BObject_markLOSUpdate` (`0x14029CF30`) or
//! `BObject_markLOSOff` (`0x14029CE80`). The mark is a circle of `BObject_getSimLOS`
//! (`0x140299F50`) tiles at the truncated sim tile of the object position. This port keeps
//! one mark per entity and brings all marks up to date once per world update, after the
//! entity updates of the tick.

use super::grid::VisibilityGrid;
use crate::entities::Revealer;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{MAX_TEAMS, TeamId};
use crate::sync::SyncChecksum;
use crate::world::World;
use glam::Vec3;
use num_traits::ToPrimitive;
use std::collections::BTreeMap;

/// Sim LOS of a revealer that explores the whole map.
const WHOLE_MAP: i32 = -1;
/// LOS scalar that `BObject::setLOSScalar` treats as "no scalar".
const UNSCALED: f32 = 1.0;

/// One explored sight circle on the visibility map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SightMark {
    team_id: TeamId,
    x: i32,
    z: i32,
    /// Radius in tiles, or `WHOLE_MAP`.
    radius: i32,
}

/// Visibility map and the sight circles that it counts.
#[derive(Debug, Clone, Default)]
pub(crate) struct VisibilityState {
    pub(super) grid: Option<VisibilityGrid>,
    pub(super) marks: BTreeMap<EntityId, SightMark>,
}

impl VisibilityState {
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        if let Some(grid) = &self.grid {
            checksum.hash_u32(1);
            grid.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::try_from(self.marks.len()).unwrap_or(u32::MAX));
        for (id, mark) in &self.marks {
            checksum.hash_u32(id.as_u32());
            checksum.hash_u32(u32::from(mark.team_id));
            checksum.hash_i32(mark.x);
            checksum.hash_i32(mark.z);
            checksum.hash_i32(mark.radius);
        }
    }
}

/// Sight that an entity wants this update.
#[derive(Debug, Clone, Copy)]
struct Sight {
    team_id: TeamId,
    position: Vec3,
    radius: i32,
    revealer: bool,
}

/// Proto sim LOS of `BPlayer` setup (2008 `player.cpp:233-247`): truncated tiles, at least 1
/// for a positive LOS.
pub(super) fn proto_sim_line_of_sight(line_of_sight: f32, reciprocal_tile_scale: f32) -> i32 {
    if line_of_sight.is_nan() || line_of_sight <= 0.0 {
        return 0;
    }
    truncate_tiles(line_of_sight * reciprocal_tile_scale).max(1)
}

/// `BObject_getSimLOS` with the LOS scalar flag: truncated tiles of the scaled LOS. A result
/// of 0 or less is 1, or `WHOLE_MAP` for a revealer.
fn scaled_sim_line_of_sight(
    proto_line_of_sight: f32,
    scalar: f32,
    reciprocal_tile_scale: f32,
    revealer: bool,
) -> i32 {
    if proto_line_of_sight.is_nan() || proto_line_of_sight <= 0.0 {
        return 0;
    }
    let tiles = truncate_tiles(proto_line_of_sight * scalar * reciprocal_tile_scale);
    if tiles > 0 {
        tiles
    } else if revealer {
        WHOLE_MAP
    } else {
        1
    }
}

/// Truncate toward zero and saturate like the `__vctsxs` conversion of retail.
fn truncate_tiles(value: f32) -> i32 {
    value
        .to_i32()
        .unwrap_or(if value > 0.0 { i32::MAX } else { i32::MIN })
}

/// Sim LOS of a trigger revealer (`BWorld::createRevealer` sets the LOS scalar).
pub(super) fn revealer_sim_line_of_sight(revealer: &Revealer, reciprocal_tile_scale: f32) -> i32 {
    let scalar = revealer.line_of_sight_scalar();
    // `BObject::setLOSScalar(1.0)` clears the scalar flag: the proto sim LOS applies.
    if scalar.to_bits() == UNSCALED.to_bits() {
        proto_sim_line_of_sight(revealer.prototype_line_of_sight(), reciprocal_tile_scale)
    } else {
        scaled_sim_line_of_sight(
            revealer.prototype_line_of_sight(),
            scalar,
            reciprocal_tile_scale,
            true,
        )
    }
}

impl World {
    /// Make a black visibility map for the sim data tiles of the configured terrain.
    pub(in crate::world) fn reset_visibility_grid(&mut self) {
        self.visibility.marks.clear();
        self.visibility.grid = self.terrain_simulation.as_ref().map(|terrain| {
            let (axis, tile_scale) = terrain.data_grid();
            VisibilityGrid::new(axis, tile_scale)
        });
        if !self.fog_of_war_enabled {
            for team_id in self.visibility_team_ids() {
                if let Some(grid) = &mut self.visibility.grid {
                    grid.explore_all(team_id);
                }
            }
        }
    }

    /// Teams of the retail whole-map loops: `1..getNumberTeams()`.
    pub(super) fn visibility_team_ids(&self) -> std::ops::Range<TeamId> {
        let count = self
            .players()
            .map(|player| player.team_id.saturating_add(1))
            .max()
            .unwrap_or(1);
        1..count.min(TeamId::try_from(MAX_TEAMS).unwrap_or(TeamId::MAX))
    }

    /// Bring every sight circle up to date with the entity positions and LOS values.
    pub(in crate::world) fn update_line_of_sight(&mut self) {
        let Some(grid) = &self.visibility.grid else {
            return;
        };
        let reciprocal = grid.tile_scale().recip();
        let center = grid.axis().to_f32().unwrap_or(0.0) * grid.tile_scale() * 0.5;
        let sights = self.current_sights(reciprocal);
        let stale = self
            .visibility
            .marks
            .keys()
            .filter(|id| !sights.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for id in stale {
            if let Some(mark) = self.visibility.marks.remove(&id) {
                self.unmark(mark);
            }
        }
        for (id, sight) in sights {
            let whole_map = sight.revealer && sight.radius == WHOLE_MAP;
            if whole_map {
                // `BObject_markLOSOn` moves the whole-map revealer to the map center.
                if let Some(object) = self.objects.get_mut(id) {
                    object.base.position = Vec3::new(center, 0.0, center);
                }
            }
            let (x, z) = if whole_map {
                (0, 0)
            } else {
                (
                    truncate_tiles(sight.position.x * reciprocal),
                    truncate_tiles(sight.position.z * reciprocal),
                )
            };
            let wanted = SightMark {
                team_id: sight.team_id,
                x,
                z,
                radius: sight.radius,
            };
            match self.visibility.marks.get(&id).copied() {
                // `markLOSUpdate` returns early for a LOS of 0 or less.
                Some(mark) if mark == wanted || sight.radius <= 0 => {}
                Some(mark) => {
                    self.unmark(mark);
                    self.mark(id, wanted);
                }
                None if sight.radius > 0 || sight.radius == WHOLE_MAP => self.mark(id, wanted),
                None => {}
            }
        }
    }

    fn mark(&mut self, id: EntityId, mark: SightMark) {
        let Some(grid) = &mut self.visibility.grid else {
            return;
        };
        if mark.radius == WHOLE_MAP {
            grid.explore_all(mark.team_id);
        } else {
            grid.explore_circle(mark.x, mark.z, mark.radius, mark.team_id);
        }
        self.visibility.marks.insert(id, mark);
    }

    fn unmark(&mut self, mark: SightMark) {
        let Some(grid) = &mut self.visibility.grid else {
            return;
        };
        if mark.radius == WHOLE_MAP {
            grid.unexplore_all(mark.team_id);
        } else {
            grid.unexplore_circle(mark.x, mark.z, mark.radius, mark.team_id);
        }
    }

    /// Sight of each unit and revealer that has LOS this update, in entity order.
    fn current_sights(&self, reciprocal_tile_scale: f32) -> BTreeMap<EntityId, Sight> {
        let mut sights = BTreeMap::new();
        for (id, unit) in self.units.iter() {
            // `BUnit::kill` and garrison turn LOS off; physics replacements are thrown
            // objects that `BUnit::init` creates without LOS.
            if !unit.is_alive() || unit.is_garrisoned() || unit.is_physics_replacement() {
                continue;
            }
            let Some(player) = self.get_player(unit.base.player_id) else {
                continue;
            };
            let authored = unit.authored_line_of_sight();
            let current = player
                .technologies
                .line_of_sight(unit.logical_proto_object_name(), authored);
            let scalar = unit.line_of_sight_scalar;
            let radius = if scalar.to_bits() == UNSCALED.to_bits() {
                unit_proto_sim_line_of_sight(authored, current, reciprocal_tile_scale)
            } else {
                scaled_sim_line_of_sight(current, scalar, reciprocal_tile_scale, false)
            };
            sights.insert(
                id,
                Sight {
                    team_id: player.team_id,
                    position: unit.base.position,
                    radius,
                    revealer: false,
                },
            );
        }
        for (id, object) in self.objects.iter() {
            let Some(revealer) = object.revealer() else {
                continue;
            };
            if !object.base.is_alive() {
                continue;
            }
            sights.insert(
                id,
                Sight {
                    team_id: revealer.team_id(),
                    position: object.base.position,
                    radius: revealer_sim_line_of_sight(revealer, reciprocal_tile_scale),
                    revealer: true,
                },
            );
        }
        sights
    }
}

/// Proto sim LOS of a unit. A technology that changes `LOS` sets the truncated tiles without
/// the minimum of 1 (2008 `techeffect.cpp:1100-1106`).
fn unit_proto_sim_line_of_sight(authored: f32, current: f32, reciprocal_tile_scale: f32) -> i32 {
    if current.to_bits() == authored.to_bits() {
        proto_sim_line_of_sight(authored, reciprocal_tile_scale)
    } else if current.is_nan() {
        0
    } else {
        truncate_tiles(current * reciprocal_tile_scale)
    }
}

#[cfg(test)]
mod tests;
