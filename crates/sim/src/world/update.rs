//! Fixed-substep orchestration for authoritative world systems.
//!
//! One world update follows `BWorld_update` (`0x1405C5690`): the queued vehicle terrain
//! penetrations, then the entity updates (the physics readback included), then the Havok
//! substeps of the physics world with the full update length.

use super::World;
use crate::entities::Unit;
use crate::entities::squads::formation_offset_to_world;
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use std::collections::BTreeMap;

/// The longest entity update step. A longer update runs several entity steps.
const MAX_ENTITY_STEP_SECONDS: f32 = 0.05;
/// The most entity steps of one update.
const MAX_ENTITY_STEPS: f32 = 1_000.0;

/// Splits an update into equal entity steps of at most [`MAX_ENTITY_STEP_SECONDS`]. Returns
/// `None` for a time that is not positive and finite.
fn entity_steps(dt: f32) -> Option<(u32, f32)> {
    if !(dt.is_finite() && dt > 0.0) {
        return None;
    }
    let count = (dt / MAX_ENTITY_STEP_SECONDS)
        .ceil()
        .clamp(1.0, MAX_ENTITY_STEPS)
        .to_u32()
        .unwrap_or(1);
    Some((count, dt / count.to_f32().unwrap_or(1.0)))
}

impl World {
    /// Update all entities for one tick.
    pub fn update_entities(&mut self, dt: f32) {
        self.update_entities_internal(dt, None, None);
    }

    /// Update entities plus tactic-backed gameplay for one tick.
    pub fn update_entities_with_gameplay(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.update_entities_internal(dt, None, Some(gameplay));
    }

    /// Update all database-backed systems without a tactic gameplay catalog.
    pub fn update_entities_with_database(&mut self, dt: f32, database: &Database) {
        self.update_entities_internal(dt, Some(database), None);
    }

    /// Update every database-backed gameplay system for one authoritative tick.
    pub fn update_entities_with_database_and_gameplay(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        self.update_entities_internal(dt, Some(database), Some(gameplay));
    }

    fn update_entities_internal(
        &mut self,
        dt: f32,
        database: Option<&Database>,
        gameplay: Option<&GameplayCatalog>,
    ) {
        self.update_game_timers();
        self.update_camera_shakes();
        self.update_rumbles();
        self.update_screen_fade();
        let Some((step_count, step_duration)) = entity_steps(dt) else {
            return;
        };
        self.create_pending_physics_objects();
        self.resolve_vehicle_terrain_penetrations();
        self.update_power_visual_lifetimes();
        if let Some(gameplay) = gameplay {
            self.grant_secondary_turret_scan_tokens(gameplay);
        }
        for _ in 0..step_count {
            self.update_entity_substep(step_duration, database, gameplay);
        }
        self.update_revealers(dt);
        self.update_line_of_sight();
        self.update_idle_actions(dt);
        if let (Some(database), Some(gameplay)) = (database, gameplay) {
            self.update_auto_repairs(database, gameplay);
        }
        self.read_back_object_physics();
        self.update_object_states();
        // `BWorld::updatePostAsync` (2008 `world.cpp:3331-3368`): the power manager runs after
        // the units (and so after their physics readback) and right before the physics step.
        if let Some(database) = database {
            self.update_active_powers(dt, database, gameplay);
            self.resolve_pending_rage_kills(Some(database));
        }
        self.step_physics(dt);
    }

