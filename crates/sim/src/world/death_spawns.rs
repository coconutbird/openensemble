//! Database-backed squads created by the retail unit-death action.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::scenario::create_squad_from_prototype;
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[derive(Debug)]
struct DeathSpawnRequest {
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_squad_name: String,
    check_position: bool,
    maximum_count: i32,
}

impl World {
    /// Resolve death spawns atomically before the same substep removes dead units.
    pub(super) fn resolve_dead_unit_death_spawns(
        &mut self,
        database: &Database,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let dead_units = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (!unit.is_alive() && !unit.is_static_death_replacement()).then_some(id)
            })
            .collect::<Vec<_>>();
        for unit_id in dead_units {
            let Some(request) = self.death_spawn_request(unit_id, database) else {
                continue;
            };
            self.execute_death_spawn(&request, database, gameplay);
        }
    }

    fn death_spawn_request(
        &self,
        unit_id: EntityId,
        database: &Database,
    ) -> Option<DeathSpawnRequest> {
        let unit = self.get_unit(unit_id)?;
        let proto = find_proto_object(database, &unit.proto_object_name)?;
        let authored = proto
            .death_spawn_squad
            .as_ref()
            .map(|spawn| spawn.proto_squad.trim())
            .filter(|name| !name.is_empty());
        let player = self.get_player(unit.base.player_id)?;
        let proto_squad_name = player
            .technologies
            .death_spawn_squad(&unit.proto_object_name, authored)?
            .trim();
        if proto_squad_name.is_empty() {
            return None;
        }
        let static_config = proto.death_spawn_squad.as_ref();
        Some(DeathSpawnRequest {
            player_id: unit.base.player_id,
            position: unit.base.position,
            forward: unit.base.forward,
            proto_squad_name: proto_squad_name.to_owned(),
            check_position: static_config.is_some_and(|spawn| spawn.check_position.is_some()),
            maximum_count: static_config
                .and_then(|spawn| spawn.max_population_count)
                .unwrap_or_default(),
        })
    }

    fn execute_death_spawn(
        &mut self,
        request: &DeathSpawnRequest,
        database: &Database,
        gameplay: Option<&GameplayCatalog>,
    ) {
        if request.check_position && self.death_spawn_position_is_obstructed(request.position) {
            return;
        }
        let Some((logical_index, logical_proto)) =
            find_proto_squad(database, &request.proto_squad_name)
        else {
            return;
        };
        let logical_id = database_id(logical_proto.dbid, logical_index);
        if request.maximum_count > 0
            && self.player_squad_count(request.player_id, Some(logical_id))
                >= u32::try_from(request.maximum_count).unwrap_or(u32::MAX)
        {
            return;
        }
        let effective_proto = self
            .get_player(request.player_id)
            .map(|player| {
                player
                    .technologies
                    .resolved_squad_prototype(&request.proto_squad_name)
            })
            .and_then(|name| find_proto_squad(database, name))
            .map_or(logical_proto, |(_, proto)| proto);
        let spawn_player = if squad_forces_gaia(effective_proto, database) {
            GAIA_PLAYER
        } else {
            request.player_id
        };
        let squad_id = create_squad_from_prototype(
            self,
            spawn_player,
            request.position,
            request.forward,
            &request.proto_squad_name,
            database,
        );
        if let Some(gameplay) = gameplay
            && let Some(unit_ids) = self.get_squad(squad_id).map(|squad| squad.unit_ids.clone())
        {
            for unit_id in unit_ids {
                let _configured = self.configure_unit_revival(unit_id, gameplay);
            }
        }
        if self.squad_has_hero_revival(squad_id) {
            let _killed = self.kill_squad(squad_id, false);
        }
    }

    fn death_spawn_position_is_obstructed(&self, position: Vec3) -> bool {
        if !position.is_finite() {
            return true;
        }
        if self.has_terrain_simulation() {
            return self.terrain_height(position, false).is_none();
        }
        self.terrain_bounds()
            .is_some_and(|bounds| !bounds.contains(position))
    }

    fn squad_has_hero_revival(&self, squad_id: EntityId) -> bool {
        self.get_squad(squad_id).is_some_and(|squad| {
            squad.unit_ids.iter().any(|unit_id| {
                self.get_unit(*unit_id)
                    .is_some_and(crate::entities::Unit::has_hero_revival)
            })
        })
    }
}

fn find_proto_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|proto| proto.name.eq_ignore_ascii_case(name))
}

fn find_proto_squad<'database>(
    database: &'database Database,
    name: &str,
) -> Option<(usize, &'database ProtoSquad)> {
    database
        .squads
        .iter()
        .enumerate()
        .find(|(_, proto)| proto.name.eq_ignore_ascii_case(name))
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn squad_forces_gaia(proto: &ProtoSquad, database: &Database) -> bool {
    proto.units.as_ref().is_some_and(|units| {
        units.entries.iter().any(|entry| {
            find_proto_object(database, entry.proto_object.trim()).is_some_and(|object| {
                object
                    .flags
                    .iter()
                    .any(|flag| flag.eq_ignore_ascii_case("ForceToGaiaPlayer"))
            })
        })
    })
}

#[cfg(test)]
mod tests;
