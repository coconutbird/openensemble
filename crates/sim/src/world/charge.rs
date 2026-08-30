//! Authoritative persistent Charge accumulation and ready-state effects.

mod pull;

use super::World;
use crate::entity_id::EntityId;
use crate::gameplay::{ChargeActionProfile, GameplayCatalog};

impl World {
    pub(super) fn update_charges(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            self.update_unit_charge(unit_id, dt, gameplay);
        }
    }

    fn update_unit_charge(&mut self, unit_id: EntityId, dt: f32, gameplay: &GameplayCatalog) {
        let profile = self
            .units
            .get(unit_id)
            .and_then(|unit| gameplay.charge(&unit.proto_object_name))
            .cloned();
        let Some(profile) = profile else {
            let attachment = self
                .units
                .get_mut(unit_id)
                .and_then(|unit| unit.charge.reset());
            self.remove_charge_effect(attachment);
            return;
        };

        let enabled = self.charge_action_enabled(unit_id, &profile);
        let replaced_attachment = self.units.get_mut(unit_id).and_then(|unit| {
            unit.charge
                .configure(profile.action_name(), profile.damage_charge())
        });
        self.remove_charge_effect(replaced_attachment);
        self.clear_missing_charge_effect(unit_id);

        let ready = self
            .units
            .get_mut(unit_id)
            .is_some_and(|unit| unit.charge.advance(dt, enabled));
        let expected_effect = ready
            .then(|| profile.effect())
            .flatten()
            .and_then(|effect| {
                effect
                    .prototype_id()
                    .map(|id| (id, effect.prototype_name()))
            });
        self.reconcile_charge_effect(unit_id, expected_effect);
    }

    fn charge_action_enabled(&self, unit_id: EntityId, profile: &ChargeActionProfile) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let authored_enabled = !profile.starts_disabled();
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        unit.logical_proto_object_name(),
                        profile.action_name(),
                        authored_enabled,
                    )
                });
        unit.is_operational()
            && unit
                .actions
                .is_enabled(profile.action_name(), !player_enabled)
    }

    fn clear_missing_charge_effect(&mut self, unit_id: EntityId) {
        let missing = self
            .units
            .get(unit_id)
            .and_then(|unit| unit.charge.attachment_entity_id())
            .is_some_and(|attachment| self.objects.get(attachment).is_none());
        if missing && let Some(unit) = self.units.get_mut(unit_id) {
            unit.charge.set_attachment_entity_id(None);
        }
    }

    fn reconcile_charge_effect(&mut self, unit_id: EntityId, expected: Option<(i32, &str)>) {
        let current = self
            .units
            .get(unit_id)
            .and_then(|unit| unit.charge.attachment_entity_id());
        let matches = current
            .zip(expected)
            .is_some_and(|(attachment_id, (prototype_id, _))| {
                self.objects
                    .get(attachment_id)
                    .is_some_and(|object| object.proto_object_id == prototype_id)
            });
        if matches {
            return;
        }
        self.remove_charge_effect(current);
        let attachment = expected.and_then(|(prototype_id, prototype_name)| {
            self.add_visual_attachment_to_unit(unit_id, prototype_id, prototype_name)
        });
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.charge.set_attachment_entity_id(attachment);
        }
    }

    fn remove_charge_effect(&mut self, attachment: Option<EntityId>) {
        if let Some(attachment) = attachment {
            let _removed = self.remove_object(attachment);
        }
    }
}

#[cfg(test)]
mod tests;
