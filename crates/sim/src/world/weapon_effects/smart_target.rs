//! Smart targeting of stasis and daze weapons: `BSquad_chooseSmartTarget` (`0x1403A3C20`),
//! `BSquad_incrementSmartTargetReference` (`0x1403A41B0`) and the release of
//! `BSquadActionAttack_disconnect` (`0x1403B1920`).

use crate::entities::SquadState;
use crate::entities::squads::weapon_effects::SmartTargetChoice;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AbilityGameplay, GameplayCatalog, SmartTargetType};
use crate::world::World;

impl World {
    /// Runs the smart-target choice of every squad attack that starts, and releases the
    /// reference of every squad attack that ended or changed its target.
    ///
    /// Retail chooses in `BSquad::doAttack` when it makes the squad attack action. The sim
    /// has no squad attack action, so the choice runs once for each new attack target.
    pub(in crate::world) fn update_smart_targets(&mut self, gameplay: &GameplayCatalog) {
        let squad_ids = self.squads.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for squad_id in squad_ids {
            let Some(squad) = self.squads.get(squad_id) else {
                continue;
            };
            let current = squad
                .attack_target
                .filter(|_| squad.is_alive() && squad.state == SquadState::Attacking);
            let choice = squad.weapon_effects.smart_target_choice;
            if choice.is_some_and(|choice| Some(choice.target_id) == current) {
                continue;
            }
            if let Some(choice) = choice {
                self.release_smart_target_reference(choice);
            }
            let choice =
                current.map(|target_id| self.choose_smart_target(squad_id, target_id, gameplay));
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.weapon_effects.smart_target_choice = choice;
            }
        }
    }

    /// `BSquadActionAttack_disconnect`: the attack gives back its reference. Retail
    /// decrements for every attack of a smart-target weapon, also when the increment
    /// failed, and stops at zero.
    pub(in crate::world) fn release_smart_target_reference(&mut self, choice: SmartTargetChoice) {
        let Some(kind) = choice.claim else {
            return;
        };
        if let Some(target) = self.squads.get_mut(choice.target_id) {
            let count = &mut target.weapon_effects.smart_targets[kind as usize];
            *count = count.saturating_sub(1);
        }
    }

    /// `BSquad_chooseSmartTarget`: keep the target if it takes another reference, else
    /// move the attack to the first enemy squad near it that does.
    fn choose_smart_target(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> SmartTargetChoice {
        let unclaimed = SmartTargetChoice {
            target_id,
            claim: None,
        };
        let Some(leader_id) = self.squad_leader_id(squad_id) else {
            return unclaimed;
        };
        let Some(kind) = self
            .selected_attack_weapon_effects(leader_id, target_id, gameplay)
            .and_then(|effects| effects.smart_target)
        else {
            return unclaimed;
        };
        // A target that is not a squad needs the air branch (`targetsAir`), which the sim
        // does not model.
        if self.squads.get(target_id).is_none() {
            return unclaimed;
        }
        let claimed = SmartTargetChoice {
            target_id,
            claim: Some(kind),
        };
        let Some(target_leader_id) = self.squad_leader_id(target_id) else {
            return claimed;
        };
        if self.increment_smart_target_reference(target_id, kind, gameplay) {
            return claimed;
        }
        let (chosen_id, redundant) =
            self.find_smart_target(squad_id, target_id, target_leader_id, kind, gameplay);
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.attack_target = Some(chosen_id);
            if redundant {
                // `invalidateAbilityID`: revert to the normal attack.
                squad.attack_ability_id = None;
            }
        }
        SmartTargetChoice {
            target_id: chosen_id,
            claim: Some(kind),
        }
    }

    /// The `BUnitQuery` of `BSquad_chooseSmartTarget`: the squads with a live unit that the
    /// player sees, of an enemy, within the `SmartTargetRange` of the command ability of
    /// the squad around the target, and of one of the object types of the target leader.
    /// Returns the new target and whether every candidate was full. The sim visits the
    /// candidates in squad ID order.
    fn find_smart_target(
        &mut self,
        squad_id: EntityId,
        target_id: EntityId,
        target_leader_id: EntityId,
        kind: SmartTargetType,
        gameplay: &GameplayCatalog,
    ) -> (EntityId, bool) {
        let candidates =
            self.smart_target_candidates(squad_id, target_id, target_leader_id, gameplay);
        let mut chosen = (target_id, false);
        for candidate_id in candidates {
            if self.increment_smart_target_reference(candidate_id, kind, gameplay) {
                return (candidate_id, false);
            }
            chosen = (candidate_id, true);
        }
        chosen
    }

    fn smart_target_candidates(
        &self,
        squad_id: EntityId,
        target_id: EntityId,
        target_leader_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Vec<EntityId> {
        let (Some(squad), Some(target), Some(target_leader)) = (
            self.squads.get(squad_id),
            self.squads.get(target_id),
            self.units.get(target_leader_id),
        ) else {
            return Vec::new();
        };
        let player_id = squad.base.player_id;
        let range = self.smart_target_range(squad_id, gameplay);
        let range_squared = range * range;
        let center = target.base.position;
        let mut types = self.prototype_declared_object_types(&target_leader.proto_object_name);
        types.push(target_leader.proto_object_name.clone());
        self.squads
            .iter()
            .filter(|(_, candidate)| {
                candidate.unit_ids.iter().any(|&unit_id| {
                    self.units.get(unit_id).is_some_and(|unit| {
                        let offset = unit.base.position - center;
                        unit.is_alive()
                            && offset.x * offset.x + offset.z * offset.z <= range_squared
                            && self.players_are_enemies(player_id, unit.base.player_id)
                            && self.is_entity_visible_from_player(player_id, unit_id)
                            && !self.entity_hidden_by_cloak_from_player(player_id, unit_id)
                            && types.iter().any(|object_type| {
                                self.prototype_name_is_object_type(
                                    &unit.proto_object_name,
                                    object_type,
                                )
                            })
                    })
                })
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// `gDatabase.getSquadAbilityID(this, AIDCommand)`: the `SmartTargetRange` of the
    /// command ability of the squad leader, or 0 without one.
    fn smart_target_range(&self, squad_id: EntityId, gameplay: &GameplayCatalog) -> f32 {
        let Some(leader) = self
            .squad_leader_id(squad_id)
            .and_then(|id| self.units.get(id))
        else {
            return 0.0;
        };
        gameplay
            .command_ability_id()
            .and_then(|id| gameplay.resolve_order_ability(&leader.proto_object_name, id))
            .map_or(0.0, AbilityGameplay::smart_target_range)
    }

    /// `BSquad_incrementSmartTargetReference`: a stasis target takes up to the
    /// `NumStasisFieldsToStop` of its leader (1 without a leader), a daze target one.
    fn increment_smart_target_reference(
        &mut self,
        squad_id: EntityId,
        kind: SmartTargetType,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let maximum = match kind {
            SmartTargetType::Stasis => self
                .squad_leader_id(squad_id)
                .and_then(|id| self.units.get(id))
                .map_or(1, |leader| {
                    gameplay.stasis_fields_to_stop(&leader.proto_object_name)
                }),
            SmartTargetType::Daze => 1,
        };
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return false;
        };
        let count = &mut squad.weapon_effects.smart_targets[kind as usize];
        if i32::from(*count) >= maximum {
            return false;
        }
        *count = count.saturating_add(1);
        true
    }

    /// `BSquad::getLeaderUnit`: the first live unit of the squad.
    fn squad_leader_id(&self, squad_id: EntityId) -> Option<EntityId> {
        self.squads
            .get(squad_id)?
            .unit_ids
            .iter()
            .copied()
            .find(|&unit_id| self.units.get(unit_id).is_some_and(Entity::is_alive))
    }
}
