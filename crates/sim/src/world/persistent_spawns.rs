//! Source-backed execution of persistent unit `SpawnSquad` actions.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, PersistentSpawnSquadProfile};
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::spawn::{spawn_squad_at, squad_prototype_id};
use glam::Vec3;
use pipeline::database::hw1::objects::{ProtoObject, TrainLimitType};
use pipeline::database::hw1::{Database, Squad as ProtoSquad};
use std::f32::consts::{FRAC_PI_2, TAU};

#[derive(Debug, Clone)]
struct SpawnOwner {
    id: EntityId,
    player_id: PlayerId,
    squad_id: Option<EntityId>,
    proto_object_name: String,
    position: Vec3,
    forward: Vec3,
    obstruction_radius: f32,
}

#[derive(Debug, Clone)]
struct SpawnedSquadSpec {
    logical_id: i32,
    build_points: f32,
    obstruction_radius: f32,
    flying: bool,
    rally_point_type: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct SpawnTrainLimit {
    count: u32,
    bucket: Option<u8>,
}

impl World {
    pub(super) fn update_persistent_squad_spawns(
        &mut self,
        elapsed: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let owner_ids = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (unit.is_operational()
                    && unit.base.player_id != GAIA_PLAYER
                    && !gameplay
                        .persistent_squad_spawns(&unit.proto_object_name)
                        .is_empty())
                .then_some(id)
            })
            .collect::<Vec<_>>();
        for owner_id in owner_ids {
            self.update_unit_persistent_spawns(owner_id, elapsed, database, gameplay);
        }
    }

    fn update_unit_persistent_spawns(
        &mut self,
        owner_id: EntityId,
        elapsed: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(owner) = self.spawn_owner(owner_id) else {
            return;
        };
        for profile in gameplay.persistent_squad_spawns(&owner.proto_object_name) {
            self.update_persistent_spawn_action(&owner, profile, elapsed, database);
        }
    }

    fn update_persistent_spawn_action(
        &mut self,
        owner: &SpawnOwner,
        profile: &PersistentSpawnSquadProfile,
        elapsed: f32,
        database: &Database,
    ) {
        if !self.persistent_spawn_enabled(owner, profile) {
            return;
        }
        let Some(spec) = self.spawned_squad_spec(owner.player_id, profile.squad_type(), database)
        else {
            return;
        };
        let live_auto_joins = if profile.auto_join() {
            self.live_join_count(owner.squad_id)
        } else {
            0
        };
        let ready = self.units.get_mut(owner.id).is_some_and(|unit| {
            unit.persistent_spawns
                .action_mut(profile.action_name())
                .advance(
                    elapsed,
                    spec.build_points,
                    profile.work_rate(),
                    profile.count(),
                    profile.auto_join(),
                    live_auto_joins,
                )
        });
        if !ready {
            return;
        }
        let spawned = self.try_persistent_squad_spawn(owner, profile, &spec, database);
        let next_variance = self.next_spawn_work_variance(profile.work_rate_variance());
        if let Some(unit) = self.units.get_mut(owner.id) {
            unit.persistent_spawns
                .action_mut(profile.action_name())
                .finish_attempt(spawned, spec.build_points, profile.count(), next_variance);
        }
    }

    fn try_persistent_squad_spawn(
        &mut self,
        owner: &SpawnOwner,
        profile: &PersistentSpawnSquadProfile,
        spec: &SpawnedSquadSpec,
        database: &Database,
    ) -> bool {
        let limit = Self::persistent_spawn_train_limit(owner, profile.squad_type(), database);
        if limit.is_some_and(|rule| {
            self.persistent_spawn_train_count(owner.id, profile.squad_type(), rule) >= rule.count
        }) {
            return false;
        }
        let (position, forward) =
            self.persistent_spawn_transform(owner, spec, profile.stationary());
        let Ok(squad_id) = spawn_squad_at(
            self,
            database,
            owner.player_id,
            spec.logical_id,
            position,
            forward,
        ) else {
            return false;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.trained_by = Some(owner.id);
            squad.train_limit_bucket = limit.and_then(|rule| rule.bucket);
        }
        self.issue_persistent_spawn_rally(owner, squad_id, spec, database);
        if profile.auto_join()
            && let Some(target_squad_id) = owner.squad_id
        {
            let _issued = self.issue_auto_join_order(owner.player_id, squad_id, target_squad_id);
        }
        true
    }

    fn spawn_owner(&self, owner_id: EntityId) -> Option<SpawnOwner> {
        let owner = self.units.get(owner_id)?;
        Some(SpawnOwner {
            id: owner_id,
            player_id: owner.base.player_id,
            squad_id: owner.squad_id,
            proto_object_name: owner.proto_object_name.clone(),
            position: owner.base.position,
            forward: planar_forward(owner.base.forward),
            obstruction_radius: owner.obstruction_radius(),
        })
    }

    fn persistent_spawn_enabled(
        &self,
        owner: &SpawnOwner,
        profile: &PersistentSpawnSquadProfile,
    ) -> bool {
        let Some(unit) = self.units.get(owner.id) else {
            return false;
        };
        let authored_enabled = !profile.starts_disabled();
        let player_enabled = self
            .get_player(owner.player_id)
            .map_or(authored_enabled, |player| {
                player.technologies.action_enabled(
                    &owner.proto_object_name,
                    profile.action_name(),
                    authored_enabled,
                )
            });
        unit.actions
            .is_enabled(profile.action_name(), !player_enabled)
    }

    fn live_join_count(&self, target_squad_id: Option<EntityId>) -> u32 {
        let Some(target_squad_id) = target_squad_id else {
            return 0;
        };
        let count = self
            .squads
            .iter()
            .filter(|(_, squad)| squad.is_alive() && squad.join_target() == Some(target_squad_id))
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    fn next_spawn_work_variance(&mut self, width: f32) -> f32 {
        if width > 0.0 {
            self.sim_rng.range_float(-0.5 * width, 0.5 * width)
        } else {
            0.0
        }
    }
}

