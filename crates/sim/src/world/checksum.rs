//! Deterministic hashing of authoritative world state.

use super::World;
use super::team::hash_team_relations;
use crate::entities::{Base, BaseEntity, BaseId, Projectile, Squad, Unit};
use crate::entity::EntityManager;
use crate::entity_id::EntityId;
use crate::player::Player;
use crate::sync::SyncChecksum;
use glam::Vec3;
use std::collections::BTreeMap;

impl World {
    /// Compute a checksum of the entire world state for sync verification.
    ///
    /// This hashes all deterministic state: game time, players, entities, and
    /// active orders. Two simulations with the same inputs should produce
    /// identical checksums.
    #[must_use]
    pub fn checksum(&self) -> u32 {
        let mut checksum = SyncChecksum::new();
        checksum.hash_u32(self.game_time_ms);
        hash_players(&mut checksum, &self.players);
        hash_team_relations(&mut checksum, &self.team_relations);
        hash_units(&mut checksum, &self.units);
        hash_squads(&mut checksum, &self.squads);
        hash_projectiles(&mut checksum, &self.projectiles);
        hash_bases(&mut checksum, &self.bases);
        checksum.value()
    }

    /// Compute a full checksum that also includes RNG state.
    #[must_use]
    pub fn checksum_with_rng(&self) -> u32 {
        let mut checksum = SyncChecksum::new();
        checksum.hash_u32(self.checksum());
        let mut rng = self.rng.clone();
        for _ in 0..8 {
            checksum.hash_u32(rng.u_rand());
        }
        checksum.value()
    }
}

fn hash_players(checksum: &mut SyncChecksum, players: &[Player]) {
    checksum.hash_u32(u32::try_from(players.len()).unwrap_or(u32::MAX));
    for player in players {
        checksum.hash_u32(u32::from(player.id));
        checksum.hash_u32(u32::from(player.team_id));
        checksum.hash_i32(player.civ_id);
        checksum.hash_i32(player.leader_id);
        checksum.hash_u32(player.state as u32);
        checksum.hash_u32(player.player_type as u32);
        for &amount in &player.resources.amounts {
            checksum.hash_f32(amount);
        }
        for population in &player.population {
            checksum.hash_f32(population.count);
            checksum.hash_f32(population.max);
            checksum.hash_f32(population.cap);
            checksum.hash_f32(population.future);
        }
        player.technologies.hash_state(checksum);
        player.research.hash_state(checksum);
    }
}

