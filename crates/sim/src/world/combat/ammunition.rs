//! Retail ammunition gates shared by tactic selection and attack execution.

use crate::entities::Unit;
use crate::gameplay::AttackProfile;
use crate::player::PlayerTechState;

pub(super) fn can_select(
    unit: &Unit,
    profile: Option<&AttackProfile>,
    technologies: Option<&PlayerTechState>,
) -> bool {
    let Some(profile) = profile else {
        return true;
    };
    if !profile.ammunition.is_used() {
        return true;
    }
    let continuing_cycle = unit
        .combat
        .action_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(&profile.action_name))
        && (unit.combat.is_animating() || unit.combat.is_reloading());
    continuing_cycle
        || unit.ammunition.has_full_attack(
            profile.maximum_attacks_per_animation(),
            effective_damage(unit, profile, technologies),
        )
}

pub(super) fn effective_damage(
    unit: &Unit,
    profile: &AttackProfile,
    technologies: Option<&PlayerTechState>,
) -> f32 {
    technologies.map_or(profile.damage_per_attack, |state| {
        state.weapon_damage(
            &unit.proto_object_name,
            &profile.weapon_name,
            profile.damage_per_attack,
        )
    })
}
