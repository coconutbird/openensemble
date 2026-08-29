//! Deterministic hashing of live trigger-script state.

use super::{Condition, Effect, EntityFilterSet, TriggerEngine, TriggerScript, TriggerValue};
use crate::sync::SyncChecksum;

impl TriggerEngine {
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.next_script_id);
        checksum.hash_u32(u32::try_from(self.scripts.len()).unwrap_or(u32::MAX));
        for (script_id, script) in &self.scripts {
            checksum.hash_u32(*script_id);
            hash_script(checksum, script);
        }
    }
}

fn hash_script(checksum: &mut SyncChecksum, script: &TriggerScript) {
    checksum.hash_u32(script.id);
    checksum.hash_u32(script.script_type as u32);
    checksum.hash_u32(u32::from(script.is_active));
    checksum.hash_u32(u32::from(script.is_paused));
    checksum.hash_u32(u32::from(script.marked_for_cleanup));
    hash_string(checksum, &script.name);
    checksum.hash_u32(u32::try_from(script.variables.len()).unwrap_or(u32::MAX));
    for (variable_id, variable) in &script.variables {
        checksum.hash_u32(*variable_id);
        checksum.hash_u32(variable.editor_id);
        checksum.hash_u32(variable.var_type as u32);
        checksum.hash_u32(u32::from(variable.is_null));
        checksum.hash_u32(u32::from(variable.is_input));
        checksum.hash_u32(u32::from(variable.is_output));
        hash_value(checksum, &variable.value);
    }
    checksum.hash_u32(u32::try_from(script.triggers.len()).unwrap_or(u32::MAX));
    for trigger in &script.triggers {
        checksum.hash_u32(trigger.id);
        checksum.hash_u32(trigger.editor_id);
        checksum.hash_i32(trigger.group_id);
        checksum.hash_u32(u32::from(trigger.start_active));
        checksum.hash_u32(u32::from(trigger.is_conditional));
        checksum.hash_u32(u32::from(trigger.is_active));
        checksum.hash_u32(trigger.activated_time);
        checksum.hash_u32(trigger.next_evaluate_time);
        checksum.hash_u32(trigger.evaluate_frequency);
        checksum.hash_u32(trigger.evaluate_count);
        checksum.hash_u32(trigger.evaluate_limit);
        checksum.hash_u32(match trigger.condition_mode {
            super::ConditionMode::All => 0,
            super::ConditionMode::Any => 1,
        });
        hash_conditions(checksum, &trigger.conditions);
        hash_effects(checksum, &trigger.effects_on_true);
        hash_effects(checksum, &trigger.effects_on_false);
    }
}

fn hash_conditions(checksum: &mut SyncChecksum, conditions: &[Condition]) {
    checksum.hash_u32(u32::try_from(conditions.len()).unwrap_or(u32::MAX));
    for condition in conditions {
        checksum.hash_i32(condition.id);
        checksum.hash_u32(u32::from(condition.raw_type));
        checksum.hash_u32(u32::from(condition.version));
        checksum.hash_u32(u32::from(condition.is_async));
        checksum.hash_u32(u32::from(condition.invert));
        hash_bindings(checksum, &condition.inputs);
        hash_bindings(checksum, &condition.outputs);
    }
}

fn hash_effects(checksum: &mut SyncChecksum, effects: &[Effect]) {
    checksum.hash_u32(u32::try_from(effects.len()).unwrap_or(u32::MAX));
    for effect in effects {
        checksum.hash_i32(effect.id);
        checksum.hash_u32(u32::from(effect.raw_type));
        checksum.hash_u32(u32::from(effect.version));
        hash_bindings(checksum, &effect.inputs);
        hash_bindings(checksum, &effect.outputs);
    }
}

fn hash_bindings(checksum: &mut SyncChecksum, bindings: &[super::VarBinding]) {
    checksum.hash_u32(u32::try_from(bindings.len()).unwrap_or(u32::MAX));
    for binding in bindings {
        checksum.hash_u32(u32::from(binding.signature_id));
        checksum.hash_u32(binding.variable_id);
    }
}

