//! Retail fog of war: the visibility map, revealers and visibility queries.

mod grid;
mod sight;
mod spans;

pub use grid::{TileVisibility, VisibilityGrid};
pub(crate) use sight::VisibilityState;

use super::World;
use crate::entities::{Object, Revealer};
use crate::entity::Entity;
use crate::entity_id::{EntityClass, EntityId};
use crate::player::{PlayerId, TeamId, TeamRelation};
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

const REVEALER_PROTO_NAME: &str = "sys_revealer";

impl World {
    /// Return whether gameplay fog of war currently limits team visibility.
    #[must_use]
    pub const fn fog_of_war_enabled(&self) -> bool {
        self.fog_of_war_enabled
    }

    /// Enable or disable gameplay fog of war for every team (`BWorld_setFogOfWar`
    /// `0x1405D2A40`, sim part).
    ///
    /// Disabling explores every tile once more for each team, so every tile is visible and
    /// explored. Enabling again unexplores every tile once: the tiles stay explored and only
    /// the tiles of the sight circles stay visible.
    pub fn set_fog_of_war_enabled(&mut self, enabled: bool) {
        if enabled == self.fog_of_war_enabled {
            return;
        }
        self.fog_of_war_enabled = enabled;
        for team_id in self.visibility_team_ids() {
            if let Some(grid) = &mut self.visibility.grid {
                if enabled {
                    grid.unexplore_all(team_id);
                } else {
                    grid.explore_all(team_id);
                }
            }
        }
    }

    /// Return whether every team has explored every tile of the visibility map.
    ///
    /// A world without sim terrain or without teams has no explored map and returns `false`.
    #[must_use]
    pub fn black_map_is_cleared(&self) -> bool {
        let mut teams = self.visibility_team_ids();
        !teams.is_empty()
            && self
                .visibility
                .grid
                .as_ref()
                .is_some_and(|grid| teams.all(|team_id| grid.is_fully_explored(team_id)))
    }

    /// Explore every tile for every team and keep the fog (`BWorld_clearBlackMap`
    /// `0x1405D2BE0`): a black tile becomes fogged, a visible tile stays visible.
    pub fn clear_black_map(&mut self) {
        for team_id in self.visibility_team_ids() {
            if let Some(grid) = &mut self.visibility.grid {
                grid.explore_and_unexplore_all(team_id);
            }
        }
    }

    /// Make every fogged tile black again for every team
    /// (`BTriggerEffect_teResetBlackMap` `0x1404630E0`). A visible tile stays visible.
    pub fn reset_black_map(&mut self) {
        for team_id in self.visibility_team_ids() {
            if let Some(grid) = &mut self.visibility.grid {
                grid.reset_black_map(team_id);
            }
        }
    }

    /// Read-only visibility map of the sim data tiles, for the renderer and the minimap.
    #[must_use]
    pub const fn visibility_grid(&self) -> Option<&VisibilityGrid> {
        self.visibility.grid.as_ref()
    }

    /// Give a world without sim terrain a 256 by 256 visibility map of 1-unit tiles and mark
    /// the current sight circles, for tests of the visibility rules of other systems.
    #[cfg(test)]
    pub(crate) fn configure_test_visibility_grid(&mut self) {
        self.visibility = VisibilityState {
            grid: Some(VisibilityGrid::new(256, 1.0)),
            ..VisibilityState::default()
        };
        self.update_line_of_sight();
    }