    fn update_entity_substep(
        &mut self,
        dt: f32,
        database: Option<&Database>,
        gameplay: Option<&GameplayCatalog>,
    ) {
        self.update_cryo(dt, gameplay);
        self.update_weapon_effects(dt, database, gameplay);
        let air_speed_limits = gameplay.map_or_else(BTreeMap::new, |gameplay| {
            self.prepare_air_avoidance(dt, gameplay)
        });
        if let (Some(database), Some(gameplay)) = (database, gameplay) {
            self.update_air_traffic_controls(database, gameplay);
            self.update_mines(database, gameplay);
            self.update_ambient_life_spawners(dt, database, gameplay);
            self.update_cloaks(dt, database, gameplay);
            self.update_persistent_squad_spawns(dt, database, gameplay);
            self.update_spirit_bonds(database, gameplay);
        }
        if let (Some(database), Some(gameplay)) = (database, gameplay) {
            self.update_infections(dt, database, gameplay);
            self.update_captures(dt, database, gameplay);
            self.update_repair_other(dt, database, gameplay);
            self.update_heals(dt, database, gameplay);
        }
        if let Some(gameplay) = gameplay {
            self.update_change_modes(dt, gameplay);
            self.update_gathering(dt, gameplay);
            self.update_ambient_life(dt, gameplay);
            self.update_wanders(dt, gameplay);
            self.update_bombs(gameplay);
            self.update_detonations(dt, gameplay);
            self.update_revivals(dt, gameplay);
            self.update_charges(dt, gameplay);
            self.update_attack_move_orders(gameplay);
            self.prepare_squad_carpet_bombs();
            self.update_move_air_tactics(dt, gameplay);
            self.update_squad_carpet_bomb_attacks(dt, gameplay);
            self.update_smart_targets(gameplay);
            self.update_combat_orders(dt, gameplay);
            self.update_protection(dt, gameplay);
            self.update_shields(dt, gameplay);
            self.update_jumps(dt, gameplay);
        }
        self.update_squad_pulls(dt);
        let hardpoint_yaw_targets = self.capture_hardpoint_yaw_targets();
        self.update_transport_fly_ins(dt);
        let units = &self.units;
        for (_, squad) in self.squads.iter_mut() {
            squad.update_recovery(dt);
            let keeps_height = squad
                .unit_ids
                .iter()
                .any(|&unit_id| units.get(unit_id).is_some_and(Unit::has_hover_action));
            squad.update_with_speed_limit(
                dt,
                air_speed_limits.get(&squad.base.id).copied(),
                keeps_height,
            );
        }
        self.update_physics_vehicle_squads();
        // `BSquad::update` (`0x140397DE0`) calls `BSquad_updateTurning` (`0x1403A70E0`) after
        // the orders and the move of each squad (2008 `squad.cpp:1531-1540`).
        let squad_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_squad_turn_radius(squad_id, dt);
        }
        if let Some(gameplay) = gameplay {
            self.advance_air_avoidance(dt, gameplay);
        }
        self.update_move_air(dt);
        self.prepare_squad_ground_moves();
        self.read_back_physics();
        self.update_death_physics(dt);
        self.finish_rocket_deaths(database, gameplay);
        self.finish_unit_throws();
        for (_, unit) in self.units.iter_mut() {
            unit.update(dt);
        }
        self.tie_units_to_ground(false);
        self.resolve_collisions_and_attacks(gameplay);
        sync_formation_members(self);
        self.tie_units_to_ground(true);
        self.finalize_air_avoidance_positions();
        self.update_trained_air_births(dt);
        self.sync_associated_socket_transforms();
        self.synchronize_attachments();
        self.update_garrisons(gameplay);
        self.restore_hardpoint_yaw_targets(&hardpoint_yaw_targets);
        self.update_projectiles(dt, database, gameplay);
        self.resolve_pending_rage_kills(database);
        if let Some(database) = database {
            self.resolve_dead_unit_death_replacements(database, gameplay);
        }
        if let Some(gameplay) = gameplay {
            self.resolve_dead_unit_detonations(gameplay);
        }
        self.cleanup_physics_detonate_replacements();
        self.cleanup_dead_physics_replacements();
        if let Some(database) = database {
            self.resolve_dead_unit_death_spawns(database, gameplay);
        }
        // A dead physics replacement is a corpse: `cleanup_dead_physics_replacements` removes
        // it once its physics action waits.
        let dead_units = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (!unit.is_alive()
                    && !unit.is_static_death_replacement()
                    && !unit.is_physics_replacement())
                .then_some(id)
            })
            .collect::<Vec<_>>();
        for id in dead_units {
            let _removed = self.remove_unit(id);
        }
        let dead_squads = self
            .squads
            .iter()
            .filter_map(|(id, squad)| (!squad.is_alive()).then_some(id))
            .collect::<Vec<_>>();
        for id in dead_squads {
            let _removed = self.remove_squad(id);
        }
    }
}

/// Puts each formation member that no other system moves at its formation slot around the
/// squad, with the squad heading and velocity. A unit with a dynamic body takes its transform
/// from the readback instead.
fn sync_formation_members(world: &mut World) {
    for (squad_id, squad) in world.squads.iter() {
        if squad.is_being_pulled() || squad.is_jumping() {
            continue;
        }
        let position = squad.base.position;
        let forward = squad.base.forward;
        let velocity = squad.base.velocity;
        for &unit_id in &squad.unit_ids {
            let Some(unit) = world.units.get_mut(unit_id) else {
                continue;
            };
            if unit.squad_id != Some(squad_id)
                || unit.is_physics_driven()
                || unit.uses_move_air()
                || unit.ground_move_owns_squad_transform()
                || unit.is_jumping()
            {
                continue;
            }
            unit.base.position =
                position + formation_offset_to_world(forward, unit.formation_offset);
            unit.base.forward = forward;
            unit.base.velocity = velocity;
        }
    }
}
