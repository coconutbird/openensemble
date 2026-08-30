//! Authoritative persistent energy-shield action lifecycle.

use super::World;
use crate::entities::{EnergyShieldPhase, EnergyShieldPresentationKind, UnitEnergyShieldAction};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{EnergyShieldActionProfile, EnergyShieldVisualProfile, GameplayCatalog};
use num_traits::ToPrimitive;

struct ShieldActionContext {
    profile: EnergyShieldActionProfile,
    enabled: bool,
}

impl World {
    pub(super) fn update_energy_shield_presentations(
        &mut self,
        dt: f32,
        gameplay: &GameplayCatalog,
    ) {
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            self.update_unit_energy_shield_presentations(unit_id, dt, gameplay);
        }
    }

    fn update_unit_energy_shield_presentations(
        &mut self,
        unit_id: EntityId,
        dt: f32,
        gameplay: &GameplayCatalog,
    ) {
        let contexts = self.energy_shield_contexts(unit_id, gameplay);
        let mut previous = self
            .units
            .get_mut(unit_id)
            .map(|unit| unit.shields.take_energy_shield_actions())
            .unwrap_or_default();
        let previous_components = infantry_components(&previous);
        let mut actions = Vec::with_capacity(contexts.len());
        for context in contexts {
            let mut action = take_matching_action(&mut previous, &context.profile)
                .unwrap_or_else(|| action_from_profile(&context.profile));
            self.update_energy_shield_action(unit_id, dt, gameplay, context.enabled, &mut action);
            actions.push(action);
        }
        for action in previous {
            self.remove_energy_shield_attachment(&action);
        }
        self.project_infantry_components(unit_id, &previous_components, &actions);
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.shields.replace_energy_shield_actions(actions);
        }
    }

    fn energy_shield_contexts(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Vec<ShieldActionContext> {
        let Some(unit) = self.units.get(unit_id) else {
            return Vec::new();
        };
        let player = self.get_player(unit.base.player_id);
        gameplay
            .energy_shield_actions(&unit.proto_object_name)
            .iter()
            .cloned()
            .map(|profile| {
                let authored_enabled = !profile.starts_disabled();
                let player_enabled = player.map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    )
                });
                let enabled = unit
                    .actions
                    .is_enabled(profile.action_name(), !player_enabled);
                ShieldActionContext { profile, enabled }
            })
            .collect()
    }

    fn update_energy_shield_action(
        &mut self,
        unit_id: EntityId,
        dt: f32,
        gameplay: &GameplayCatalog,
        enabled: bool,
        action: &mut UnitEnergyShieldAction,
    ) {
        match action.kind {
            EnergyShieldPresentationKind::Attachment => {
                self.update_external_energy_shield(unit_id, dt, gameplay, enabled, action);
            }
            EnergyShieldPresentationKind::InfantryComponent => {
                update_infantry_energy_shield(dt, enabled, action);
            }
        }
    }

    fn update_external_energy_shield(
        &mut self,
        unit_id: EntityId,
        dt: f32,
        gameplay: &GameplayCatalog,
        enabled: bool,
        action: &mut UnitEnergyShieldAction,
    ) {
        if action
            .attachment_entity_id
            .is_some_and(|attachment_id| self.get_object(attachment_id).is_none())
        {
            action.attachment_entity_id = None;
            action.phase = EnergyShieldPhase::Down;
            action.transition_seconds_remaining = None;
        }
        if !enabled {
            self.lower_energy_shield_immediately(action);
            return;
        }
        match action.phase {
            EnergyShieldPhase::Down => self.raise_energy_shield(unit_id, gameplay, action),
            EnergyShieldPhase::Up => {}
            EnergyShieldPhase::Hit => {
                if action.transition_seconds_remaining.is_none() {
                    let duration = self.play_shield_animation(gameplay, action, "Incoming");
                    action.transition_seconds_remaining = Some(duration);
                } else if advance_transition(action, dt) {
                    let _duration = self.play_shield_animation(gameplay, action, "Idle");
                    action.phase = EnergyShieldPhase::Up;
                    action.transition_seconds_remaining = None;
                }
            }
            EnergyShieldPhase::Lowering => {
                if action.transition_seconds_remaining.is_none() {
                    let duration = self.play_shield_animation(gameplay, action, "Death");
                    action.transition_seconds_remaining = Some(duration);
                } else if advance_transition(action, dt) {
                    self.lower_energy_shield_immediately(action);
                }
            }
        }
    }

    fn raise_energy_shield(
        &mut self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
        action: &mut UnitEnergyShieldAction,
    ) {
        let shields_are_up = self
            .units
            .get(unit_id)
            .is_some_and(|unit| unit.is_alive() && unit.shields.current > 0.0 && !unit.is_down());
        let (Some(prototype_id), Some(prototype_name)) =
            (action.prototype_id, action.prototype_name.as_deref())
        else {
            return;
        };
        if !shields_are_up {
            return;
        }
        let Some(attachment_id) =
            self.add_visual_attachment_to_unit(unit_id, prototype_id, prototype_name)
        else {
            return;
        };
        action.attachment_entity_id = Some(attachment_id);
        action.phase = EnergyShieldPhase::Up;
        action.transition_seconds_remaining = None;
        let _duration = self.play_shield_animation(gameplay, action, "Idle");
    }

    fn play_shield_animation(
        &mut self,
        gameplay: &GameplayCatalog,
        action: &UnitEnergyShieldAction,
        animation: &str,
    ) -> f32 {
        let Some(attachment_id) = action.attachment_entity_id else {
            return 0.0;
        };
        let clip = action
            .prototype_name
            .as_deref()
            .and_then(|prototype| gameplay.scripted_animation_clip(prototype, animation));
        let asset_path = clip.map(|clip| clip.asset_path().to_owned());
        let duration_ms = match clip {
            Some(clip) => clip.duration_ms(),
            None => 0,
        };
        let _played = self.play_entity_animation(
            attachment_id,
            animation.to_owned(),
            asset_path,
            duration_ms,
        );
        milliseconds_to_seconds(duration_ms)
    }

    fn lower_energy_shield_immediately(&mut self, action: &mut UnitEnergyShieldAction) {
        self.remove_energy_shield_attachment(action);
        action.attachment_entity_id = None;
        action.phase = EnergyShieldPhase::Down;
        action.transition_seconds_remaining = None;
    }

    fn remove_energy_shield_attachment(&mut self, action: &UnitEnergyShieldAction) {
        if let Some(attachment_id) = action.attachment_entity_id {
            let _removed = self.remove_object(attachment_id);
        }
    }

    fn project_infantry_components(
        &mut self,
        unit_id: EntityId,
        previous: &[String],
        actions: &[UnitEnergyShieldAction],
    ) {
        let current = infantry_components(actions);
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        for component in previous {
            if !contains_case_insensitive(&current, component) {
                unit.set_visual_component_visible(component, true);
            }
        }
        for component in current {
            unit.set_visual_component_visible(&component, false);
        }
    }
}

