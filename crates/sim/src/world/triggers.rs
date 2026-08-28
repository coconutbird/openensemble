//! World ownership and update boundary for deterministic trigger scripts.

use super::World;
use crate::EntityId;
use crate::entities::units::TriggerCommandStateRef;
use crate::gameplay::GameplayCatalog;
use crate::trigger::{TriggerEngine, TriggerUpdate, TriggerValue};
use pipeline::database::hw1::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BuildingCommandEvent {
    pub(super) state_ref: TriggerCommandStateRef,
    pub(super) trained_squad: Option<EntityId>,
    pub(super) finish: bool,
}

impl World {
    /// Read the authoritative trigger engine.
    #[must_use]
    pub fn trigger_engine(&self) -> &TriggerEngine {
        &self.trigger_engine
    }

    /// Mutably access the authoritative trigger engine during setup or tooling.
    pub fn trigger_engine_mut(&mut self) -> &mut TriggerEngine {
        &mut self.trigger_engine
    }

    /// Evaluate scenario scripts after entity updates for the current tick.
    pub fn update_triggers(&mut self) -> TriggerUpdate {
        let mut engine = std::mem::take(&mut self.trigger_engine);
        let update = engine.update(self);
        self.trigger_engine = engine;
        self.flush_building_command_events();
        update
    }

    /// Evaluate scenario scripts with the same layered database as gameplay.
    pub fn update_triggers_with_database(&mut self, database: &Database) -> TriggerUpdate {
        let mut engine = std::mem::take(&mut self.trigger_engine);
        let update = engine.update_with_database(self, database);
        self.trigger_engine = engine;
        self.flush_building_command_events();
        update
    }

    /// Evaluate scripts with both scenario-layered data and resolved tactics.
    pub fn update_triggers_with_gameplay(
        &mut self,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> TriggerUpdate {
        let mut engine = std::mem::take(&mut self.trigger_engine);
        let update = engine.update_with_gameplay(self, database, gameplay);
        self.trigger_engine = engine;
        self.flush_building_command_events();
        update
    }

    pub(crate) fn notify_building_command_task(
        &mut self,
        state_ref: TriggerCommandStateRef,
        trained_squad: Option<EntityId>,
    ) {
        let finish = !self.units.iter().any(|(_, unit)| {
            unit.production
                .tasks()
                .any(|task| task.trigger_state() == Some(state_ref))
        });
        let event = BuildingCommandEvent {
            state_ref,
            trained_squad,
            finish,
        };
        if !self.apply_building_command_event(event) {
            self.pending_building_command_events.push(event);
        }
    }

    fn flush_building_command_events(&mut self) {
        let events = std::mem::take(&mut self.pending_building_command_events);
        for event in events {
            let _applied = self.apply_building_command_event(event);
        }
    }

    fn apply_building_command_event(&mut self, event: BuildingCommandEvent) -> bool {
        let Some(script) = self
            .trigger_engine
            .get_script_mut(event.state_ref.script_id)
        else {
            return false;
        };
        let Some(variable) = script.get_variable_mut(event.state_ref.variable_id) else {
            return true;
        };
        let TriggerValue::BuildingCommandState(state) = &mut variable.value else {
            return true;
        };
        if let Some(squad_id) = event.trained_squad {
            state.record_trained_squad(squad_id);
        }
        if event.finish {
            state.finish();
        }
        variable.is_null = false;
        true
    }
}
