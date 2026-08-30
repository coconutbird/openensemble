//! Source-backed execution of persistent unit `AmbientLifeSpawner` actions.

use super::{elapsed_milliseconds, inside_square};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AmbientLifeSpawnerProfile, GameplayCatalog};
use crate::player::PlayerId;
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

#[derive(Debug, Clone)]
struct SpawnerOwner {
    player_id: PlayerId,
    proto_object_name: String,
    position: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct SpawnedAmbientSpec {
    squad_prototype_id: i32,
    obstruction_radius: f32,
}

impl World {
    pub(in crate::world) fn update_ambient_life_spawners(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(elapsed_ms) = elapsed_milliseconds(dt) else {
            return;
        };
        let object_ids = self.objects.ids().collect::<Vec<_>>();
        for object_id in object_ids {
            self.update_ambient_life_spawner(object_id, elapsed_ms, database, gameplay);
        }
    }

    fn update_ambient_life_spawner(
        &mut self,
        object_id: EntityId,
        elapsed_ms: u32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(owner) = self.ambient_spawner_owner(object_id) else {
            return;
        };
        let Some(profile) = gameplay
            .ambient_life_spawner(&owner.proto_object_name)
            .cloned()
        else {
            self.disconnect_ambient_spawner(object_id);
            return;
        };
        if !self.ambient_spawner_enabled(&owner, &profile) {
            self.disconnect_ambient_spawner(object_id);
            return;
        }
        let due = self.objects.get_mut(object_id).is_some_and(|object| {
            let state = &mut object.ambient_life_spawner;
            state.connect(profile.check_frequency_ms());
            if state.start() {
                return false;
            }
            state.opportunity_due(elapsed_ms, profile.check_frequency_ms())
        });
        if !due {
            return;
        }
        let Some(dangerous_squad_id) = self
            .first_ambient_spawner_opportunity(owner.position, profile.opportunity_check_radius())
        else {
            return;
        };
        if self.spawn_ambient_life(&owner, &profile, dangerous_squad_id, database, gameplay)
            && let Some(object) = self.objects.get_mut(object_id)
        {
            object.ambient_life_spawner.complete();
        }
    }

    fn ambient_spawner_owner(&self, object_id: EntityId) -> Option<SpawnerOwner> {
        let object = self
            .objects
            .get(object_id)
            .filter(|object| object.is_alive())?;
        Some(SpawnerOwner {
            player_id: object.base.player_id,
            proto_object_name: object.proto_object_name.clone(),
            position: object.base.position,
        })
    }

    fn ambient_spawner_enabled(
        &self,
        owner: &SpawnerOwner,
        profile: &AmbientLifeSpawnerProfile,
    ) -> bool {
        let authored_enabled = !profile.starts_disabled();
        self.get_player(owner.player_id)
            .map_or(authored_enabled, |player| {
                player.technologies.action_enabled(
                    &owner.proto_object_name,
                    profile.action_name(),
                    authored_enabled,
                )
            })
    }

    fn disconnect_ambient_spawner(&mut self, object_id: EntityId) {
        if let Some(object) = self.objects.get_mut(object_id) {
            object.ambient_life_spawner.disconnect();
        }
    }

    fn first_ambient_spawner_opportunity(&self, center: Vec3, radius: f32) -> Option<EntityId> {
        self.squads.iter().find_map(|(id, squad)| {
            (squad.is_alive() && inside_square(center, squad.base.position, radius)).then_some(id)
        })
    }

    fn spawn_ambient_life(
        &mut self,
        owner: &SpawnerOwner,
        profile: &AmbientLifeSpawnerProfile,
        dangerous_squad_id: EntityId,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(spec) = self.spawned_ambient_spec(owner.player_id, profile.squad_type(), database)
        else {
            return false;
        };
        let owner_radius = database
            .objects
            .iter()
            .find(|prototype| {
                prototype
                    .name
                    .eq_ignore_ascii_case(&owner.proto_object_name)
            })
            .map_or(0.0, object_radius);
        let angle = self.trigger_random_float(0.0, std::f32::consts::TAU);
        let direction = Vec3::new(angle.sin(), 0.0, angle.cos()).normalize_or(Vec3::Z);
        let distance_scalar = self.trigger_random_float(2.0, 4.0);
        let position =
            owner.position + direction * (owner_radius + spec.obstruction_radius * distance_scalar);
        let Ok(squad_id) = spawn_squad_at(
            self,
            database,
            owner.player_id,
            spec.squad_prototype_id,
            position,
            direction,
        ) else {
            return false;
        };
        self.initialize_spawned_ambient_life(squad_id, dangerous_squad_id, gameplay);
        true
    }

    fn initialize_spawned_ambient_life(
        &mut self,
        squad_id: EntityId,
        dangerous_squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) {
        let profile = self
            .squads
            .get(squad_id)
            .and_then(|squad| squad.unit_ids.first())
            .and_then(|id| self.units.get(*id))
            .and_then(|unit| gameplay.ambient_life(&unit.proto_object_name))
            .cloned();
        if let Some(profile) = profile {
            self.initialize_ambient_life(squad_id, &profile);
            let _accepted = self.flee_ambient_life_from_map(squad_id, Some(dangerous_squad_id));
        }
    }

    fn spawned_ambient_spec(
        &self,
        player_id: PlayerId,
        logical_squad_name: &str,
        database: &Database,
    ) -> Option<SpawnedAmbientSpec> {
        let squad_prototype_id = squad_prototype_id(database, logical_squad_name)?;
        let prototype = self.effective_ambient_squad(player_id, logical_squad_name, database)?;
        let member = prototype.units.as_ref()?.entries.first()?;
        let member_name =
            self.get_player(player_id)
                .map_or(member.proto_object.as_str(), |player| {
                    player
                        .technologies
                        .resolved_unit_prototype(&member.proto_object)
                });
        let member = database
            .objects
            .iter()
            .find(|prototype| prototype.name.eq_ignore_ascii_case(member_name))?;
        Some(SpawnedAmbientSpec {
            squad_prototype_id,
            obstruction_radius: object_radius(member),
        })
    }

    fn effective_ambient_squad<'a>(
        &self,
        player_id: PlayerId,
        logical_name: &str,
        database: &'a Database,
    ) -> Option<&'a ProtoSquad> {
        let effective_name = self.get_player(player_id).map_or(logical_name, |player| {
            player.technologies.resolved_squad_prototype(logical_name)
        });
        find_squad(database, effective_name).or_else(|| find_squad(database, logical_name))
    }
}

fn find_squad<'a>(database: &'a Database, name: &str) -> Option<&'a ProtoSquad> {
    database
        .squads
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name))
}

fn object_radius(prototype: &ProtoObject) -> f32 {
    finite_nonnegative(prototype.obstruction_radius_x)
        .max(finite_nonnegative(prototype.obstruction_radius_z))
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