fn take_matching_action(
    actions: &mut Vec<UnitEnergyShieldAction>,
    profile: &EnergyShieldActionProfile,
) -> Option<UnitEnergyShieldAction> {
    let index = actions.iter().position(|action| {
        action
            .action_name
            .eq_ignore_ascii_case(profile.action_name())
            && action_matches_profile(action, profile)
    })?;
    Some(actions.remove(index))
}

fn action_matches_profile(
    action: &UnitEnergyShieldAction,
    profile: &EnergyShieldActionProfile,
) -> bool {
    match profile.visual() {
        EnergyShieldVisualProfile::Attachment {
            prototype_name,
            prototype_id,
            bone_name,
        } => {
            action.kind == EnergyShieldPresentationKind::Attachment
                && action.prototype_name.as_ref() == Some(prototype_name)
                && action.prototype_id == *prototype_id
                && action.bone_name == *bone_name
        }
        EnergyShieldVisualProfile::Infantry {
            component_name,
            hit_duration_ms,
        } => {
            action.kind == EnergyShieldPresentationKind::InfantryComponent
                && action.component_name.as_ref() == Some(component_name)
                && action.hit_duration_ms == *hit_duration_ms
        }
    }
}

fn action_from_profile(profile: &EnergyShieldActionProfile) -> UnitEnergyShieldAction {
    let mut action = UnitEnergyShieldAction {
        action_name: profile.action_name().to_owned(),
        kind: EnergyShieldPresentationKind::Attachment,
        phase: EnergyShieldPhase::Down,
        attachment_entity_id: None,
        transition_seconds_remaining: None,
        prototype_name: None,
        prototype_id: None,
        bone_name: None,
        component_name: None,
        hit_duration_ms: 0,
    };
    match profile.visual() {
        EnergyShieldVisualProfile::Attachment {
            prototype_name,
            prototype_id,
            bone_name,
        } => {
            action.prototype_name = Some(prototype_name.clone());
            action.prototype_id = *prototype_id;
            action.bone_name.clone_from(bone_name);
        }
        EnergyShieldVisualProfile::Infantry {
            component_name,
            hit_duration_ms,
        } => {
            action.kind = EnergyShieldPresentationKind::InfantryComponent;
            action.component_name = Some(component_name.clone());
            action.hit_duration_ms = *hit_duration_ms;
        }
    }
    action
}

fn update_infantry_energy_shield(dt: f32, enabled: bool, action: &mut UnitEnergyShieldAction) {
    if !enabled {
        action.phase = EnergyShieldPhase::Down;
        action.transition_seconds_remaining = None;
        return;
    }
    if action.phase != EnergyShieldPhase::Hit {
        action.phase = EnergyShieldPhase::Down;
        return;
    }
    if action.transition_seconds_remaining.is_none() {
        action.transition_seconds_remaining = Some(milliseconds_to_seconds(action.hit_duration_ms));
    } else if advance_transition(action, dt) {
        action.phase = EnergyShieldPhase::Down;
        action.transition_seconds_remaining = None;
    }
}

fn advance_transition(action: &mut UnitEnergyShieldAction, dt: f32) -> bool {
    let Some(remaining) = &mut action.transition_seconds_remaining else {
        return false;
    };
    *remaining = (*remaining - dt.max(0.0)).max(0.0);
    *remaining <= f32::EPSILON
}

fn infantry_components(actions: &[UnitEnergyShieldAction]) -> Vec<String> {
    let mut components = actions
        .iter()
        .filter(|action| action.kind == EnergyShieldPresentationKind::InfantryComponent)
        .filter_map(|action| action.component_name.clone())
        .collect::<Vec<_>>();
    components.sort_by_key(|component| component.to_ascii_lowercase());
    components.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    components
}

fn contains_case_insensitive(values: &[String], expected: &str) -> bool {
    values
        .iter()
        .any(|value| value.eq_ignore_ascii_case(expected))
}

fn milliseconds_to_seconds(milliseconds: u32) -> f32 {
    milliseconds.to_f32().unwrap_or(f32::MAX) / 1_000.0
}

#[cfg(test)]
mod tests;
