//! Damage over time: `BUnit_applyDamageOverTimeEffect` (`0x1404D7170`) and
//! `BUnitActionDOT_update` (`0x1404EC550`).

use crate::entities::units::weapon_effects::{DotAction, DotStack};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{DotEffectSizes, GameplayCatalog};
use crate::player::PlayerId;
use crate::world::World;
use crate::world::combat::WeaponEffectHit;
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

impl World {
    /// `BUnit_applyDamageOverTimeEffect`: add or refresh the damage over time of a hit on
    /// the receiving unit. `damage_multiplier` is the weapon-type modifier of the hit.
    pub(in crate::world) fn add_damage_over_time(
        &mut self,
        hit: Option<WeaponEffectHit<'_>>,
        source: (Option<EntityId>, PlayerId),
        target_id: EntityId,
        damage_multiplier: f32,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(dot) = hit.and_then(|hit| hit.profile.damage_over_time.as_ref()) else {
            return;
        };
        let sizes =
            gameplay.map_or_else(DotEffectSizes::default, GameplayCatalog::dot_effect_sizes);
        let stack = DotStack {
            source_id: source.0,
            source_player_id: source.1,
            source_team_id: self.get_player(source.1).map_or(0, |player| player.team_id),
            end_time_ms: dot_end_time(self.game_time_ms, dot.duration),
        };
        let Some(unit) = self.units.get_mut(target_id).filter(|unit| unit.is_alive()) else {
            return;
        };
        let effect = dot.effect.for_radius(unit.obstruction_radius(), sizes);
        add_stack(
            &mut unit.weapon_effects.dot_actions,
            stack,
            effect,
            dot.rate,
            damage_multiplier,
        );
    }

    /// Runs every damage over time action of the live units for one step.
    pub(super) fn update_damage_over_time(
        &mut self,
        dt: f32,
        database: Option<&Database>,
        gameplay: &GameplayCatalog,
    ) {
        let unit_ids = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (unit.is_alive() && !unit.weapon_effects.dot_actions.is_empty()).then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in unit_ids {
            let count = self
                .units
                .get(unit_id)
                .map_or(0, |unit| unit.weapon_effects.dot_actions.len());
            for index in 0..count {
                self.update_dot_action(unit_id, index, dt, database, gameplay);
            }
            self.remove_finished_dot_actions(unit_id);
        }
    }

    fn update_dot_action(
        &mut self,
        unit_id: EntityId,
        index: usize,
        dt: f32,
        database: Option<&Database>,
        gameplay: &GameplayCatalog,
    ) {
        let Some(action) = self
            .units
            .get(unit_id)
            .and_then(|unit| unit.weapon_effects.dot_actions.get(index))
        else {
            return;
        };
        if !action.working {
            // `cStateNone`: create the effect object and start the work without damage.
            let effect = action
                .effect
                .clone()
                .filter(|_| action.effect_attachment.is_none());
            let attachment =
                effect.and_then(|name| self.attach_effect_object(unit_id, &name, database));
            if let Some(action) = self.dot_action_mut(unit_id, index) {
                action.effect_attachment = action.effect_attachment.or(attachment);
                action.working = true;
            }
            return;
        }
        let damage = action.rate * dt * action.damage_multiplier;
        let mut stack_index = action.stacks.len();
        while stack_index > 0 {
            stack_index -= 1;
            let Some(stack) = self
                .units
                .get(unit_id)
                .and_then(|unit| unit.weapon_effects.dot_actions.get(index))
                .and_then(|action| action.stacks.get(stack_index))
                .copied()
            else {
                continue;
            };
            let _dealt = self.apply_damage_over_time_tick(
                (stack.source_id, stack.source_player_id),
                unit_id,
                damage,
                Some(gameplay),
            );
            if self.game_time_ms >= stack.end_time_ms
                && let Some(action) = self.dot_action_mut(unit_id, index)
                && stack_index < action.stacks.len()
            {
                action.stacks.remove(stack_index);
            }
        }
    }

    fn dot_action_mut(&mut self, unit_id: EntityId, index: usize) -> Option<&mut DotAction> {
        self.units
            .get_mut(unit_id)?
            .weapon_effects
            .dot_actions
            .get_mut(index)
    }

    /// `cStateDone`: an action without stacks ends and removes its effect object.
    fn remove_finished_dot_actions(&mut self, unit_id: EntityId) {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        let mut effects = Vec::new();
        unit.weapon_effects.dot_actions.retain(|action| {
            let done = action.working && action.stacks.is_empty();
            if done {
                effects.push(action.effect_attachment);
            }
            !done
        });
        for effect in effects {
            self.remove_effect_object(effect);
        }
    }
}

/// The retail end time `(int)((float)gametime + DOTduration * 1000)` as a `DWORD`.
pub(super) fn dot_end_time(game_time_ms: u32, duration: f32) -> u32 {
    let end = game_time_ms.to_f32().unwrap_or(f32::MAX) + duration * 1_000.0;
    u32::from_ne_bytes(end.to_i32().unwrap_or(i32::MIN).to_ne_bytes())
}

/// Retail adds a stack to the first damage over time action of the same source, else to
/// every action of the same effect, rate and multiplier, else to a new action.
fn add_stack(
    actions: &mut Vec<DotAction>,
    stack: DotStack,
    effect: Option<&str>,
    rate: f32,
    damage_multiplier: f32,
) {
    let mut added = false;
    for action in actions.iter_mut() {
        if action.has_source(stack.source_id) {
            // The refresh needs the same source and team; either way the hit ends here.
            if let Some(existing) = action.stacks.iter_mut().find(|existing| {
                existing.source_id == stack.source_id
                    && existing.source_team_id == stack.source_team_id
            }) {
                existing.end_time_ms = stack.end_time_ms;
            }
            return;
        }
        if action.matches(effect, rate, damage_multiplier) {
            action.stacks.push(stack);
            added = true;
        }
    }
    if !added {
        actions.push(DotAction {
            working: false,
            effect: effect.map(str::to_owned),
            effect_attachment: None,
            damage_multiplier,
            rate,
            stacks: vec![stack],
        });
    }
}
