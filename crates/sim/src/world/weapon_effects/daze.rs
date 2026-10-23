//! Daze: the daze of `BDamageHelper_doDamageWithWeaponType` (`0x1401DE5D0`), `BSquad_addDaze`
//! (`0x1403A4260`), `BSquad_resetDaze` (`0x1403A43D0`) and `BSquadActionDaze`
//! (`BSquadActionDaze_update` `0x1403BA370`, `_daze` `0x1403BA3D0`, `_undaze` `0x1403BA670`).

use crate::entities::UnitDataScalar;
use crate::entities::squads::weapon_effects::SquadDaze;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, SmartTargetType};
use crate::world::World;
use crate::world::combat::WeaponEffectHit;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

/// `BSquadActionDaze_daze`: a modifier under this value immobilizes the squad.
const IMMOBILIZE_MODIFIER: f32 = 0.000_001;

impl World {
    /// The daze of one weapon hit on the squad of the receiving unit.
    pub(in crate::world) fn daze_on_hit(
        &mut self,
        hit: WeaponEffectHit<'_>,
        attacker_id: Option<EntityId>,
        target_id: EntityId,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(daze) = hit.profile.daze.as_ref() else {
            return;
        };
        let Some(target) = self.units.get(target_id) else {
            return;
        };
        let Some(squad_id) = target.squad_id.filter(|id| self.squads.get(*id).is_some()) else {
            return;
        };
        let type_matches = daze
            .target_type
            .as_deref()
            .is_some_and(|object_type| target.is_object_type(object_type));
        let already_dazed = hit
            .projectile
            .is_some_and(|projectile| projectile.target_already_dazed.get());
        if !type_matches || already_dazed {
            return;
        }
        // `mSmartTarget`: the attacker's tactic weapon has a smart-target type.
        let smart_target = hit.profile.smart_target.is_some()
            && attacker_id.is_some_and(|id| self.units.get(id).is_some());
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return;
        };
        let resist = gameplay.map_or(1.0, |catalog| catalog.daze_resist(&squad.proto_squad_name));
        let duration = daze.duration_ms * resist;
        if let Some(active) = squad.weapon_effects.daze.as_mut() {
            active.remaining_ms = daze_milliseconds(duration);
        } else if duration > 0.0 {
            squad.weapon_effects.daze = Some(SquadDaze {
                working: false,
                remaining_ms: daze_milliseconds(duration),
                movement_modifier: daze.movement_modifier,
                smart_target,
                effects: Vec::new(),
            });
        }
        if !daze.aoe
            && let Some(projectile) = hit.projectile
        {
            projectile.target_already_dazed.set(true);
            self.notify_projectiles_target_dazed(projectile.target_id);
        }
    }

    /// `cEventTargetDazed`: every projectile at a unit of the squad of `target_id` stops
    /// dazing.
    fn notify_projectiles_target_dazed(&mut self, target_id: EntityId) {
        let Some(squad_id) = self.units.get(target_id).and_then(|unit| unit.squad_id) else {
            return;
        };
        let units = &self.units;
        for (_, projectile) in self.projectiles.iter_mut() {
            if units
                .get(projectile.target_id)
                .is_some_and(|unit| unit.squad_id == Some(squad_id))
            {
                projectile.set_target_already_dazed();
            }
        }
    }

    /// Runs the daze action of every squad for one step.
    pub(super) fn update_dazes(
        &mut self,
        elapsed_ms: u32,
        database: Option<&Database>,
        gameplay: &GameplayCatalog,
    ) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| squad.weapon_effects.daze.is_some().then_some(id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let Some(daze) = self
                .squads
                .get_mut(squad_id)
                .and_then(|squad| squad.weapon_effects.daze.as_mut())
            else {
                continue;
            };
            if !daze.working {
                daze.working = true;
                self.daze_squad(squad_id, database, gameplay);
            } else if daze.remaining_ms <= elapsed_ms {
                daze.remaining_ms = 0;
                self.undaze_squad(squad_id);
            } else {
                daze.remaining_ms -= elapsed_ms;
            }
        }
    }

    /// `BSquadActionDaze_daze`.
    fn daze_squad(
        &mut self,
        squad_id: EntityId,
        database: Option<&Database>,
        gameplay: &GameplayCatalog,
    ) {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return;
        };
        let Some(modifier) = squad
            .weapon_effects
            .daze
            .as_ref()
            .map(|daze| daze.movement_modifier)
        else {
            return;
        };
        if modifier < IMMOBILIZE_MODIFIER {
            squad.weapon_effects.daze_immobilized = true;
        }
        squad.weapon_effects.attack_blocked = true;
        let unit_ids = squad.unit_ids.clone();
        if modifier >= IMMOBILIZE_MODIFIER {
            self.scale_member_velocity(&unit_ids, modifier);
        }
        let infantry = unit_ids
            .first()
            .and_then(|id| self.units.get(*id))
            .is_some_and(|leader| leader.is_object_type("Infantry"));
        let effect = gameplay.daze_effect(infantry).map(str::to_owned);
        let mut effects = Vec::with_capacity(unit_ids.len());
        for unit_id in unit_ids {
            let attachment = effect
                .as_deref()
                .and_then(|name| self.attach_effect_object(unit_id, name, database));
            effects.push(attachment);
        }
        if let Some(daze) = self
            .squads
            .get_mut(squad_id)
            .and_then(|squad| squad.weapon_effects.daze.as_mut())
        {
            daze.effects = effects;
        }
    }

    /// `BSquadActionDaze_undaze` and the disconnect of the finished action.
    fn undaze_squad(&mut self, squad_id: EntityId) {
        let Some(squad) = self.squads.get_mut(squad_id) else {
            return;
        };
        let Some(daze) = squad.weapon_effects.daze.take() else {
            return;
        };
        if daze.movement_modifier < IMMOBILIZE_MODIFIER {
            squad.weapon_effects.daze_immobilized = false;
        }
        squad.weapon_effects.attack_blocked = false;
        if daze.smart_target {
            let count = &mut squad.weapon_effects.smart_targets[SmartTargetType::Daze as usize];
            *count = count.saturating_sub(1);
        }
        let unit_ids = squad.unit_ids.clone();
        if daze.movement_modifier >= IMMOBILIZE_MODIFIER {
            self.scale_member_velocity(&unit_ids, daze.movement_modifier.recip());
        }
        for effect in daze.effects {
            self.remove_effect_object(effect);
        }
    }

    /// `BSquad_setAbilityMovementSpeedModifier`: scale the velocity of each member unit.
    fn scale_member_velocity(&mut self, unit_ids: &[EntityId], factor: f32) {
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(*unit_id) {
                unit.adjust_data_scalar(UnitDataScalar::Velocity, factor);
            }
        }
    }
}

/// `(int)duration`, stored as the `DWORD` `mDazedDuration`.
fn daze_milliseconds(duration: f32) -> u32 {
    u32::from_ne_bytes(duration.to_i32().unwrap_or(i32::MIN).to_ne_bytes())
}
