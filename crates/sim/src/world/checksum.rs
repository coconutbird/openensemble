//! Deterministic hashing of authoritative world state.

use super::World;
use super::team::hash_team_relations;
use crate::entities::{Base, BaseEntity, BaseId, Object, ObjectKind, Projectile, Squad, Unit};
use crate::entity::EntityManager;
use crate::entity_id::EntityId;
use crate::player::{Player, PopulationCost};
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
        checksum.hash_u32(u32::from(self.coop));
        checksum.hash_u32(u32::from(self.fog_of_war_enabled));
        checksum.hash_u32(u32::from(self.black_map_cleared));
        self.scenario_score.hash_state(&mut checksum);
        self.design_lines.hash_state(&mut checksum);
        self.hash_objectives(&mut checksum);
        hash_optional_bounds(&mut checksum, self.terrain_bounds);
        hash_optional_bounds(&mut checksum, self.playable_bounds);
        if let Some(terrain) = &self.terrain_simulation {
            checksum.hash_u32(1);
            terrain.hash_state(&mut checksum);
        } else {
            checksum.hash_u32(0);
        }
        hash_config_symbols(&mut checksum, self.config_symbols());
        checksum.hash_u32(u32::from(self.scenario_allows_veterancy()));
        checksum.hash_u32(u32::from(self.veterancy_enabled()));
        self.general_events.hash_state(&mut checksum);
        self.presentation.hash_state(&mut checksum);
        self.presentation_control.hash_state(&mut checksum);
        self.game_timers.hash_state(&mut checksum);
        self.hash_custom_commands(&mut checksum);
        self.power_manager.hash_state(&mut checksum);
        checksum.hash_f32(self.construction_damage_multiplier);
        self.hash_prototype_catalogs(&mut checksum);
        hash_players(&mut checksum, &self.players);
        hash_team_relations(&mut checksum, &self.team_relations);
        hash_objects(&mut checksum, &self.objects);
        hash_units(&mut checksum, &self.units);
        hash_squads(&mut checksum, &self.squads);
        hash_projectiles(&mut checksum, &self.projectiles);
        hash_bases(&mut checksum, &self.bases);
        checksum.hash_u32(
            u32::try_from(self.pending_building_command_events.len()).unwrap_or(u32::MAX),
        );
        for event in &self.pending_building_command_events {
            checksum.hash_u32(event.state_ref.script_id);
            checksum.hash_u32(event.state_ref.variable_id);
            hash_optional_entity_id(&mut checksum, event.trained_squad);
            checksum.hash_u32(u32::from(event.finish));
        }
        self.trigger_engine.hash_state(&mut checksum);
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
        checksum.hash_u32(self.sim_rng.seed());
        checksum.value()
    }
}

