//! The weapon effect families in the world: damage over time (`BUnitActionDOT`), daze
//! (`BSquadActionDaze`) and stasis (`BUnitActionStasis` and the stasis of
//! `BUnitActionRangedAttack`).

use super::World;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

mod daze;
mod dot;
mod smart_target;
mod stasis;

#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod tests;

/// One fired attack of a weapon with a stasis flag (`BUnitActionRangedAttack_fireWeapon`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StasisFire {
    /// The attacking unit.
    pub source_id: EntityId,
    /// The target unit of the attack.
    pub target_id: EntityId,
    /// The attack action of the fired weapon.
    pub action_name: String,
}

impl World {
    /// Runs the weapon effect actions for one entity step: the stasis holds of the ranged
    /// attacks, the stasis actions, the damage over time actions, the daze actions and the
    /// stasis self heals.
    pub(super) fn update_weapon_effects(
        &mut self,
        dt: f32,
        database: Option<&Database>,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(gameplay) = gameplay else {
            return;
        };
        self.update_stasis_holds(gameplay);
        self.update_unit_stasis_actions(gameplay);
        self.update_damage_over_time(dt, database, gameplay);
        self.update_dazes(elapsed_milliseconds(dt), database, gameplay);
        if let Some(database) = database {
            self.apply_stasis_self_heals(database);
        }
    }

    /// Attach a visual effect object to a unit, or `None` without a database or prototype.
    fn attach_effect_object(
        &mut self,
        unit_id: EntityId,
        name: &str,
        database: Option<&Database>,
    ) -> Option<EntityId> {
        let prototype_id = crate::spawn::object_prototype_id(database?, name)?;
        self.add_visual_attachment_to_unit(unit_id, prototype_id, name)
    }

    fn remove_effect_object(&mut self, effect_id: Option<EntityId>) {
        if let Some(effect_id) = effect_id {
            self.detach_attachment_from_parent(effect_id);
            let _removed = self.remove_object(effect_id);
        }
    }
}

/// `gWorld->getLastUpdateLength()`: the step length in whole milliseconds.
fn elapsed_milliseconds(dt: f32) -> u32 {
    if !dt.is_finite() || dt <= 0.0 {
        return 0;
    }
    (dt * 1_000.0)
        .round_ties_even()
        .to_u32()
        .unwrap_or(u32::MAX)
}
