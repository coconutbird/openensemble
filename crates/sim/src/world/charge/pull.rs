//! Target-side quadratic squad flight created by a charged pull impact.

use super::super::World;
use crate::entities::squads::{PullMemberPlan, formation_offset_to_world};
use crate::entities::{SquadState, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::PullAttackProfile;
use glam::Vec3;
use num_traits::ToPrimitive;

const LANDING_RANGE_ADJUSTMENT: f32 = 0.5;
const LANDING_RANGE_ADJUSTMENT_THRESHOLD: f32 = 1.0;
const MIN_DIRECTION_SQUARED: f32 = 0.000_001;

impl World {
    pub(in crate::world) fn begin_charged_squad_pull(
        &mut self,
        attacker_id: EntityId,
        target_unit_id: EntityId,
        action_name: &str,
        pull: &PullAttackProfile,
        normal_attack_range: f32,
    ) -> bool {
        let Some(target_squad_id) = self
            .units
            .get(target_unit_id)
            .and_then(|unit| unit.squad_id)
        else {
            return false;
        };
        let Some(attacker) = self.units.get(attacker_id) else {
            return false;
        };
        let attacker_position = attacker.base.position;
        let attacker_radius = attacker.obstruction_radius();
        let Some((target_average, target_radius)) = self.pull_target_geometry(target_squad_id)
        else {
            return false;
        };
        let mut landing_distance = attacker_radius + target_radius + normal_attack_range.max(0.0);
        if normal_attack_range > LANDING_RANGE_ADJUSTMENT_THRESHOLD {
            landing_distance -= LANDING_RANGE_ADJUSTMENT;
        }
        let direction = Vec3::new(
            target_average.x - attacker_position.x,
            0.0,
            target_average.z - attacker_position.z,
        );
        if direction.length_squared() <= MIN_DIRECTION_SQUARED
            || direction.length() <= landing_distance * 1.1
        {
            return false;
        }
        let mut target_anchor = attacker_position + direction.normalize() * landing_distance;
        target_anchor = self.clamp_pull_target(target_anchor);
        if let Some(height) = self.terrain_height(target_anchor, true) {
            target_anchor.y = height;
        }

        let Some((forward, member_ids)) = self
            .squads
            .get(target_squad_id)
            .map(|squad| (squad.base.forward, squad.unit_ids.clone()))
        else {
            return false;
        };
        let mut maximum_distance = 0.0f32;
        let plans = member_ids
            .iter()
            .filter_map(|unit_id| {
                let unit = self.units.get(*unit_id)?;
                let end = target_anchor + formation_offset_to_world(forward, unit.formation_offset);
                let delta = end - unit.base.position;
                maximum_distance = maximum_distance.max(Vec3::new(delta.x, 0.0, delta.z).length());
                Some(PullMemberPlan::new(*unit_id, unit.base.position, end))
            })
            .collect::<Vec<_>>();
        let started = self.squads.get_mut(target_squad_id).is_some_and(|squad| {
            squad.remove_all_orders();
            if !squad.pull.begin(
                attacker_id,
                action_name,
                target_anchor,
                pull.velocity_scalar,
                pull.end_animation_type.as_deref(),
                plans,
            ) {
                return false;
            }
            squad.base.position = target_anchor;
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Working;
            true
        });
        if !started {
            return false;
        }

        let duration_ms = pull_duration_ms(maximum_distance, pull.velocity_scalar);
        for unit_id in member_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.clear_attack_order();
                unit.stop();
                unit.state = UnitState::Idle;
                unit.set_jump_pull_untargetable(true);
            }
            if let Some(animation) = pull.end_animation_type.as_deref() {
                let _played =
                    self.play_entity_animation(unit_id, animation.to_owned(), None, duration_ms);
            }
        }
        true
    }

    pub(in crate::world) fn update_squad_pulls(&mut self, dt: f32) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| squad.is_being_pulled().then_some(id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.advance_squad_pull(squad_id, dt);
        }
    }

    fn advance_squad_pull(&mut self, squad_id: EntityId, dt: f32) {
        let advance = match self.squads.get_mut(squad_id) {
            Some(squad) if squad.is_alive() => squad.pull.advance(dt),
            Some(squad) => {
                squad.pull.cancel();
                return;
            }
            None => return,
        };
        for (unit_id, position) in advance.positions {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            let previous = unit.base.position;
            unit.base.position = position;
            unit.base.velocity = if dt > 0.0 {
                (position - previous) / dt
            } else {
                Vec3::ZERO
            };
            unit.move_target = None;
            unit.state = UnitState::Idle;
        }
        self.sync_pulled_squad_origin(squad_id, dt);
        if advance.complete {
            self.finish_squad_pull(squad_id);
        }
    }

    fn finish_squad_pull(&mut self, squad_id: EntityId) {
        let member_ids = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        for unit_id in member_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.base.velocity = Vec3::ZERO;
                unit.move_target = None;
                unit.state = UnitState::Idle;
                unit.set_jump_pull_untargetable(false);
            }
            let _played = self.play_entity_animation(unit_id, "Idle".to_owned(), None, 0);
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.pull.cancel();
            squad.base.velocity = Vec3::ZERO;
            squad.move_target = None;
            if squad.is_alive() {
                squad.state = SquadState::Idle;
            }
        }
    }

    fn sync_pulled_squad_origin(&mut self, squad_id: EntityId, dt: f32) {
        let Some(average) = self.pull_member_average(squad_id) else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            let previous = squad.base.position;
            squad.base.position = average;
            squad.base.velocity = if dt > 0.0 {
                (average - previous) / dt
            } else {
                Vec3::ZERO
            };
        }
    }

    fn pull_target_geometry(&self, squad_id: EntityId) -> Option<(Vec3, f32)> {
        let average = self.pull_member_average(squad_id)?;
        let radius = self
            .squads
            .get(squad_id)?
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .map(|unit| {
                let delta = unit.base.position - average;
                Vec3::new(delta.x, 0.0, delta.z).length() + unit.obstruction_radius()
            })
            .fold(0.0, f32::max);
        Some((average, radius))
    }

    fn pull_member_average(&self, squad_id: EntityId) -> Option<Vec3> {
        let squad = self.squads.get(squad_id)?;
        let mut sum = Vec3::ZERO;
        let mut count = 0u32;
        for unit in squad.unit_ids.iter().filter_map(|id| self.units.get(*id)) {
            sum += unit.base.position;
            count = count.saturating_add(1);
        }
        (count > 0).then(|| sum / count.to_f32().unwrap_or(1.0))
    }

    fn clamp_pull_target(&self, mut target: Vec3) -> Vec3 {
        let Some(bounds) = self.effective_playable_bounds() else {
            return target;
        };
        let minimum_x = (bounds.min_x() + 1.0).min(bounds.max_x());
        let minimum_z = (bounds.min_z() + 1.0).min(bounds.max_z());
        let maximum_x = (bounds.max_x() - 1.0).max(bounds.min_x());
        let maximum_z = (bounds.max_z() - 1.0).max(bounds.min_z());
        target.x = target.x.clamp(minimum_x, maximum_x);
        target.z = target.z.clamp(minimum_z, maximum_z);
        target
    }
}

fn pull_duration_ms(distance: f32, velocity: f32) -> u32 {
    if !(distance.is_finite() && velocity.is_finite() && velocity > 0.0) {
        return 0;
    }
    (distance / velocity * 1_000.0)
        .ceil()
        .to_u32()
        .unwrap_or(u32::MAX)
}
