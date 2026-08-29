//! Persistent Covenant base-shield generator lifecycle.

use super::World;
use crate::entities::{Base, BaseId, BasePlasmaShield};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, PlasmaShieldGeneratorProfile};
use crate::scenario::configure_unit_from_proto;
use glam::Vec3;
use std::collections::BTreeSet;

mod subshields;

#[derive(Debug, Clone)]
struct GeneratorSelection {
    primary_id: EntityId,
    count: usize,
    profile: PlasmaShieldGeneratorProfile,
}

#[derive(Debug, Clone, Copy)]
struct BaseAnchorSnapshot {
    unit_id: EntityId,
    player_id: u8,
    position: Vec3,
    forward: Vec3,
    hitpoint_percentage: f32,
}

impl World {
    pub(in crate::world) fn update_plasma_shields(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let base_ids = self.bases.keys().copied().collect::<Vec<_>>();
        for base_id in base_ids {
            self.update_base_plasma_shield(base_id, dt, gameplay);
        }
    }

    fn update_base_plasma_shield(&mut self, base_id: BaseId, dt: f32, gameplay: &GameplayCatalog) {
        let Some(selection) = self.select_plasma_generators(base_id, gameplay) else {
            self.disable_base_plasma_shield(base_id);
            return;
        };
        if let Some(base) = self.bases.get_mut(&base_id) {
            base.plasma_shield.primary_generator_id = Some(selection.primary_id);
        }

        let configured_squad = self
            .bases
            .get(&base_id)
            .and_then(|base| base.plasma_shield.shield_squad_id);
        if let Some((shield_squad_id, shield_unit_id)) =
            configured_squad.and_then(|id| self.live_shield_child(id).map(|unit| (id, unit)))
        {
            self.synchronize_base_plasma_shield(
                base_id,
                shield_squad_id,
                shield_unit_id,
                selection.count,
            );
            self.update_base_plasma_subshields(base_id, shield_squad_id, shield_unit_id, gameplay);
            return;
        }

        let destroyed = configured_squad.is_some();
        if let Some(shield_squad_id) = configured_squad {
            self.remove_base_plasma_subshields(base_id);
            let _removed = self.kill_squad(shield_squad_id, true);
            if let Some(base) = self.bases.get_mut(&base_id) {
                base.plasma_shield.shield_squad_id = None;
                base.plasma_shield.rebuild_remaining = selection.profile.rebuild_time();
            }
        }
        if destroyed || self.defer_plasma_shield_rebuild(base_id, dt, &selection.profile) {
            return;
        }
        self.create_base_plasma_shield(base_id, &selection, gameplay);
    }

    fn select_plasma_generators(
        &self,
        base_id: BaseId,
        gameplay: &GameplayCatalog,
    ) -> Option<GeneratorSelection> {
        let base = self.bases.get(&base_id)?;
        self.units
            .get(base.anchor_building_id)
            .filter(|anchor| anchor.is_alive())?;
        let generators = base
            .buildings()
            .filter(|&unit_id| {
                self.units.get(unit_id).is_some_and(|unit| {
                    unit.is_operational()
                        && gameplay
                            .plasma_shield_generator(&unit.proto_object_name)
                            .is_some()
                })
            })
            .collect::<Vec<_>>();
        let primary_id = base
            .plasma_shield
            .primary_generator_id
            .filter(|id| generators.contains(id))
            .or_else(|| generators.first().copied())?;
        let generator = self.units.get(primary_id)?;
        let profile = gameplay
            .plasma_shield_generator(&generator.proto_object_name)?
            .clone();
        Some(GeneratorSelection {
            primary_id,
            count: generators.len(),
            profile,
        })
    }

    fn defer_plasma_shield_rebuild(
        &mut self,
        base_id: BaseId,
        dt: f32,
        profile: &PlasmaShieldGeneratorProfile,
    ) -> bool {
        let under_attack = self.base_is_under_attack(base_id);
        let Some(base) = self.bases.get_mut(&base_id) else {
            return true;
        };
        if under_attack {
            base.plasma_shield.attack_wait_remaining = profile.under_attack_wait();
            base.plasma_shield.rebuild_remaining = profile.rebuild_time();
            return true;
        }
        if base.plasma_shield.attack_wait_remaining > 0.0 {
            base.plasma_shield.attack_wait_remaining = countdown(
                base.plasma_shield.attack_wait_remaining,
                dt,
                profile.under_attack_wait(),
            );
            base.plasma_shield.rebuild_remaining = profile.rebuild_time();
            return true;
        }
        if base.plasma_shield.rebuild_remaining > 0.0 {
            base.plasma_shield.rebuild_remaining = countdown(
                base.plasma_shield.rebuild_remaining,
                dt,
                profile.rebuild_time(),
            );
        }
        base.plasma_shield.rebuild_remaining > 0.0
    }