impl World {
    fn spawned_squad_spec(
        &self,
        player_id: PlayerId,
        logical_name: &str,
        database: &Database,
    ) -> Option<SpawnedSquadSpec> {
        let logical_id = squad_prototype_id(database, logical_name)?;
        let effective_name = self.get_player(player_id).map_or(logical_name, |player| {
            player.technologies.resolved_squad_prototype(logical_name)
        });
        let squad =
            find_squad(database, effective_name).or_else(|| find_squad(database, logical_name))?;
        let leader = squad
            .units
            .as_ref()
            .and_then(|units| units.entries.first())
            .and_then(|entry| {
                self.effective_object(player_id, entry.proto_object.trim(), database)
            });
        Some(SpawnedSquadSpec {
            logical_id,
            build_points: finite_nonnegative(squad.build_points),
            obstruction_radius: leader.map_or(0.0, object_radius),
            flying: leader.is_some_and(object_is_flying),
            rally_point_type: leader.and_then(|object| object.rally_point.clone()),
        })
    }

    fn effective_object<'database>(
        &self,
        player_id: PlayerId,
        logical_name: &str,
        database: &'database Database,
    ) -> Option<&'database ProtoObject> {
        let effective_name = self.get_player(player_id).map_or(logical_name, |player| {
            player.technologies.resolved_unit_prototype(logical_name)
        });
        find_object(database, effective_name).or_else(|| find_object(database, logical_name))
    }

    fn persistent_spawn_train_limit(
        owner: &SpawnOwner,
        squad_type: &str,
        database: &Database,
    ) -> Option<SpawnTrainLimit> {
        let owner_proto = find_object(database, &owner.proto_object_name)?;
        owner_proto
            .train_limits
            .iter()
            .find(|limit| {
                limit.target.trim().eq_ignore_ascii_case(squad_type)
                    && !matches!(limit.limit_type, Some(TrainLimitType::Unit))
            })
            .map(|limit| SpawnTrainLimit {
                count: u32::from(limit.count.unwrap_or_default()),
                bucket: limit.bucket,
            })
    }

    fn persistent_spawn_train_count(
        &self,
        owner_id: EntityId,
        squad_type: &str,
        rule: SpawnTrainLimit,
    ) -> u32 {
        let count = self
            .squads
            .iter()
            .filter(|(_, squad)| {
                squad.trained_by == Some(owner_id)
                    && rule.bucket.map_or_else(
                        || squad.proto_squad_name.eq_ignore_ascii_case(squad_type),
                        |bucket| squad.train_limit_bucket == Some(bucket),
                    )
            })
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }
}

