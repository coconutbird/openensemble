//! Per-building proxy shields relayed into a base's main plasma shield.

use super::{BaseAnchorSnapshot, World, is_building};
use crate::entities::BaseId;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, PlasmaSubshieldProfile};
use crate::scenario::configure_unit_from_proto;
use std::collections::BTreeMap;

impl World {
    pub(super) fn update_base_plasma_subshields(
        &mut self,
        base_id: BaseId,
        main_squad_id: EntityId,
        main_unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) {
        let eligible = self.eligible_subshield_buildings(base_id, gameplay);
        let existing = self.bases.get(&base_id).map_or_else(BTreeMap::new, |base| {
            base.plasma_shield.subshield_squads.clone()
        });
        for (&building_id, &shield_squad_id) in &existing {
            let Some(profile) = eligible.get(&building_id) else {
                self.remove_base_plasma_subshield(base_id, building_id);
                continue;
            };
            let matches = self
                .live_shield_child(shield_squad_id)
                .and_then(|unit_id| self.units.get(unit_id))
                .is_some_and(|unit| {
                    unit.proto_object_name
                        .eq_ignore_ascii_case(profile.shield_proto_object_name())
                });
            if !matches {
                self.remove_base_plasma_subshield(base_id, building_id);
            }
        }
        for (building_id, profile) in eligible {
            let squad_id = self
                .bases
                .get(&base_id)
                .and_then(|base| base.plasma_subshield_squad(building_id));
            let Some(squad_id) = squad_id else {
                self.create_base_plasma_subshield(
                    base_id,
                    building_id,
                    main_squad_id,
                    main_unit_id,
                    &profile,
                );
                continue;
            };
            if let Some(unit_id) = self.live_shield_child(squad_id) {
                self.synchronize_base_plasma_subshield(building_id, unit_id, main_unit_id);
            }
        }
    }

    fn eligible_subshield_buildings(
        &self,
        base_id: BaseId,
        gameplay: &GameplayCatalog,
    ) -> BTreeMap<EntityId, PlasmaSubshieldProfile> {
        let Some(base) = self.bases.get(&base_id) else {
            return BTreeMap::new();
        };
        base.buildings()
            .filter(|&building_id| building_id != base.anchor_building_id)
            .filter_map(|building_id| {
                let building = self.units.get(building_id)?;
                if !building.is_alive() || !building.built || building.build_socket_id.is_none() {
                    return None;
                }
                let profile = gameplay.plasma_subshield(&building.proto_object_name)?;
                Some((building_id, profile.clone()))
            })
            .collect()
    }

    fn create_base_plasma_subshield(
        &mut self,
        base_id: BaseId,
        building_id: EntityId,
        main_squad_id: EntityId,
        main_unit_id: EntityId,
        profile: &PlasmaSubshieldProfile,
    ) {
        let Some(building) = self.protected_building_snapshot(building_id) else {
            return;
        };
        let squad_id = self.create_squad_at(building.player_id, building.position);
        let unit_id = if is_building(profile.shield_proto_object()) {
            self.create_building_at(building.player_id, building.position)
        } else {
            self.create_unit_at(building.player_id, building.position)
        };
        configure_unit_from_proto(
            self,
            unit_id,
            profile.shield_proto_object_name(),
            profile.shield_proto_object_index(),
            profile.shield_proto_object(),
        );
        if !self.attach_unit_to_squad(unit_id, squad_id) {
            let _removed = self.remove_unit(unit_id);
            let _removed = self.remove_squad(squad_id);
            return;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            profile
                .shield_proto_object_name()
                .clone_into(&mut squad.proto_squad_name);
            squad.base.set_forward(building.forward);
            squad.set_damage_proxy(main_squad_id);
            squad.shields.clear_recharge_request();
        }
        let Some(protected_squad_id) = self.ensure_protected_building_squad(building) else {
            let _removed = self.kill_squad(squad_id, true);
            return;
        };
        if let Some(squad) = self.squads.get_mut(protected_squad_id) {
            squad.set_damage_proxy(squad_id);
        }
        if let Some(base) = self.bases.get_mut(&base_id) {
            base.plasma_shield
                .subshield_squads
                .insert(building_id, squad_id);
        }
        self.synchronize_base_plasma_subshield(building_id, unit_id, main_unit_id);
    }

    fn synchronize_base_plasma_subshield(
        &mut self,
        building_id: EntityId,
        subshield_unit_id: EntityId,
        main_unit_id: EntityId,
    ) {
        let Some(building) = self.protected_building_snapshot(building_id) else {
            return;
        };
        let main_percentage = self.units.get(main_unit_id).map_or(0.0, |main| {
            if main.shields.maximum > 0.0 {
                (main.shields.current / main.shields.maximum).clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        let Some(subshield) = self.units.get_mut(subshield_unit_id) else {
            return;
        };
        subshield.base.set_position(building.position);
        subshield.base.set_forward(building.forward);
        subshield.hitpoints = building.hitpoint_percentage.min(subshield.max_hitpoints);
        subshield
            .shields
            .set_current(subshield.shields.maximum * main_percentage);
        subshield.shields.clear_recharge_request();
    }

    fn protected_building_snapshot(&self, building_id: EntityId) -> Option<BaseAnchorSnapshot> {
        let building = self
            .units
            .get(building_id)
            .filter(|building| building.is_alive())?;
        let hitpoint_percentage = if building.max_hitpoints > 0.0 {
            (building.hitpoints / building.max_hitpoints).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some(BaseAnchorSnapshot {
            unit_id: building_id,
            player_id: building.base.player_id,
            position: building.base.position,
            forward: building.base.forward,
            hitpoint_percentage,
        })
    }

    fn remove_base_plasma_subshield(&mut self, base_id: BaseId, building_id: EntityId) {
        let squad_id = self
            .bases
            .get_mut(&base_id)
            .and_then(|base| base.plasma_shield.subshield_squads.remove(&building_id));
        if let Some(squad_id) = squad_id {
            let _removed = self.kill_squad(squad_id, true);
        }
    }

    pub(super) fn remove_base_plasma_subshields(&mut self, base_id: BaseId) {
        let building_ids = self.bases.get(&base_id).map_or_else(Vec::new, |base| {
            base.plasma_shield
                .subshield_squads
                .keys()
                .copied()
                .collect()
        });
        for building_id in building_ids {
            self.remove_base_plasma_subshield(base_id, building_id);
        }
    }
}