    fn create_base_plasma_shield(
        &mut self,
        base_id: BaseId,
        selection: &GeneratorSelection,
        gameplay: &GameplayCatalog,
    ) {
        let Some(anchor) = self.base_anchor_snapshot(base_id) else {
            return;
        };
        let profile = &selection.profile;
        let shield_squad_id = self.create_squad_at(anchor.player_id, anchor.position);
        let shield_unit_id = if is_building(profile.shield_proto_object()) {
            self.create_building_at(anchor.player_id, anchor.position)
        } else {
            self.create_unit_at(anchor.player_id, anchor.position)
        };
        configure_unit_from_proto(
            self,
            shield_unit_id,
            profile.shield_proto_object_name(),
            profile.shield_proto_object_index(),
            profile.shield_proto_object(),
        );
        if !self.attach_unit_to_squad(shield_unit_id, shield_squad_id) {
            let _removed = self.remove_unit(shield_unit_id);
            let _removed = self.remove_squad(shield_squad_id);
            return;
        }
        if let Some(squad) = self.squads.get_mut(shield_squad_id) {
            profile
                .shield_proto_object_name()
                .clone_into(&mut squad.proto_squad_name);
            squad.base.set_forward(anchor.forward);
        }
        let Some(protected_squad_id) = self.ensure_protected_building_squad(anchor) else {
            let _removed = self.kill_squad(shield_squad_id, true);
            return;
        };
        if let Some(squad) = self.squads.get_mut(protected_squad_id) {
            squad.set_damage_proxy(shield_squad_id);
        }
        if let Some(base) = self.bases.get_mut(&base_id) {
            base.plasma_shield.shield_squad_id = Some(shield_squad_id);
            base.plasma_shield.rebuild_remaining = 0.0;
            base.plasma_shield.attack_wait_remaining = 0.0;
        }
        self.synchronize_base_plasma_shield(
            base_id,
            shield_squad_id,
            shield_unit_id,
            selection.count,
        );
        self.update_base_plasma_subshields(base_id, shield_squad_id, shield_unit_id, gameplay);
    }

    fn ensure_protected_building_squad(&mut self, anchor: BaseAnchorSnapshot) -> Option<EntityId> {
        if let Some(squad_id) = self.units.get(anchor.unit_id)?.squad_id
            && self.squads.get(squad_id).is_some()
        {
            return Some(squad_id);
        }
        let squad_id = self.create_squad_at(anchor.player_id, anchor.position);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.base.set_forward(anchor.forward);
        }
        if self.attach_unit_to_squad(anchor.unit_id, squad_id) {
            Some(squad_id)
        } else {
            let _removed = self.remove_squad(squad_id);
            None
        }
    }

    fn synchronize_base_plasma_shield(
        &mut self,
        base_id: BaseId,
        squad_id: EntityId,
        unit_id: EntityId,
        generator_count: usize,
    ) {
        let Some(anchor) = self.base_anchor_snapshot(base_id) else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.base.set_position(anchor.position);
            squad.base.set_forward(anchor.forward);
        }
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.base.set_position(anchor.position);
            unit.base.set_forward(anchor.forward);
            unit.hitpoints = anchor.hitpoint_percentage.min(unit.max_hitpoints);
            let generator_count = u16::try_from(generator_count.max(1)).unwrap_or(u16::MAX);
            unit.damage_taken_multiplier = 1.0 / f32::from(generator_count);
        }
        if let Some(base) = self.bases.get_mut(&base_id) {
            base.plasma_shield.rebuild_remaining = 0.0;
            base.plasma_shield.attack_wait_remaining = 0.0;
        }
    }

    fn base_anchor_snapshot(&self, base_id: BaseId) -> Option<BaseAnchorSnapshot> {
        let base = self.bases.get(&base_id)?;
        let anchor = self.units.get(base.anchor_building_id)?;
        let hitpoint_percentage = if anchor.max_hitpoints > 0.0 {
            (anchor.hitpoints / anchor.max_hitpoints).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some(BaseAnchorSnapshot {
            unit_id: base.anchor_building_id,
            player_id: base.player_id,
            position: anchor.base.position,
            forward: anchor.base.forward,
            hitpoint_percentage,
        })
    }

    fn live_shield_child(&self, squad_id: EntityId) -> Option<EntityId> {
        let squad = self.squads.get(squad_id).filter(|squad| squad.is_alive())?;
        let unit_id = squad.unit_ids.first().copied()?;
        self.units
            .get(unit_id)
            .filter(|unit| unit.is_alive())
            .map(|_| unit_id)
    }

    fn base_is_under_attack(&self, base_id: BaseId) -> bool {
        let Some(base) = self.bases.get(&base_id) else {
            return false;
        };
        let mut targets = base.buildings().collect::<BTreeSet<_>>();
        for building_id in base.buildings() {
            if let Some(squad_id) = self.units.get(building_id).and_then(|unit| unit.squad_id) {
                targets.insert(squad_id);
            }
        }
        self.squads.iter().any(|(_, squad)| {
            squad.is_alive() && squad.attack_target.is_some_and(|id| targets.contains(&id))
        }) || self.units.iter().any(|(_, unit)| {
            unit.is_alive()
                && unit.squad_id.is_none()
                && unit.attack_target.is_some_and(|id| targets.contains(&id))
        })
    }

    pub(in crate::world) fn disable_base_plasma_shield(&mut self, base_id: BaseId) {
        self.remove_base_plasma_subshields(base_id);
        let shield_squad_id = self
            .bases
            .get(&base_id)
            .and_then(|base| base.plasma_shield.shield_squad_id);
        if let Some(shield_squad_id) = shield_squad_id {
            let _removed = self.kill_squad(shield_squad_id, true);
        }
        if let Some(base) = self.bases.get_mut(&base_id) {
            base.plasma_shield = BasePlasmaShield::default();
        }
    }

    pub(in crate::world) fn destroy_removed_base_plasma_shield(&mut self, base: &Base) {
        for &shield_squad_id in base.plasma_shield.subshield_squads.values() {
            let _removed = self.kill_squad(shield_squad_id, true);
        }
        if let Some(shield_squad_id) = base.plasma_shield_squad() {
            let _removed = self.kill_squad(shield_squad_id, true);
        }
    }
}

fn is_building(proto: &pipeline::database::hw1::ProtoObject) -> bool {
    proto
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Building"))
}

fn countdown(remaining: f32, dt: f32, authored_duration: f32) -> f32 {
    let next = (remaining - dt).max(0.0);
    let tolerance = f32::EPSILON * authored_duration.max(1.0) * 64.0;
    if next <= tolerance { 0.0 } else { next }
}

#[cfg(test)]
mod tests;