    /// Create one retail class-0 revealer for a team.
    ///
    /// `lifespan_ms = Some(0)` is intentionally distinct from `None`: version
    /// 2 of trigger effect 285 authors a timed zero-life object, while version
    /// 3 treats a zero value as permanent.
    pub fn create_revealer(
        &mut self,
        database: &Database,
        team_id: TeamId,
        position: Vec3,
        line_of_sight_scalar: f32,
        lifespan_ms: Option<u32>,
    ) -> Option<EntityId> {
        if !position.is_finite() {
            return None;
        }
        let owner = self.first_player_on_team(team_id)?;
        let (prototype_id, prototype) = revealer_prototype(database)?;
        let minimum = database
            .game_data
            .as_ref()
            .and_then(|data| data.minimum_revealer_size)
            .filter(|value| value.is_finite())
            .unwrap_or(0.0);
        let scalar = if line_of_sight_scalar == Revealer::GLOBAL_LINE_OF_SIGHT {
            line_of_sight_scalar
        } else {
            line_of_sight_scalar.max(minimum)
        };
        let expiration = lifespan_ms.map(|lifespan| self.game_time_ms.wrapping_add(lifespan));
        let revealer = Revealer::new(team_id, scalar, prototype.los.unwrap_or(0.0), expiration);
        let id = self.objects.allocate_id();
        let object = Object::new_revealer(
            id,
            owner,
            position,
            prototype_id,
            prototype.name.clone(),
            revealer,
        );
        self.objects.insert(id, object);
        Some(id)
    }

    /// Get a live class-0 object by its generational ID.
    #[must_use]
    pub fn get_object(&self, id: EntityId) -> Option<&Object> {
        self.objects.get(id)
    }

    /// Mutably get a live class-0 object.
    pub fn get_object_mut(&mut self, id: EntityId) -> Option<&mut Object> {
        self.objects.get_mut(id)
    }

    /// Remove a class-0 object and invalidate its ID.
    pub fn remove_object(&mut self, id: EntityId) -> Option<Object> {
        self.objects.get(id)?;
        self.prepare_remove_object_authored_relationships(id);
        self.remove_owned_attachments(id);
        self.detach_attachment_from_parent(id);
        self.release_physics_object(id);
        let removed = self.objects.remove(id);
        if removed.is_some() {
            self.power_manager.forget_visual(id);
        }
        removed
    }

    /// Get authoritative revealer state for one class-0 object.
    #[must_use]
    pub fn get_revealer(&self, id: EntityId) -> Option<&Revealer> {
        self.get_object(id).and_then(Object::revealer)
    }

    /// Iterate active revealers in deterministic class-0 pool order.
    pub fn revealers(&self) -> impl Iterator<Item = (EntityId, &Revealer)> {
        self.objects
            .iter()
            .filter_map(|(id, object)| object.revealer().map(|revealer| (id, revealer)))
    }

    /// `BVisibleMap::isPositionVisibleToTeam`: a sight circle of the team sees the tile under
    /// a world point. With fog disabled every point is visible.
    #[must_use]
    pub fn is_position_visible_to_team(&self, team_id: TeamId, position: Vec3) -> bool {
        position.is_finite()
            && (!self.fog_of_war_enabled
                || self
                    .visibility
                    .grid
                    .as_ref()
                    .is_some_and(|grid| grid.is_position_visible(team_id, position)))
    }

    /// `BVisibleMap::isPositionExploredToTeam`: the team explored the tile under a world point.
    #[must_use]
    pub fn is_position_explored_to_team(&self, team_id: TeamId, position: Vec3) -> bool {
        position.is_finite()
            && self
                .visibility
                .grid
                .as_ref()
                .is_some_and(|grid| grid.is_position_explored(team_id, position))
    }

    /// Return whether one live entity should be presented to a team's UI.
    ///
    /// A team's own entities remain visible. Other entities are visible when fog is disabled,
    /// a visible tile of the team is under them (`BObject_getNewVisibility` `0x14029D0C0`) or
    /// a power forces them visible. An undetected enemy cloak hides them.
    #[must_use]
    pub fn is_entity_visible_to_team(&self, team_id: TeamId, entity_id: EntityId) -> bool {
        let Some(_position) = self.entity_position(entity_id) else {
            return false;
        };
        if self.is_entity_hidden_by_cloak_to_team(team_id, entity_id) {
            return false;
        }
        if !self.fog_of_war_enabled {
            return true;
        }
        let owned_by_team = self
            .entity_owner(entity_id)
            .and_then(|player_id| self.get_player(player_id))
            .is_some_and(|player| player.team_id == team_id);
        owned_by_team
            || self.entity_on_visible_tile(team_id, entity_id)
            || self.cleansing_forces_entity_visibility(team_id, entity_id)
            || self.wave_forces_entity_visibility(team_id, entity_id)
    }