fn hash_units(checksum: &mut SyncChecksum, units: &EntityManager<Unit>) {
    checksum.hash_u32(u32::try_from(units.len()).unwrap_or(u32::MAX));
    for (_, unit) in units.iter() {
        hash_base_entity(checksum, &unit.base);
        checksum.hash_u32(unit.kind as u32);
        checksum.hash_u32(unit.archetype as u32);
        checksum.hash_u32(unit.state as u32);
        checksum.hash_i32(unit.proto_object_id);
        checksum.hash_u32(u32::try_from(unit.proto_object_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(unit.proto_object_name.as_bytes());
        checksum.hash_f32(unit.hitpoints);
        checksum.hash_f32(unit.max_hitpoints);
        unit.shields.hash_state(checksum);
        checksum.hash_f32(unit.damage_multiplier);
        checksum.hash_f32(unit.damage_taken_multiplier);
        checksum.hash_f32(unit.speed);
        checksum.hash_f32(unit.acceleration);
        checksum.hash_f32(unit.turn_rate_degrees);
        checksum.hash_vec3(
            unit.obstruction_half_extents.x,
            unit.obstruction_half_extents.y,
            unit.obstruction_half_extents.z,
        );
        if let Some(body) = &unit.physics {
            checksum.hash_u32(1);
            body.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        hash_optional_vec3(checksum, unit.move_target);
        hash_optional_entity_id(checksum, unit.attack_target);
        checksum.hash_f32(unit.attack_range);
        checksum.hash_u32(unit.attack_ability_id.map_or(u32::MAX, u32::from));
        unit.actions.hash_state(checksum);
        unit.combat.hash_state(checksum);
        unit.production.hash_state(checksum);
        hash_optional_entity_id(checksum, unit.squad_id);
        hash_optional_base_id(checksum, unit.base_id);
        checksum.hash_vec3(
            unit.formation_offset.x,
            unit.formation_offset.y,
            unit.formation_offset.z,
        );
    }
}

fn hash_projectiles(checksum: &mut SyncChecksum, projectiles: &EntityManager<Projectile>) {
    checksum.hash_u32(u32::try_from(projectiles.len()).unwrap_or(u32::MAX));
    for (_, projectile) in projectiles.iter() {
        hash_base_entity(checksum, &projectile.base);
        projectile.hash_state(checksum);
    }
}

fn hash_squads(checksum: &mut SyncChecksum, squads: &EntityManager<Squad>) {
    checksum.hash_u32(u32::try_from(squads.len()).unwrap_or(u32::MAX));
    for (_, squad) in squads.iter() {
        hash_base_entity(checksum, &squad.base);
        checksum.hash_u32(squad.state as u32);
        checksum.hash_u32(squad.archetype as u32);
        checksum.hash_u32(squad.formation as u32);
        checksum.hash_f32(squad.speed);
        checksum.hash_f32(squad.acceleration);
        checksum.hash_f32(squad.turn_rate_degrees);
        checksum.hash_i32(squad.proto_squad_id);
        checksum.hash_u32(u32::try_from(squad.proto_squad_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(squad.proto_squad_name.as_bytes());
        checksum.hash_f32(squad.turn_radius);
        checksum.hash_f32(squad.min_turn_radius);
        checksum.hash_f32(squad.max_turn_radius);
        hash_optional_vec3(checksum, squad.move_target);
        hash_optional_entity_id(checksum, squad.attack_target);
        checksum.hash_f32(squad.attack_range);
        checksum.hash_u32(squad.mode as u32);
        checksum.hash_u32(squad.attack_ability_id.map_or(u32::MAX, u32::from));
        squad.recovery.hash_state(checksum);
        squad.shields.hash_state(checksum);
        squad.hash_ability_execution(checksum);
        checksum.hash_u32(u32::try_from(squad.unit_ids.len()).unwrap_or(u32::MAX));
        for &unit_id in &squad.unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
    }
}

fn hash_bases(checksum: &mut SyncChecksum, bases: &BTreeMap<BaseId, Base>) {
    checksum.hash_u32(u32::try_from(bases.len()).unwrap_or(u32::MAX));
    for (id, base) in bases {
        checksum.hash_u32(u32::from(id.as_u16()));
        checksum.hash_u32(u32::from(base.player_id));
        checksum.hash_u32(base.anchor_building_id.as_u32());
        checksum.hash_vec3(base.position.x, base.position.y, base.position.z);
        checksum.hash_u32(u32::try_from(base.building_count()).unwrap_or(u32::MAX));
        for building_id in base.buildings() {
            checksum.hash_u32(building_id.as_u32());
        }
    }
}

fn hash_base_entity(checksum: &mut SyncChecksum, entity: &BaseEntity) {
    checksum.hash_u32(entity.id.as_u32());
    checksum.hash_u32(u32::from(entity.player_id));
    checksum.hash_vec3(entity.position.x, entity.position.y, entity.position.z);
    checksum.hash_vec3(entity.forward.x, entity.forward.y, entity.forward.z);
    checksum.hash_vec3(entity.velocity.x, entity.velocity.y, entity.velocity.z);
    checksum.hash_u32(u32::from(entity.alive));
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_vec3(value.x, value.y, value.z);
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_optional_entity_id(checksum: &mut SyncChecksum, value: Option<EntityId>) {
    checksum.hash_u32(value.map_or(EntityId::INVALID.as_u32(), EntityId::as_u32));
}

fn hash_optional_base_id(checksum: &mut SyncChecksum, value: Option<BaseId>) {
    checksum.hash_u32(value.map_or(u32::MAX, |id| u32::from(id.as_u16())));
}
