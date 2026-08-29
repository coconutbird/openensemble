//! Fixed-substep orchestration for authoritative world systems.

use super::World;
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::physics::{prepare_squad_movement, substeps, sync_squad_members};
use pipeline::database::hw1::Database;

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
        let Some((step_count, step_duration)) = substeps(dt) else {
            return;
        };
        self.update_power_visual_lifetimes();
        if let Some(database) = database {
            self.update_active_powers(dt, database, gameplay);
            self.resolve_pending_rage_kills(Some(database));
        }
        for _ in 0..step_count {
            self.update_entity_substep(step_duration, database, gameplay);
        }
        self.update_revealers(dt);
        self.update_idle_actions(dt);
        self.update_object_states();
    }

    fn update_entity_substep(
        &mut self,
        dt: f32,
        database: Option<&Database>,
        gameplay: Option<&GameplayCatalog>,
    ) {
        self.update_cryo(dt);
        if let (Some(database), Some(gameplay)) = (database, gameplay) {
            self.update_mines(database, gameplay);
        }
        if let Some(gameplay) = gameplay {
            self.update_detonations(dt, gameplay);
            self.update_revivals(dt, gameplay);
            self.update_attack_move_orders(gameplay);
            self.update_combat_orders(dt, gameplay);
            self.update_protection(dt, gameplay);
            self.update_shields(dt, gameplay);
        }
        self.update_transport_fly_ins(dt);
        let physics_anchors = prepare_squad_movement(&self.squads, &mut self.units);
        for (_, squad) in self.squads.iter_mut() {
            squad.update_recovery(dt);
            if !physics_anchors.contains_key(&squad.base.id) {
                squad.update(dt);
            }
        }
        for (_, unit) in self.units.iter_mut() {
            unit.update(dt);
        }
        self.resolve_collisions_and_attacks(gameplay);
        sync_squad_members(&mut self.squads, &mut self.units, &physics_anchors);
        self.sync_associated_socket_transforms();
        self.synchronize_attachments();
        self.update_garrisons(gameplay);
        self.update_projectiles(dt, gameplay);
        self.resolve_pending_rage_kills(database);
        if let Some(database) = database {
            self.resolve_dead_unit_death_replacements(database, gameplay);
        }
        if let Some(gameplay) = gameplay {
            self.resolve_dead_unit_detonations(gameplay);
        }
        self.cleanup_physics_detonate_replacements();
        if let Some(database) = database {
            self.resolve_dead_unit_death_spawns(database, gameplay);
        }
        let dead_units = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (!unit.is_alive() && !unit.is_static_death_replacement()).then_some(id)
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
