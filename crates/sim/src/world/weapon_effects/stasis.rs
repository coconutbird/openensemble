//! Stasis: `BUnitActionRangedAttack_applyStasisEffect` (`0x14055DF60`), the stasis bomb and
//! the release of `BUnitActionRangedAttack` (2008 `unitactionrangedattack.cpp:310-318`,
//! `609-625`, `4173-4185`), and `BUnitActionStasis_update` (`0x140569930`).

use super::StasisFire;
use crate::entities::SquadState;
use crate::entities::units::weapon_effects::StasisHold;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, WeaponEffectProfile};
use crate::world::World;
use crate::world::combat::AttackDamage;
use glam::Vec3;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

/// `pTargetUnit->getHitpoints() <= 1.0f`: a stasis bomb also catches a crashing aircraft.
const STASIS_BOMB_HITPOINTS: f32 = 1.0;

impl World {
    /// The stasis of one fired attack: the application on the target and the self heal of a
    /// stasis beam (`BUnitActionRangedAttack::doBeamAttack`, `mSelfHealAmount`).
    pub(in crate::world) fn apply_stasis_on_fire(
        &mut self,
        fired: &StasisFire,
        effects: &WeaponEffectProfile,
    ) {
        let heal = if effects.stasis.stasis {
            effects.damage_per_second * effects.stasis.heal_to_drain_ratio
        } else {
            0.0
        };
        if heal != 0.0
            && let Some(squad_id) = self
                .units
                .get(fired.source_id)
                .and_then(|unit| unit.squad_id)
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.weapon_effects.pending_self_heal += heal;
        }
        let Some(hold) = self
            .units
            .get(fired.source_id)
            .map(|unit| unit.weapon_effects.stasis_hold.clone())
        else {
            return;
        };
        if let Some(previous) = hold.as_ref().filter(|hold| {
            hold.target_id != fired.target_id
                || !hold.action_name.eq_ignore_ascii_case(&fired.action_name)
        }) {
            // `BUnitActionRangedAttack_changeTarget` releases the old target first.
            self.release_stasis_hold(fired.source_id, previous.target_id);
            return self.apply_stasis_on_fire(fired, effects);
        }
        let needs_application = (effects.stasis.stasis && hold.is_none())
            || (effects.stasis.drain && !hold.as_ref().is_some_and(|hold| hold.drain))
            || (effects.stasis.bomb && !hold.as_ref().is_some_and(|hold| hold.bomb));
        if needs_application {
            self.apply_stasis_effect(fired, effects, hold.as_ref());
        }
    }

    /// `BUnitActionRangedAttack_applyStasisEffect`.
    fn apply_stasis_effect(
        &mut self,
        fired: &StasisFire,
        effects: &WeaponEffectProfile,
        hold: Option<&StasisHold>,
    ) {
        let Some(target) = self.units.get_mut(fired.target_id) else {
            return;
        };
        let Some(squad_id) = target.squad_id.filter(|id| self.squads.get(*id).is_some()) else {
            return;
        };
        // Transporters cannot be held in stasis.
        if target.is_object_type("Transporter") {
            return;
        }
        target.weapon_effects.stasis_action = true;
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.weapon_effects.stasis_effects =
                squad.weapon_effects.stasis_effects.wrapping_add(1);
        }
        if let Some(attacker) = self.units.get_mut(fired.source_id) {
            attacker.weapon_effects.stasis_hold = Some(StasisHold {
                target_id: fired.target_id,
                action_name: fired.action_name.clone(),
                drain: effects.stasis.drain || hold.is_some_and(|hold| hold.drain),
                bomb: effects.stasis.bomb || hold.is_some_and(|hold| hold.bomb),
            });
        }
    }

    /// The stasis holds of the ranged attacks: the stasis bomb on a target at 1 hit point or
    /// less, then the release when the attack ends or changes its target.
    pub(super) fn update_stasis_holds(&mut self, gameplay: &GameplayCatalog) {
        let holders = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                unit.weapon_effects
                    .stasis_hold
                    .as_ref()
                    .map(|hold| (id, hold.clone()))
            })
            .collect::<Vec<_>>();
        for (attacker_id, hold) in holders {
            if hold.bomb {
                self.detonate_stasis_bomb(attacker_id, &hold, gameplay);
            }
            // The ranged attack ends with the squad attack, also while the unit still keeps
            // its last engagement.
            let attacking = self.units.get(attacker_id).is_some_and(|unit| {
                unit.is_alive()
                    && unit.combat.engaged_target() == Some(hold.target_id)
                    && unit
                        .combat
                        .action_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case(&hold.action_name))
                    && unit
                        .squad_id
                        .and_then(|id| self.squads.get(id))
                        .is_none_or(|squad| squad.state == SquadState::Attacking)
            });
            let target_alive = self.units.get(hold.target_id).is_some_and(Entity::is_alive);
            if !attacking || !target_alive {
                self.release_stasis_hold(attacker_id, hold.target_id);
            }
        }
    }

    /// `BSquad::releaseStasisEffect` for the target of a hold, and the end of the hold.
    fn release_stasis_hold(&mut self, attacker_id: EntityId, target_id: EntityId) {
        if let Some(attacker) = self.units.get_mut(attacker_id) {
            attacker.weapon_effects.stasis_hold = None;
        }
        if let Some(squad_id) = self.units.get(target_id).and_then(|unit| unit.squad_id)
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.weapon_effects.stasis_effects =
                squad.weapon_effects.stasis_effects.wrapping_sub(1);
        }
    }

    /// The linked area damage of a `StasisBomb` weapon at a held target with 1 hit point or
    /// less (`doAreaEffectDamage` at the target position).
    fn detonate_stasis_bomb(
        &mut self,
        attacker_id: EntityId,
        hold: &StasisHold,
        gameplay: &GameplayCatalog,
    ) {
        let Some(target) = self
            .units
            .get(hold.target_id)
            .filter(|target| target.hitpoints <= STASIS_BOMB_HITPOINTS)
        else {
            return;
        };
        let ground_zero = target.base.position;
        let Some(attacker) = self.units.get(attacker_id) else {
            return;
        };
        let proto_object_name = attacker.proto_object_name.clone();
        let attacker_player_id = attacker.base.player_id;
        let Some(linked_action) = gameplay
            .object(&proto_object_name)
            .and_then(|object| object.attack_profile(&hold.action_name))
            .and_then(|profile| profile.weapon_effects.as_ref())
            .and_then(|effects| effects.stasis.linked_action.clone())
        else {
            return;
        };
        let Some(bomb) = gameplay.stasis_bomb(&proto_object_name, &linked_action) else {
            return;
        };
        let _dealt = self.apply_attack_damage(
            &AttackDamage {
                attacker_id,
                attacker_player_id,
                primary_target_id: None,
                ground_zero,
                direction: Vec3::ZERO,
                damage: bomb.damage,
                weapon_type: bomb.weapon_type,
                area_damage: Some(bomb.area_damage),
                death_throw: None,
            },
            Some(gameplay),
        );
    }

    /// `BUnitActionStasis_update` of every unit with a stasis action.
    pub(super) fn update_unit_stasis_actions(&mut self, gameplay: &GameplayCatalog) {
        let unit_ids = self
            .units
            .iter()
            .filter_map(|(id, unit)| unit.weapon_effects.stasis_action.then_some(id))
            .collect::<Vec<_>>();
        for unit_id in unit_ids {
            if !self.update_unit_stasis_action(unit_id, gameplay)
                && let Some(unit) = self.units.get_mut(unit_id)
            {
                unit.weapon_effects.stasis_action = false;
            }
        }
    }

    /// Returns whether the stasis action stays.
    fn update_unit_stasis_action(&mut self, unit_id: EntityId, gameplay: &GameplayCatalog) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let Some(squad_id) = unit.squad_id.filter(|id| self.squads.get(*id).is_some()) else {
            return true;
        };
        let fields_to_stop = gameplay.stasis_fields_to_stop(&unit.proto_object_name);
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return true;
        };
        let effects = i32::from(squad.weapon_effects.stasis_effects);
        let multiplier = if effects >= fields_to_stop {
            0.0
        } else if effects > 0 {
            1.0 - effects.to_f32().unwrap_or(0.0) / fields_to_stop.to_f32().unwrap_or(1.0)
        } else {
            1.0
        };
        if multiplier.to_bits() == squad.weapon_effects.stasis_speed_multiplier.to_bits() {
            return true;
        }
        squad.weapon_effects.stasis_speed_multiplier = multiplier;
        if effects >= fields_to_stop {
            // A full stasis blocks the attacks, stops the squad and removes its actions.
            squad.weapon_effects.attack_blocked = true;
            squad.stop();
            squad.remove_all_orders();
            if squad.state == SquadState::Attacking {
                squad.state = SquadState::Idle;
            }
            let member_ids = squad.unit_ids.clone();
            self.stop_unit_firing(&member_ids);
        } else {
            squad.weapon_effects.attack_blocked = false;
        }
        effects > 0
    }

    /// The pending self heals of the stasis beams: `BSquad::repairHitpoints` without
    /// reinforcement.
    pub(super) fn apply_stasis_self_heals(&mut self, database: &Database) {
        let heals = self
            .squads
            .iter_mut()
            .filter_map(|(id, squad)| {
                let heal = std::mem::take(&mut squad.weapon_effects.pending_self_heal);
                (heal != 0.0).then_some((id, heal))
            })
            .collect::<Vec<_>>();
        for (squad_id, heal) in heals {
            let _excess = self.repair_squad_by_hitpoints(database, squad_id, heal, false);
        }
    }
}