    pub(super) fn is_entity_visible_from_player(
        &self,
        player_id: PlayerId,
        entity_id: EntityId,
    ) -> bool {
        self.get_player(player_id)
            .is_some_and(|player| self.is_entity_visible_to_team(player.team_id, entity_id))
    }

    /// Return whether an undetected enemy cloak suppresses an entity for a team.
    ///
    /// Attachments inherit their parent unit's cloak so presentation effects do
    /// not reveal a hidden squad independently of the unit model.
    #[must_use]
    pub fn is_entity_hidden_by_cloak_to_team(&self, team_id: TeamId, entity_id: EntityId) -> bool {
        let Some(squad_id) = self.cloak_squad_for_entity(entity_id) else {
            return false;
        };
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        if !squad.is_cloaked() || squad.is_cloak_detected() {
            return false;
        }
        let Some(owner_team) = self
            .get_player(squad.base.player_id)
            .map(|player| player.team_id)
        else {
            return false;
        };
        self.team_relation(team_id, owner_team) == TeamRelation::Enemy
    }

    pub(super) fn entity_hidden_by_cloak_from_player(
        &self,
        player_id: PlayerId,
        entity_id: EntityId,
    ) -> bool {
        self.get_player(player_id)
            .is_some_and(|player| self.is_entity_hidden_by_cloak_to_team(player.team_id, entity_id))
    }

    /// A squad is visible when one of its units is (`BSquad::isVisible`). A unit samples every
    /// tile under its sim bounding box; another entity samples the tile under its position.
    fn entity_on_visible_tile(&self, team_id: TeamId, entity_id: EntityId) -> bool {
        let Some(grid) = &self.visibility.grid else {
            return false;
        };
        let mask = 0x1_0000_u32.checked_shl(u32::from(team_id)).unwrap_or(0);
        match entity_id.class() {
            Some(EntityClass::Squad) => self.squads.get(entity_id).is_some_and(|squad| {
                squad
                    .unit_ids
                    .iter()
                    .any(|unit_id| self.entity_on_visible_tile(team_id, *unit_id))
            }),
            Some(EntityClass::Unit) => self.units.get(entity_id).is_some_and(|unit| {
                let (center, extents) = unit.simulation_bounds();
                center.is_finite()
                    && extents.is_finite()
                    && grid.bits_in_box(center - extents, center + extents) & mask != 0
            }),
            _ => self.entity_position(entity_id).is_some_and(|position| {
                position.is_finite() && grid.bits_in_box(position, position) & mask != 0
            }),
        }
    }

    pub(super) fn update_revealers(&mut self, dt: f32) {
        for (_, object) in self.objects.iter_mut() {
            object.update(dt);
        }
        let expired = self
            .objects
            .iter()
            .filter_map(|(id, object)| {
                object
                    .revealer()
                    .is_some_and(|revealer| revealer.is_expired(self.game_time_ms))
                    .then_some(id)
            })
            .collect::<Vec<_>>();
        for id in expired {
            let _removed = self.remove_object(id);
        }
    }

    fn first_player_on_team(&self, team_id: TeamId) -> Option<PlayerId> {
        self.players()
            .find(|player| player.team_id == team_id)
            .map(|player| player.id)
    }

    fn cloak_squad_for_entity(&self, mut entity_id: EntityId) -> Option<EntityId> {
        for _ in 0..8 {
            match entity_id.class()? {
                EntityClass::Squad => return Some(entity_id),
                EntityClass::Unit => return self.units.get(entity_id)?.squad_id,
                EntityClass::Object | EntityClass::Projectile => {
                    entity_id = self.entity_object_state(entity_id)?.attached_to()?;
                }
                _ => return None,
            }
        }
        None
    }
}

fn revealer_prototype(database: &Database) -> Option<(i32, &ProtoObject)> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(REVEALER_PROTO_NAME))
        .map(|(index, prototype)| (crate::prototypes::proto_id(index), prototype))
}

#[cfg(test)]
mod tests;
