//! Weapon damage resolution shared by direct and area attacks.

use super::World;
use crate::entities::SquadMode;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub(super) struct DamageAttribution {
    unit_id: Option<EntityId>,
    player_id: PlayerId,
}

impl DamageAttribution {
    pub(super) const fn combat(unit_id: EntityId, player_id: PlayerId) -> Self {
        Self {
            unit_id: Some(unit_id),
            player_id,
        }
    }

    #[cfg(test)]
    const fn scripted(player_id: PlayerId) -> Self {
        Self {
            unit_id: None,
            player_id,
        }
    }
}

impl World {
    #[cfg(test)]
    pub(in crate::world) fn apply_weapon_damage(
        &mut self,
        attacker_player_id: PlayerId,
        target_id: EntityId,
        damage: f32,
        weapon_type: Option<&str>,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        self.apply_weapon_damage_oriented(
            DamageAttribution::scripted(attacker_player_id),
            target_id,
            damage,
            weapon_type,
            None,
            gameplay,
        )
    }

    #[cfg(test)]
    pub(in crate::world) fn apply_directional_weapon_damage(
        &mut self,
        attacker_player_id: PlayerId,
        target_id: EntityId,
        damage: f32,
        weapon_type: Option<&str>,
        direction: Vec3,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        self.apply_weapon_damage_oriented(
            DamageAttribution::scripted(attacker_player_id),
            target_id,
            damage,
            weapon_type,
            Some(direction),
            gameplay,
        )
    }

    pub(super) fn apply_attributed_weapon_damage(
        &mut self,
        attribution: DamageAttribution,
        target_id: EntityId,
        damage: f32,
        weapon_type: Option<&str>,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        self.apply_weapon_damage_oriented(
            attribution,
            target_id,
            damage,
            weapon_type,
            None,
            gameplay,
        )
    }

    pub(in crate::world) fn apply_reflected_collision_damage(
        &mut self,
        source_id: EntityId,
        source_player_id: PlayerId,
        target_id: EntityId,
        damage: f32,
        gameplay: &GameplayCatalog,
    ) -> f32 {
        self.apply_attributed_weapon_damage(
            DamageAttribution::combat(source_id, source_player_id),
            target_id,
            damage,
            None,
            Some(gameplay),
        )
    }

    pub(super) fn apply_attributed_directional_weapon_damage(
        &mut self,
        attribution: DamageAttribution,
        target_id: EntityId,
        damage: f32,
        weapon_type: Option<&str>,
        direction: Vec3,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        self.apply_weapon_damage_oriented(
            attribution,
            target_id,
            damage,
            weapon_type,
            Some(direction),
            gameplay,
        )
    }

    fn apply_weapon_damage_oriented(
        &mut self,
        attribution: DamageAttribution,
        requested_target_id: EntityId,
        damage: f32,
        weapon_type: Option<&str>,
        direction: Option<Vec3>,
        gameplay: Option<&GameplayCatalog>,
    ) -> f32 {
        let Some(modifier_target) = self
            .units
            .get(requested_target_id)
            .filter(|target| target.is_attackable())
        else {
            return 0.0;
        };
        let target_mode = modifier_target
            .squad_id
            .and_then(|squad_id| self.squads.get(squad_id))
            .map_or(SquadMode::Normal, |squad| squad.mode);
        let receiving_target_id = self.resolve_damage_target(requested_target_id);
        let Some(receiving_target) = self
            .units
            .get(receiving_target_id)
            .filter(|target| target.is_attackable())
        else {
            return 0.0;
        };
        let damage_direction = direction.unwrap_or(-receiving_target.base.forward);
        let weapon_modifier = gameplay.map_or(1.0, |catalog| {
            catalog.directional_weapon_damage_modifier(
                weapon_type,
                &modifier_target.proto_object_name,
                damage_direction,
                modifier_target.base.forward,
                target_mode,
                self.get_player(attribution.player_id)
                    .map(|player| &player.technologies),
            )
        });
        let construction_modifier = if receiving_target.is_building() && !receiving_target.built {
            self.construction_damage_multiplier
        } else {
            1.0
        };
        let final_multiplier = weapon_modifier
            * construction_modifier
            * receiving_target.effective_damage_taken_multiplier();
        let health_before = receiving_target.hitpoints + receiving_target.shields.current;
        let hitpoints_before = receiving_target.hitpoints;
        let target_proto_object = receiving_target.proto_object_name.clone();
        let final_damage = damage * final_multiplier;
        if !final_damage.is_finite() || final_damage <= 0.0 {
            return 0.0;
        }
        let damaged = match (gameplay, direction) {
            (Some(gameplay), Some(direction)) => self.damage_unit_directional_with_gameplay(
                receiving_target_id,
                final_damage,
                direction,
                gameplay,
            ),
            (Some(gameplay), None) => {
                self.damage_unit_with_gameplay(receiving_target_id, final_damage, gameplay)
            }
            (None, Some(direction)) => {
                self.damage_unit_directional(receiving_target_id, final_damage, direction)
            }
            (None, None) => self.damage_unit(receiving_target_id, final_damage),
        };
        if !damaged {
            return 0.0;
        }
        let (health_after, hitpoints_after) = self
            .units
            .get(receiving_target_id)
            .map_or((health_before, hitpoints_before), |target| {
                (target.hitpoints + target.shields.current, target.hitpoints)
            });
        let killed = self
            .units
            .get(receiving_target_id)
            .is_some_and(|target| !target.is_alive());
        if let (Some(attacker_id), Some(gameplay)) = (attribution.unit_id, gameplay) {
            self.bank_combat_experience(
                attacker_id,
                &target_proto_object,
                (hitpoints_before - hitpoints_after).max(0.0),
                gameplay,
            );
        }
        let dealt = ((health_before - health_after).max(0.0) / final_multiplier)
            .min(damage)
            .max(0.0);
        if killed && let Some(gameplay) = gameplay {
            let _replacement = self.create_physics_detonate_replacement(
                receiving_target_id,
                attribution.player_id,
                gameplay,
            );
        }
        if killed && let Some(attacker_id) = attribution.unit_id {
            self.queue_rage_kill(attacker_id, target_proto_object);
        }
        dealt
    }
}
