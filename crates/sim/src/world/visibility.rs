//! Authoritative trigger-revealer state and visibility queries.

use super::World;
use crate::entities::{Object, Revealer};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, TeamId};
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

const REVEALER_PROTO_NAME: &str = "sys_revealer";

impl World {
    /// Return whether gameplay fog of war currently limits team visibility.
    #[must_use]
    pub const fn fog_of_war_enabled(&self) -> bool {
        self.fog_of_war_enabled
    }

    /// Enable or disable gameplay fog of war for every team.
    ///
    /// Retail explores the entire visibility map when disabled and unexplores
    /// it again when re-enabled. Exploring the map also permanently removes
    /// black-map coverage, even after fog is enabled again.
    pub fn set_fog_of_war_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.black_map_cleared = true;
        }
        self.fog_of_war_enabled = enabled;
    }

    /// Return whether all teams have explored the whole terrain map.
    ///
    /// Retail stores this per tile and per team. Until ordinary unit LOS owns
    /// a tile grid, this flag captures the exact whole-map operations exposed
    /// by trigger effects 713 and 863.
    #[must_use]
    pub const fn black_map_is_cleared(&self) -> bool {
        self.black_map_cleared
    }

    /// Explore the entire terrain map for every team while retaining fog.
    pub fn clear_black_map(&mut self) {
        self.black_map_cleared = true;
    }

    /// Restore unexplored terrain for every team.
    ///
    /// Tiles that are actively visible cannot become black in retail, so a
    /// disabled fog map remains fully explored.
    pub fn reset_black_map(&mut self) {
        self.black_map_cleared = !self.fog_of_war_enabled;
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
        self.remove_owned_attachments(id);
        self.detach_attachment_from_parent(id);
        self.objects.remove(id)
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

    /// Return whether trigger-created revealers currently cover a world point.
    ///
    /// This is deliberately narrower than eventual complete fog-of-war state:
    /// ordinary unit LOS and persistent explored terrain are separate sources.
    #[must_use]
    pub fn is_position_revealed_to_team(&self, team_id: TeamId, position: Vec3) -> bool {
        position.is_finite()
            && (!self.fog_of_war_enabled
                || self.objects.iter().any(|(_, object)| {
                    object.base.is_alive()
                        && object.revealer().is_some_and(|revealer| {
                            revealer.team_id() == team_id
                                && revealer.covers(object.base.position, position)
                        })
                }))
    }

    /// Return whether active revealers cover a live entity's current position.
    #[must_use]
    pub fn is_entity_revealed_to_team(&self, team_id: TeamId, entity_id: EntityId) -> bool {
        self.entity_position(entity_id)
            .is_some_and(|position| self.is_position_revealed_to_team(team_id, position))
    }

    /// Return whether one live entity should be presented to a team's UI.
    ///
    /// A team's own entities remain visible. Other entities are visible when
    /// fog is disabled or an authoritative revealer covers their position.
    #[must_use]
    pub fn is_entity_visible_to_team(&self, team_id: TeamId, entity_id: EntityId) -> bool {
        let Some(position) = self.entity_position(entity_id) else {
            return false;
        };
        if !self.fog_of_war_enabled {
            return true;
        }
        let owned_by_team = self
            .entity_owner(entity_id)
            .and_then(|player_id| self.get_player(player_id))
            .is_some_and(|player| player.team_id == team_id);
        owned_by_team || self.is_position_revealed_to_team(team_id, position)
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
}

fn revealer_prototype(database: &Database) -> Option<(i32, &ProtoObject)> {
    database
        .objects
        .iter()
        .enumerate()
        .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(REVEALER_PROTO_NAME))
        .and_then(|(index, prototype)| {
            i32::try_from(index)
                .ok()
                .map(|index| (prototype.dbid.unwrap_or(index), prototype))
        })
}

#[cfg(test)]
mod tests;
