//! Shared retail-style entity kill and immediate-destroy operations.

use super::{World, design_lines, events, presentation, scoring, team};
use crate::entity_id::{EntityClass, EntityId};

impl World {
    /// Reset the world to initial state.
    pub fn reset(&mut self) {
        self.players.clear();
        self.team_relations = team::neutral_team_relations();
        self.coop = false;
        self.fog_of_war_enabled = true;
        self.black_map_cleared = false;
        self.scenario_score = scoring::ScenarioScoreState::default();
        self.design_lines = design_lines::DesignLineState::default();
        self.objectives.clear();
        self.terrain_bounds = None;
        self.playable_bounds = None;
        self.config_symbols = super::game_settings::default_config_symbols();
        self.veterancy = super::game_settings::VeterancySetting::Enabled;
        self.general_events = events::GeneralEventState::default();
        self.presentation = events::PresentationState::default();
        self.presentation_control = presentation::PresentationControlState::default();
        self.custom_commands.clear();
        self.next_custom_command_id = 0;
        self.custom_command_executions.clear();
        self.power_manager.reset();
        self.game_timers = super::timers::GameTimerState::default();
        self.game_time_ms = 0;
        self.construction_damage_multiplier = 1.0;
        self.prototype_object_types.clear();
        self.prototype_squads.clear();
        self.prototype_shield_coverages.clear();
        self.prototype_ground_vehicle_physics.clear();
        self.objects.clear();
        self.units.clear();
        self.squads.clear();
        self.projectiles.clear();
        self.bases.clear();
        self.next_base_id = 0;
        self.trigger_engine = crate::trigger::TriggerEngine::new();
        self.pending_building_command_events.clear();
    }

    /// Kill an entity, optionally removing it immediately.
    ///
    /// A regular kill leaves dead state observable until the next entity
    /// update. Immediate destruction invalidates the entity ID before this
    /// method returns, matching the distinction trigger DBIDs 37 and 38 make.
    pub fn kill_entity(&mut self, entity_id: EntityId, immediate: bool) -> bool {
        match entity_id.class() {
            Some(EntityClass::Object) => self.remove_object(entity_id).is_some(),
            Some(EntityClass::Unit) => self.kill_unit(entity_id, immediate),
            Some(EntityClass::Squad) => self.kill_squad(entity_id, immediate),
            Some(EntityClass::Projectile) => self.remove_projectile(entity_id).is_some(),
            _ => false,
        }
    }

    /// Kill or immediately destroy one mobile unit or building.
    pub fn kill_unit(&mut self, unit_id: EntityId, immediate: bool) -> bool {
        if immediate {
            return self.remove_unit(unit_id).is_some();
        }
        if self
            .get_unit(unit_id)
            .is_some_and(crate::entities::Unit::is_static_death_replacement)
        {
            return false;
        }
        if self
            .get_unit(unit_id)
            .is_some_and(crate::entities::Unit::has_hero_revival)
        {
            let downed = self
                .get_unit_mut(unit_id)
                .is_some_and(crate::entities::Unit::down_hero);
            if downed {
                self.cancel_incapacitated_squad_orders(unit_id);
            }
            return downed;
        }
        if self.get_unit(unit_id).is_none() {
            return false;
        }
        self.remove_owned_attachments(unit_id);
        let Some(unit) = self.get_unit_mut(unit_id) else {
            return false;
        };
        unit.kill();
        true
    }

    /// Kill or immediately destroy a squad and each of its member units.
    pub fn kill_squad(&mut self, squad_id: EntityId, immediate: bool) -> bool {
        let Some(member_ids) = self.get_squad(squad_id).map(|squad| squad.unit_ids.clone()) else {
            return false;
        };
        if immediate {
            for unit_id in member_ids {
                let _removed = self.remove_unit(unit_id);
            }
            let _removed = self.remove_squad(squad_id);
            return true;
        }
        if member_ids.iter().any(|unit_id| {
            self.get_unit(*unit_id)
                .is_some_and(crate::entities::Unit::has_hero_revival)
        }) {
            if let Some(squad) = self.get_squad_mut(squad_id) {
                squad.remove_all_orders();
            }
            for unit_id in member_ids {
                if let Some(unit) = self.get_unit_mut(unit_id) {
                    let _downed = unit.down_hero();
                }
            }
            return true;
        }
        if let Some(squad) = self.get_squad_mut(squad_id) {
            squad.kill();
        }
        for unit_id in member_ids {
            let _killed = self.kill_unit(unit_id, false);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entity;

    #[test]
    fn regular_kill_is_observable_until_update_and_destroy_is_immediate() {
        let mut world = World::new();
        world.init_players(1);
        let killed = world.create_unit(1);
        let destroyed = world.create_unit(1);

        assert!(world.kill_entity(killed, false));
        assert!(world.get_unit(killed).is_some_and(|unit| !unit.is_alive()));
        assert!(world.kill_entity(destroyed, true));
        assert!(world.get_unit(destroyed).is_none());

        world.update_entities(0.05);
        assert!(world.get_unit(killed).is_none());
    }

    #[test]
    fn squad_lifecycle_cascades_to_members() {
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad(1);
        let unit_id = world.create_unit(1);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));

        assert!(world.kill_squad(squad_id, false));
        assert!(
            world
                .get_squad(squad_id)
                .is_some_and(|squad| !squad.is_alive())
        );
        assert!(world.get_unit(unit_id).is_some_and(|unit| !unit.is_alive()));

        world.update_entities(0.05);
        assert!(world.get_squad(squad_id).is_none());
        assert!(world.get_unit(unit_id).is_none());
    }
}
