//! Weapon damage resolution shared by direct and area attacks.

use super::World;
use crate::entities::units::UnitDeathState;
use crate::entities::{SquadMode, Unit};
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

    const fn reflected(player_id: PlayerId) -> Self {
        Self {
            unit_id: None,
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
            .filter(|target| accepts_weapon_damage_call(target))
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
            .filter(|target| accepts_weapon_damage_call(target))
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
        let damage_event_payload = damage * weapon_modifier;
        let health_before = receiving_target.hitpoints + receiving_target.shields.current;
        let hitpoints_before = receiving_target.hitpoints;
        let target_proto_object = receiving_target.proto_object_name.clone();
        let final_damage = damage * final_multiplier;
        if !final_damage.is_finite() || final_damage <= 0.0 {
            return 0.0;
        }
        let damaged = self.damage_weapon_target_and_record_crash(
            receiving_target_id,
            final_damage,
            direction,
            gameplay,
            attribution.unit_id,
            Some(attribution.player_id),
        );
        let reflection = self.reflection_for_damage_event(
            attribution,
            receiving_target_id,
            damage_event_payload,
            gameplay,
        );
        if !damaged {
            self.apply_reflected_damage(reflection, gameplay);
            return 0.0;
        }
        self.notify_ambient_life_damaged(receiving_target_id, attribution.unit_id);
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
        let death = killed.then(|| self.weapon_death_state(attribution, weapon_type));
        if let (Some(attacker_id), Some(gameplay)) = (attribution.unit_id, gameplay) {
            self.bank_combat_experience(
                attacker_id,
                &target_proto_object,
                (hitpoints_before - hitpoints_after).max(0.0),
                gameplay,
            );
        }
        self.apply_reflected_damage(reflection, gameplay);
        let dealt = ((health_before - health_after).max(0.0) / final_multiplier)
            .min(damage)
            .max(0.0);
        if let Some(death) = death {
            self.finish_credited_unit_death(
                receiving_target_id,
                death,
                target_proto_object,
                gameplay,
            );
        }
        dealt
    }

    pub(in crate::world) fn apply_forced_attributed_death_damage(
        &mut self,
        target_id: EntityId,
        damage: f32,
        death: UnitDeathState,
        gameplay: &GameplayCatalog,
    ) {
        let Some((target_proto_object, hitpoints_before)) = self
            .units
            .get(target_id)
            .map(|unit| (unit.proto_object_name.clone(), unit.hitpoints))
        else {
            return;
        };
        let _damaged = self.damage_unit_with_gameplay(target_id, damage, gameplay);
        if let Some(target) = self.units.get_mut(target_id).filter(|unit| unit.is_alive()) {
            target.kill();
        }
        let hitpoints_after = self
            .units
            .get(target_id)
            .map_or(hitpoints_before, |unit| unit.hitpoints);
        if let Some(attacker_id) = death.killer_entity_id() {
            self.bank_combat_experience(
                attacker_id,
                &target_proto_object,
                (hitpoints_before - hitpoints_after).max(0.0),
                gameplay,
            );
        }
        self.finish_credited_unit_death(target_id, death, target_proto_object, Some(gameplay));
    }

    fn finish_credited_unit_death(
        &mut self,
        target_id: EntityId,
        death: UnitDeathState,
        target_proto_object: String,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let killer_entity_id = death.killer_entity_id();
        let killer_player_id = death.killer_player_id();
        if let Some(target) = self.units.get_mut(target_id) {
            target.record_death(death);
        }
        if let (Some(gameplay), Some(killer_player_id)) = (gameplay, killer_player_id) {
            let _replacement =
                self.create_physics_detonate_replacement(target_id, killer_player_id, gameplay);
        }
        if let Some(attacker_id) = killer_entity_id {
            self.notify_ambient_life_killed_unit(attacker_id, target_id);
            self.queue_rage_kill(attacker_id, target_proto_object);
        }
    }

    fn weapon_death_state(
        &self,
        attribution: DamageAttribution,
        weapon_type: Option<&str>,
    ) -> UnitDeathState {
        UnitDeathState::new(
            attribution.unit_id,
            Some(attribution.player_id),
            self.get_player(attribution.player_id)
                .map(|player| player.team_id),
            weapon_type,
        )
    }

    fn damage_weapon_target_and_record_crash(
        &mut self,
        target_id: EntityId,
        damage: f32,
        direction: Option<Vec3>,
        gameplay: Option<&GameplayCatalog>,
        killer_id: Option<EntityId>,
        killer_player: Option<PlayerId>,
    ) -> bool {
        let was_crashing = self.units.get(target_id).is_some_and(Unit::is_crashing);
        let damaged = self.damage_resolved_weapon_target(target_id, damage, direction, gameplay);
        if !was_crashing && self.units.get(target_id).is_some_and(Unit::is_crashing) {
            self.record_aircraft_crash_killer(target_id, killer_id, killer_player);
        }
        damaged
    }

    fn damage_resolved_weapon_target(
        &mut self,
        target_id: EntityId,
        damage: f32,
        direction: Option<Vec3>,
        gameplay: Option<&GameplayCatalog>,
    ) -> bool {
        match (gameplay, direction) {
            (Some(gameplay), Some(direction)) => {
                self.damage_unit_directional_with_gameplay(target_id, damage, direction, gameplay)
            }
            (Some(gameplay), None) => self.damage_unit_with_gameplay(target_id, damage, gameplay),
            (None, Some(direction)) => self.damage_unit_directional(target_id, damage, direction),
            (None, None) => self.damage_unit(target_id, damage),
        }
    }

    fn reflection_for_damage_event(
        &self,
        attribution: DamageAttribution,
        damaged_unit_id: EntityId,
        damage: f32,
        gameplay: Option<&GameplayCatalog>,
    ) -> Option<crate::world::reflect_damage::ReflectedDamage> {
        attribution
            .unit_id
            .zip(gameplay)
            .and_then(|(attacker_id, gameplay)| {
                self.reflected_damage_request(damaged_unit_id, attacker_id, damage, gameplay)
            })
    }

    fn apply_reflected_damage(
        &mut self,
        reflection: Option<crate::world::reflect_damage::ReflectedDamage>,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let (Some(reflection), Some(gameplay)) = (reflection, gameplay) else {
            return;
        };
        let _dealt = self.apply_attributed_weapon_damage(
            DamageAttribution::reflected(reflection.player_id),
            reflection.target_id,
            reflection.damage,
            None,
            Some(gameplay),
        );
    }
}

fn accepts_weapon_damage_call(unit: &Unit) -> bool {
    unit.is_attackable()
        || (unit.is_invulnerable()
            && unit.is_alive()
            && !unit.is_incapacitated()
            && !unit.is_garrisoned()
            && !unit.is_being_boarded())
}