impl World {
    fn persistent_spawn_transform(
        &mut self,
        owner: &SpawnOwner,
        spec: &SpawnedSquadSpec,
        stationary: bool,
    ) -> (Vec3, Vec3) {
        if stationary {
            return (owner.position, owner.forward);
        }
        let angle = self.sim_rng.range_float(0.0, TAU);
        let distance_scale = self.sim_rng.range_float(2.0, 4.0);
        let (sin, cos) = angle.sin_cos();
        let direction = Vec3::new(sin, 0.0, cos);
        let distance = owner.obstruction_radius + spec.obstruction_radius * distance_scale;
        let desired = owner.position + direction * distance;
        let position = (0_u16..4)
            .filter_map(|step| {
                let rotation = angle + FRAC_PI_2 * f32::from(step);
                let (sin, cos) = rotation.sin_cos();
                let candidate = owner.position + Vec3::new(sin, 0.0, cos) * distance;
                self.clear_persistent_spawn_position(
                    candidate,
                    spec.obstruction_radius,
                    owner.id,
                    spec.flying,
                )
            })
            .min_by(|left, right| {
                xz_distance_squared(*left, desired).total_cmp(&xz_distance_squared(*right, desired))
            })
            .unwrap_or(desired);
        (position, direction)
    }

    fn clear_persistent_spawn_position(
        &self,
        mut candidate: Vec3,
        radius: f32,
        owner_id: EntityId,
        flying: bool,
    ) -> Option<Vec3> {
        if !candidate.is_finite() || self.is_outside_playable_bounds(candidate, true) {
            return None;
        }
        if !flying && self.has_terrain_simulation() {
            candidate.y = self.terrain_height(candidate, false)?;
        }
        let obstructed = self.units.iter().any(|(unit_id, unit)| {
            unit_id != owner_id
                && unit.is_alive()
                && !unit.is_garrisoned()
                && circle_overlaps_unit(
                    candidate,
                    radius,
                    unit.base.position,
                    unit.obstruction_half_extents,
                )
        });
        (!obstructed).then_some(candidate)
    }

    fn issue_persistent_spawn_rally(
        &mut self,
        owner: &SpawnOwner,
        spawned_squad_id: EntityId,
        spec: &SpawnedSquadSpec,
        database: &Database,
    ) {
        let Some(child_type) = spec.rally_point_type.as_deref() else {
            return;
        };
        let owner_type = find_object(database, &owner.proto_object_name)
            .and_then(|object| object.rally_point.as_deref());
        let rally = self
            .unit_rally_point(owner.id, owner.player_id)
            .filter(|_| owner_type.is_some_and(|kind| kind.eq_ignore_ascii_case(child_type)))
            .or_else(|| {
                child_type
                    .eq_ignore_ascii_case("Military")
                    .then(|| self.player_rally_point(owner.player_id))
                    .flatten()
            });
        let Some(rally) = rally else {
            return;
        };
        let mut destination = self.resolve_rally_point(rally);
        let direction = destination - owner.position;
        if direction.length() > 4.0 {
            destination -= direction.normalize() * 4.0;
        }
        let _issued = self.issue_move_order(owner.player_id, spawned_squad_id, destination);
    }
}

fn find_squad<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoSquad> {
    database
        .squads
        .iter()
        .find(|squad| squad.name.eq_ignore_ascii_case(name.trim()))
}

fn find_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(name.trim()))
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite())
        .unwrap_or_default()
        .max(0.0)
}

fn object_radius(object: &ProtoObject) -> f32 {
    finite_nonnegative(object.obstruction_radius_x)
        .max(finite_nonnegative(object.obstruction_radius_z))
}

fn object_is_flying(object: &ProtoObject) -> bool {
    object
        .movement_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Air"))
        || object
            .object_types
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case("Flying"))
}

fn circle_overlaps_unit(center: Vec3, radius: f32, unit: Vec3, half_extents: Vec3) -> bool {
    let extents = half_extents.abs();
    let nearest_x = center.x.clamp(unit.x - extents.x, unit.x + extents.x);
    let nearest_z = center.z.clamp(unit.z - extents.z, unit.z + extents.z);
    let delta_x = center.x - nearest_x;
    let delta_z = center.z - nearest_z;
    delta_x * delta_x + delta_z * delta_z <= radius.max(0.0).powi(2)
}

fn planar_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

fn xz_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let delta = left - right;
    delta.x * delta.x + delta.z * delta.z
}

#[cfg(test)]
mod tests;