fn hash_value(checksum: &mut SyncChecksum, value: &TriggerValue) {
    match value {
        TriggerValue::Bool(value) => {
            checksum.hash_u32(0);
            checksum.hash_u32(u32::from(*value));
        }
        TriggerValue::Int(value) => hash_i32(checksum, 1, *value),
        TriggerValue::IntegerList(values) => hash_i32s(checksum, 40, values),
        TriggerValue::Float(value) => hash_f32(checksum, 2, *value),
        TriggerValue::String(value) => {
            checksum.hash_u32(3);
            hash_string(checksum, value);
        }
        TriggerValue::Entity(value) => hash_entity(checksum, 4, *value),
        TriggerValue::EntityList(values) => hash_entities(checksum, 5, values),
        TriggerValue::Unit(value) => hash_entity(checksum, 6, *value),
        TriggerValue::UnitList(values) => hash_entities(checksum, 7, values),
        TriggerValue::Squad(value) => hash_entity(checksum, 8, *value),
        TriggerValue::SquadList(values) => hash_entities(checksum, 9, values),
        TriggerValue::Object(value) => hash_entity(checksum, 10, *value),
        TriggerValue::ObjectList(values) => hash_entities(checksum, 11, values),
        TriggerValue::Location(value) => hash_vec3(checksum, 12, *value),
        TriggerValue::LocationList(values) => hash_vec3s(checksum, 13, values),
        TriggerValue::Vector(value) => hash_vec3(checksum, 14, *value),
        TriggerValue::VectorList(values) => hash_vec3s(checksum, 15, values),
        TriggerValue::Player(value) => hash_i32(checksum, 16, *value),
        TriggerValue::PlayerList(values) => hash_i32s(checksum, 17, values),
        TriggerValue::Team(value) => hash_i32(checksum, 18, *value),
        TriggerValue::TeamList(values) => hash_i32s(checksum, 19, values),
        TriggerValue::ProtoObject(value) => hash_i32(checksum, 20, *value),
        TriggerValue::ProtoObjectList(values) => hash_i32s(checksum, 21, values),
        TriggerValue::ProtoSquad(value) => hash_i32(checksum, 22, *value),
        TriggerValue::ProtoSquadList(values) => hash_i32s(checksum, 23, values),
        TriggerValue::Tech(value) => hash_i32(checksum, 24, *value),
        TriggerValue::TechList(values) => hash_i32s(checksum, 25, values),
        TriggerValue::ObjectType(value) => hash_tagged_string(checksum, 26, value),
        TriggerValue::ObjectTypeList(values) => hash_strings(checksum, 27, values),
        TriggerValue::DesignLine(value) => hash_i32(checksum, 47, *value),
        TriggerValue::DesignLineList(values) => hash_i32s(checksum, 48, values),
        TriggerValue::Cost(value) => {
            checksum.hash_u32(28);
            for amount in value.amounts() {
                checksum.hash_f32(amount);
            }
        }
        TriggerValue::Time(value) => {
            checksum.hash_u32(29);
            checksum.hash_u32(*value);
        }
        TriggerValue::Color(value) => {
            checksum.hash_u32(30);
            checksum.hash_u32(u32::from(value.r));
            checksum.hash_u32(u32::from(value.g));
            checksum.hash_u32(u32::from(value.b));
            checksum.hash_u32(u32::from(value.a));
        }
        TriggerValue::Objective(value) => hash_i32(checksum, 31, *value),
        TriggerValue::Trigger(value) => {
            checksum.hash_u32(32);
            checksum.hash_u32(*value);
        }
        TriggerValue::Iterator(value) => {
            checksum.hash_u32(33);
            checksum.hash_u32(value.source_list_id().unwrap_or(u32::MAX));
            hash_entities(checksum, 34, value.visited_units());
            hash_entities(checksum, 35, value.visited_squads());
            hash_i32s(checksum, 38, value.visited_players());
            hash_i32s(checksum, 39, value.visited_teams());
            hash_entities(checksum, 41, value.visited_objects());
            hash_vec3s(checksum, 42, value.visited_vectors());
        }
        TriggerValue::EntityFilterSet(value) => hash_filter_set(checksum, value),
        TriggerValue::BuildingCommandState(value) => {
            checksum.hash_u32(43);
            checksum.hash_u32(u32::from(value.is_done()));
            hash_entities(checksum, 44, value.trained_squads());
        }
        TriggerValue::AISquadAnalysis(value) => {
            checksum.hash_u32(45);
            value.hash_state(checksum);
        }
        TriggerValue::AISquadAnalysisComponent(value) => {
            checksum.hash_u32(46);
            checksum.hash_u32(*value as u32);
        }
        TriggerValue::Other { var_type, data } => {
            checksum.hash_u32(37);
            checksum.hash_u32(*var_type as u32);
            checksum.hash_u32(u32::try_from(data.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(data);
        }
    }
}

fn hash_filter_set(checksum: &mut SyncChecksum, filter_set: &EntityFilterSet) {
    checksum.hash_u32(36);
    filter_set.hash_state(checksum);
}

fn hash_i32(checksum: &mut SyncChecksum, tag: u32, value: i32) {
    checksum.hash_u32(tag);
    checksum.hash_i32(value);
}

fn hash_f32(checksum: &mut SyncChecksum, tag: u32, value: f32) {
    checksum.hash_u32(tag);
    checksum.hash_f32(value);
}

fn hash_entity(checksum: &mut SyncChecksum, tag: u32, value: crate::EntityId) {
    checksum.hash_u32(tag);
    checksum.hash_u32(value.as_u32());
}

fn hash_entities(checksum: &mut SyncChecksum, tag: u32, values: &[crate::EntityId]) {
    checksum.hash_u32(tag);
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_u32(value.as_u32());
    }
}

fn hash_i32s(checksum: &mut SyncChecksum, tag: u32, values: &[i32]) {
    checksum.hash_u32(tag);
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_i32(*value);
    }
}

fn hash_tagged_string(checksum: &mut SyncChecksum, tag: u32, value: &str) {
    checksum.hash_u32(tag);
    hash_string(checksum, value);
}

fn hash_strings(checksum: &mut SyncChecksum, tag: u32, values: &[String]) {
    checksum.hash_u32(tag);
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        hash_string(checksum, value);
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, tag: u32, value: super::value::Vec3) {
    checksum.hash_u32(tag);
    checksum.hash_vec3(value.x, value.y, value.z);
}

fn hash_vec3s(checksum: &mut SyncChecksum, tag: u32, values: &[super::value::Vec3]) {
    checksum.hash_u32(tag);
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_vec3(value.x, value.y, value.z);
    }
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}