fn hash_optional_bounds(checksum: &mut SyncChecksum, bounds: Option<super::WorldBounds>) {
    if let Some(bounds) = bounds {
        checksum.hash_u32(1);
        bounds.hash_state(checksum);
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_objects(checksum: &mut SyncChecksum, objects: &EntityManager<Object>) {
    checksum.hash_u32(u32::try_from(objects.len()).unwrap_or(u32::MAX));
    for (_, object) in objects.iter() {
        hash_base_entity(checksum, &object.base);
        object.object_state.hash_state(checksum);
        checksum.hash_i32(object.proto_object_id);
        checksum.hash_u32(u32::try_from(object.proto_object_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(object.proto_object_name.as_bytes());
        hash_optional_entity_id(checksum, object.built_by);
        hash_optional_vec3(checksum, object.visual_secondary_position());
        object.ambient_life_spawner.hash_state(checksum);
        match &object.kind {
            ObjectKind::Visual => checksum.hash_u32(0),
            ObjectKind::Icon(icon) => {
                checksum.hash_u32(2);
                if let Some(color) = icon.color_override() {
                    checksum.hash_u32(1);
                    for channel in color {
                        checksum.hash_u32(u32::from(channel));
                    }
                } else {
                    checksum.hash_u32(0);
                }
                checksum.hash_u32(u32::from(icon.visible_for_owner_only()));
                checksum.hash_u32(u32::from(icon.visible_for_team_only()));
                checksum.hash_u32(u32::from(icon.visible_to_all()));
                checksum.hash_u32(u32::from(icon.always_visible_on_minimap()));
            }
            ObjectKind::Revealer(revealer) => {
                checksum.hash_u32(1);
                checksum.hash_u32(u32::from(revealer.team_id()));
                checksum.hash_f32(revealer.line_of_sight_scalar());
                checksum.hash_f32(revealer.line_of_sight());
                checksum.hash_f32(revealer.reveal_fraction());
                checksum.hash_u32(revealer.lifespan_expiration_ms().unwrap_or(u32::MAX));
            }
        }
    }
}

fn hash_players(checksum: &mut SyncChecksum, players: &[Player]) {
    checksum.hash_u32(u32::try_from(players.len()).unwrap_or(u32::MAX));
    for player in players {
        checksum.hash_u32(u32::from(player.id));
        checksum.hash_u32(u32::from(player.team_id));
        checksum.hash_u32(player.coop_player_id().map_or(u32::MAX, u32::from));
        checksum.hash_i32(player.civ_id);
        checksum.hash_i32(player.leader_id);
        checksum.hash_u32(player.state as u32);
        checksum.hash_u32(player.player_type as u32);
        checksum.hash_f32(player.difficulty);
        for &amount in &player.resources.amounts {
            checksum.hash_f32(amount);
        }
        for &amount in &player.total_resources.amounts {
            checksum.hash_f32(amount);
        }
        for amount in player.resource_trickle_rate().amounts {
            checksum.hash_f32(amount);
        }
        checksum.hash_u32(u32::try_from(player.rate_slot_count()).unwrap_or(u32::MAX));
        for (amount, multiplier) in player.rate_components() {
            checksum.hash_f32(amount);
            checksum.hash_f32(multiplier);
        }
        checksum.hash_u32(u32::try_from(player.population.len()).unwrap_or(u32::MAX));
        for population in &player.population {
            checksum.hash_f32(population.count);
            checksum.hash_f32(population.max);
            checksum.hash_f32(population.cap);
            checksum.hash_f32(population.future);
        }
        player.technologies.hash_state(checksum);
        player.hash_rally_point_state(checksum);
        player.hash_forbid_state(checksum);
        player.hash_one_time_spawn_state(checksum);
        player.hash_power_state(checksum);
        player.research.hash_state(checksum);
    }
}

fn hash_config_symbols<'a>(checksum: &mut SyncChecksum, symbols: impl Iterator<Item = &'a str>) {
    let symbols = symbols.collect::<Vec<_>>();
    checksum.hash_u32(u32::try_from(symbols.len()).unwrap_or(u32::MAX));
    for symbol in symbols {
        checksum.hash_u32(u32::try_from(symbol.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(symbol.as_bytes());
    }
}

fn hash_units(checksum: &mut SyncChecksum, units: &EntityManager<Unit>) {
    checksum.hash_u32(u32::try_from(units.len()).unwrap_or(u32::MAX));
    for (_, unit) in units.iter() {
        hash_base_entity(checksum, &unit.base);
        unit.object_state.hash_state(checksum);
        checksum.hash_u32(unit.kind as u32);
        checksum.hash_u32(unit.archetype as u32);
        checksum.hash_u32(unit.state as u32);
        unit.idle.hash_state(checksum);
        hash_unit_prototype(checksum, unit);
        checksum.hash_u32(u32::from(unit.flying));
        unit.hash_flight_state(checksum);
        checksum.hash_u32(u32::from(unit.auto_attackable_setting()));
        checksum.hash_u32(u32::from(unit.is_invulnerable()));
        checksum.hash_u32(u32::from(unit.is_being_boarded()));
        checksum.hash_u32(u32::from(unit.is_jump_pull_untargetable()));
        checksum.hash_u32(u32::from(unit.is_external_shield()));
        checksum.hash_u32(u32::from(unit.is_reverse_moving()));
        checksum.hash_f32(unit.hitpoints);
        checksum.hash_f32(unit.max_hitpoints);
        unit.shields.hash_state(checksum);
        unit.hash_revival_state(checksum);
        unit.hash_death_state(checksum);
        checksum.hash_f32(unit.damage_multiplier);
        checksum.hash_f32(unit.damage_taken_multiplier);
        checksum.hash_f32(unit.join_damage_multiplier());
        checksum.hash_f32(unit.join_damage_taken_multiplier());
        checksum.hash_f32(unit.spirit_bond_damage_multiplier());
        unit.hash_cryo_state(checksum);
        checksum.hash_f32(unit.accuracy_scalar);
        checksum.hash_f32(unit.dodge_scalar);
        checksum.hash_f32(unit.work_rate_scalar);
        checksum.hash_f32(unit.line_of_sight_scalar);
        checksum.hash_f32(unit.velocity_scalar);
        checksum.hash_f32(unit.weapon_range_scalar);
        checksum.hash_f32(unit.speed);
        checksum.hash_f32(unit.reverse_speed());
        checksum.hash_f32(unit.acceleration);
        checksum.hash_f32(unit.turn_rate_degrees);
        checksum.hash_u32(u32::from(unit.obstructs_air()));
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
        unit.hash_tactic_state(checksum);
        hash_unit_action_state(checksum, unit);
        if let Some(tower_wall) = unit.tower_wall {
            checksum.hash_u32(1);
            tower_wall.hash_state(checksum);
        } else {
            checksum.hash_u32(0);
        }
        unit.production.hash_state(checksum);
        unit.hash_unique_technology_state(checksum);
        unit.hash_rally_point_state(checksum);
        checksum.hash_u32(u32::from(unit.built));
        unit.built_economy.hash_state(checksum);
        hash_unit_persistent_child_state(checksum, unit);
        hash_optional_entity_id(checksum, unit.built_by);
        hash_optional_entity_id(checksum, unit.build_socket_id);
        checksum.hash_u32(unit.build_socket_index.map_or(u32::MAX, u32::from));
        hash_optional_entity_id(checksum, unit.socket_plug_id);
        hash_optional_entity_id(checksum, unit.socket_parent_id);
        checksum.hash_u32(u32::try_from(unit.associated_socket_ids.len()).unwrap_or(u32::MAX));
        for &socket_id in &unit.associated_socket_ids {
            checksum.hash_u32(socket_id.as_u32());
        }
        hash_optional_entity_id(checksum, unit.associated_parking_lot_id);
        checksum.hash_vec3(
            unit.socket_local_offset.x,
            unit.socket_local_offset.y,
            unit.socket_local_offset.z,
        );
        checksum.hash_f32(unit.socket_local_yaw_degrees);
        hash_population_costs(checksum, &unit.population_costs);
        hash_population_costs(checksum, &unit.population_cap_additions);
        hash_optional_entity_id(checksum, unit.trained_by);
        checksum.hash_u32(unit.train_limit_bucket.map_or(u32::MAX, u32::from));
        hash_optional_entity_id(checksum, unit.squad_id);
        hash_optional_base_id(checksum, unit.base_id);
        checksum.hash_vec3(
            unit.formation_offset.x,
            unit.formation_offset.y,
            unit.formation_offset.z,
        );
    }
}

fn hash_unit_action_state(checksum: &mut SyncChecksum, unit: &Unit) {
    unit.garrison.hash_state(checksum);
    unit.hash_ground_move_state(checksum);
    unit.hash_jump_state(checksum);
    unit.combat.hash_state(checksum);
    unit.ammunition.hash_state(checksum);
    unit.hash_gather_state(checksum);
    unit.hash_capture_state(checksum);
    unit.hash_heal_state(checksum);
    unit.hash_infection_state(checksum);
    unit.hash_charge_state(checksum);
    unit.hash_air_avoidance_state(checksum);
    unit.collision_attack.hash_state(checksum);
    unit.hash_detonate_state(checksum);
    unit.hash_thrown_state(checksum);
    unit.hash_visual_state(checksum);
    unit.hash_physics_replacement_state(checksum);
    unit.hash_static_death_replacement_state(checksum);
}

fn hash_unit_persistent_child_state(checksum: &mut SyncChecksum, unit: &Unit) {
    unit.hash_authored_child_state(checksum);
    unit.hash_persistent_spawn_state(checksum);
    unit.hash_projectile_defense_state(checksum);
    unit.hash_child_damage_state(checksum);
}

fn hash_unit_prototype(checksum: &mut SyncChecksum, unit: &Unit) {
    checksum.hash_i32(unit.proto_object_id);
    checksum.hash_u32(u32::try_from(unit.proto_object_name.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(unit.proto_object_name.as_bytes());
    let logical_name = unit.logical_proto_object_name();
    checksum.hash_u32(u32::try_from(logical_name.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(logical_name.as_bytes());
    checksum.hash_u32(u32::try_from(unit.object_types.len()).unwrap_or(u32::MAX));
    for object_type in &unit.object_types {
        checksum.hash_u32(u32::try_from(object_type.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(object_type.as_bytes());
    }
}

fn hash_projectiles(checksum: &mut SyncChecksum, projectiles: &EntityManager<Projectile>) {
    checksum.hash_u32(u32::try_from(projectiles.len()).unwrap_or(u32::MAX));
    for (_, projectile) in projectiles.iter() {
        hash_base_entity(checksum, &projectile.base);
        projectile.object_state.hash_state(checksum);
        projectile.hash_state(checksum);
    }
}

fn hash_squads(checksum: &mut SyncChecksum, squads: &EntityManager<Squad>) {
    checksum.hash_u32(u32::try_from(squads.len()).unwrap_or(u32::MAX));
    for (_, squad) in squads.iter() {
        hash_base_entity(checksum, &squad.base);
        checksum.hash_u32(squad.state as u32);
        squad.idle.hash_state(checksum);
        checksum.hash_u32(squad.archetype as u32);
        checksum.hash_u32(squad.formation as u32);
        checksum.hash_f32(squad.speed);
        checksum.hash_f32(squad.acceleration);
        checksum.hash_f32(squad.turn_rate_degrees);
        checksum.hash_i32(squad.proto_squad_id);
        checksum.hash_u32(u32::try_from(squad.proto_squad_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(squad.proto_squad_name.as_bytes());
        checksum.hash_f32(squad.ammunition_maximum());
        checksum.hash_f32(squad.experience());
        checksum.hash_f32(squad.banked_experience());
        checksum.hash_i32(squad.veterancy_level());
        checksum.hash_f32(squad.turn_radius);
        checksum.hash_f32(squad.min_turn_radius);
        checksum.hash_f32(squad.max_turn_radius);
        checksum.hash_f32(squad.aggro_distance);
        checksum.hash_f32(squad.leash_distance);
        squad.hash_leash_state(checksum);
        checksum.hash_u32(u32::from(squad.is_reverse_moving()));
        hash_optional_vec3(checksum, squad.move_target);
        squad.hash_order_state(checksum);
        squad.hash_mines_state(checksum);
        squad.hash_detonate_state(checksum);
        squad.hash_carpet_bomb_state(checksum);
        hash_optional_entity_id(checksum, squad.attack_target);
        checksum.hash_f32(squad.attack_range);
        checksum.hash_u32(squad.mode as u32);
        checksum.hash_u32(squad.attack_ability_id.map_or(u32::MAX, u32::from));
        squad.recovery.hash_state(checksum);
        squad.shields.hash_state(checksum);
        squad.hash_cryo_state(checksum);
        squad.repair.hash_state(checksum);
        squad.rage.hash_state(checksum);
        checksum.hash_u32(squad.last_damaged_time);
        checksum.hash_u32(squad.last_attacked_time);
        squad.hash_join_state(checksum);
        squad.spirit_bond.hash_state(checksum);
        squad.cloak.hash_state(checksum);
        squad.wander.hash_state(checksum);
        squad.ambient_life.hash_state(checksum);
        squad.hash_gather_state(checksum);
        squad.hash_capture_state(checksum);
        squad.hash_repair_other_state(checksum);
        squad.hash_pull_state(checksum);
        squad.hash_jump_state(checksum);
        squad.hash_ability_execution(checksum);
        checksum.hash_u32(u32::try_from(squad.unit_ids.len()).unwrap_or(u32::MAX));
        for &unit_id in &squad.unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
        hash_population_costs(checksum, &squad.population_costs);
        hash_optional_entity_id(checksum, squad.trained_by);
        checksum.hash_u32(squad.train_limit_bucket.map_or(u32::MAX, u32::from));
        hash_optional_entity_id(checksum, squad.teleporter_destination);
        hash_optional_entity_id(checksum, squad.damage_proxy());
        checksum.hash_u32(u32::try_from(squad.associated_wall_towers().len()).unwrap_or(u32::MAX));
        for &target in squad.associated_wall_towers() {
            checksum.hash_u32(target.as_u32());
        }
        hash_optional_entity_id(checksum, squad.towing_partner);
        hash_optional_entity_id(checksum, squad.trailer_partner);
        squad.garrison.hash_state(checksum);
        checksum.hash_u32(u32::from(squad.transport_fly_in.is_some()));
        if let Some(action) = &squad.transport_fly_in {
            action.hash_state(checksum);
        }
        checksum.hash_u32(u32::from(squad.trained_air_birth.is_some()));
        if let Some(action) = &squad.trained_air_birth {
            action.hash_state(checksum);
        }
        checksum.hash_u32(u32::from(squad.power_transport.is_some()));
        if let Some(action) = &squad.power_transport {
            action.hash_state(checksum);
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
        hash_optional_entity_id(checksum, base.primary_plasma_shield_generator());
        hash_optional_entity_id(checksum, base.plasma_shield_squad());
        checksum.hash_f32(base.plasma_shield_rebuild_remaining());
        checksum.hash_f32(base.plasma_shield.attack_wait_remaining);
        checksum
            .hash_u32(u32::try_from(base.plasma_shield.subshield_squads.len()).unwrap_or(u32::MAX));
        for (&building_id, &shield_squad_id) in &base.plasma_shield.subshield_squads {
            checksum.hash_u32(building_id.as_u32());
            checksum.hash_u32(shield_squad_id.as_u32());
        }
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
    checksum.hash_u32(u32::from(entity.is_selectable()));
    checksum.hash_u32(u32::from(entity.is_mobile()));
    checksum.hash_u32(u32::from(entity.is_ever_mobile()));
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

fn hash_population_costs(checksum: &mut SyncChecksum, costs: &[PopulationCost]) {
    checksum.hash_u32(u32::try_from(costs.len()).unwrap_or(u32::MAX));
    for cost in costs {
        checksum.hash_u32(u32::try_from(cost.population_type).unwrap_or(u32::MAX));
        checksum.hash_f32(cost.amount);
    }
}
