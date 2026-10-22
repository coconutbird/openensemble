//! World ownership and update boundary for deterministic trigger scripts.

use super::World;
use crate::EntityId;
use crate::entities::units::TriggerCommandStateRef;
use crate::gameplay::GameplayCatalog;
use crate::sync::SyncChecksum;
use crate::trigger::{TriggerEngine, TriggerUpdate, TriggerValue};
use pipeline::database::hw1::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BuildingCommandEvent {
    pub(super) state_ref: TriggerCommandStateRef,
    pub(super) trained_squad: Option<EntityId>,
    pub(super) finish: bool,
}

/// The trigger engine and the world events that wait for it.
#[derive(Debug, Default)]
pub(super) struct WorldTriggers {
    /// Authoritative scenario and gameplay trigger scripts.
    engine: TriggerEngine,
    /// Trigger-state writes deferred while their script is being evaluated.
    pending_building_command_events: Vec<BuildingCommandEvent>,
    /// Removed units and squads that the running engine did not see yet.
    destroyed_entities: Vec<EntityId>,
    /// Whether `engine` is out of the world for its update.
    updating: bool,
}

impl WorldTriggers {
    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            u32::try_from(self.pending_building_command_events.len()).unwrap_or(u32::MAX),
        );
        for event in &self.pending_building_command_events {
            checksum.hash_u32(event.state_ref.script_id);
            checksum.hash_u32(event.state_ref.variable_id);
            checksum.hash_u32(event.trained_squad.map_or(u32::MAX, EntityId::as_u32));
            checksum.hash_u32(u32::from(event.finish));
        }
        checksum.hash_u32(u32::try_from(self.destroyed_entities.len()).unwrap_or(u32::MAX));
        for entity_id in &self.destroyed_entities {
            checksum.hash_u32(entity_id.as_u32());
        }
        checksum.hash_u32(u32::from(self.updating));
        self.engine.hash_state(checksum);
    }
}

impl World {
    /// Read the authoritative trigger engine.
    #[must_use]
    pub fn trigger_engine(&self) -> &TriggerEngine {
        &self.triggers.engine
    }

    /// Mutably access the authoritative trigger engine during setup or tooling.
    pub fn trigger_engine_mut(&mut self) -> &mut TriggerEngine {
        &mut self.triggers.engine
    }

    /// Drop the references that hint callouts and trigger lists keep to a removed unit or squad.
    ///
    /// Retail `BObjectManager_destroyEntity` calls `BTriggerManager_invalidateEntityID`, which
    /// reaches only the scripts that are active at that time. During a trigger update the
    /// engine is out of the world, so the ID waits until the effect that removed it ends.
    pub(super) fn release_removed_entity(&mut self, id: EntityId) {
        self.remove_hint_callouts_for_entity(id);
        if self.triggers.updating {
            self.triggers.destroyed_entities.push(id);
        } else {
            self.triggers.engine.invalidate_entity(id);
        }
    }

    /// Take the removed units and squads that the trigger engine did not see yet.
    pub(crate) fn take_destroyed_trigger_entities(&mut self) -> Vec<EntityId> {
        std::mem::take(&mut self.triggers.destroyed_entities)
    }

    /// Evaluate scenario scripts after entity updates for the current tick.
    pub fn update_triggers(&mut self) -> TriggerUpdate {
        self.run_trigger_update(TriggerEngine::update)
    }

    /// Evaluate scenario scripts with the same layered database as gameplay.
    pub fn update_triggers_with_database(&mut self, database: &Database) -> TriggerUpdate {
        self.run_trigger_update(|engine, world| engine.update_with_database(world, database))
    }

    /// Evaluate scripts with both scenario-layered data and resolved tactics.
    pub fn update_triggers_with_gameplay(
        &mut self,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> TriggerUpdate {
        self.run_trigger_update(|engine, world| {
            engine.update_with_gameplay(world, database, gameplay)
        })
    }

    /// Run one trigger-manager update. Like `BWorld_update`, there is no trigger update after
    /// the game ended.
    fn run_trigger_update(
        &mut self,
        update: impl FnOnce(&mut TriggerEngine, &mut Self) -> TriggerUpdate,
    ) -> TriggerUpdate {
        if self.is_game_over() {
            return TriggerUpdate::default();
        }
        let mut engine = std::mem::take(&mut self.triggers.engine);
        self.triggers.updating = true;
        let result = update(&mut engine, self);
        self.triggers.updating = false;
        self.triggers.engine = engine;
        for entity_id in std::mem::take(&mut self.triggers.destroyed_entities) {
            self.triggers.engine.invalidate_entity(entity_id);
        }
        self.flush_building_command_events();
        result
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
            self.triggers.pending_building_command_events.push(event);
        }
    }

    fn flush_building_command_events(&mut self) {
        let events = std::mem::take(&mut self.triggers.pending_building_command_events);
        for event in events {
            let _applied = self.apply_building_command_event(event);
        }
    }

    fn apply_building_command_event(&mut self, event: BuildingCommandEvent) -> bool {
        let Some(script) = self
            .triggers
            .engine
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
        true
    }
}
