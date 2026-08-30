//! Ground-position variant of retail's shared ranged attack executor.

use super::helpers::{scaled_launch_damage, selected_range, xz_distance_squared};
use super::{ConcreteTargetSnapshot, FireEvent, World, hardpoints};
use crate::entity_id::EntityId;
use crate::gameplay::{AttackAnimationEventKind, GameplayCatalog};
use glam::Vec3;

/// Result of advancing one unit's attack against a ground position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::world) enum PositionAttackStatus {
    /// No operational, position-compatible ranged action could be selected.
    Unavailable,
    /// The action exists but has not completed one attack cycle yet.
    Waiting,
    /// At least one attack cycle completed during this substep.
    Completed,
}

impl World {
    pub(in crate::world) fn advance_position_attack(
        &mut self,
        dt: f32,
        attacker_id: EntityId,
        target_position: Vec3,
        gameplay: &GameplayCatalog,
    ) -> PositionAttackStatus {
        if self.unblocked_attacker_player(attacker_id).is_none() {
            return PositionAttackStatus::Waiting;
        }
        let target = position_target(target_position);
        let Some(profile) = self.selected_unit_attack_profile(attacker_id, &target, gameplay)
        else {
            self.stop_unit_firing(&[attacker_id]);
            return PositionAttackStatus::Unavailable;
        };
        let Some(attacker) = self.attacker_snapshot(attacker_id, profile) else {
            self.stop_unit_firing(&[attacker_id]);
            return PositionAttackStatus::Unavailable;
        };
        let range = selected_range(0.0, attacker.authored_range, attacker.range_scalar);
        if xz_distance_squared(attacker.position, target.position) > range * range {
            self.stop_unit_firing(&[attacker_id]);
            return PositionAttackStatus::Waiting;
        }
        let authored_damage = self.authored_attack_damage(attacker_id, &attacker, profile);
        let advance = {
            let (units, rng) = (&mut self.units, &mut self.rng);
            let Some(unit) = units.get_mut(attacker_id) else {
                return PositionAttackStatus::Unavailable;
            };
            unit.combat.prepare_position(profile, target.position);
            unit.combat.request_charged_cycle(false);
            if !hardpoints::orient_for_attack(unit, profile, target.position, 0.0, false, dt) {
                return PositionAttackStatus::Waiting;
            }
            let (combat, unit_ammunition) = (&mut unit.combat, &mut unit.ammunition);
            combat.advance_position(
                dt,
                target.position,
                profile,
                unit_ammunition,
                authored_damage,
                rng,
            )
        };
        let damage = scaled_launch_damage(
            authored_damage,
            attacker.damage_multiplier,
            attacker.position,
            target.position,
            profile.uses_height_bonus_damage,
            gameplay.height_bonus_damage(),
        );
        let area_damage = self.launch_area_damage(
            attacker.player_id,
            &attacker.logical_proto_object_name,
            profile,
        );
        let mut tuning = self.launch_tuning(&attacker, profile);
        tuning.max_range = range;
        let fire_events = advance.events.iter().filter_map(|occurrence| {
            if !matches!(
                occurrence.event.kind,
                AttackAnimationEventKind::Attack { .. }
            ) {
                return None;
            }
            let mut launch_attacker = attacker.clone();
            launch_attacker.launch_position = self.attack_event_launch_position(
                attacker_id,
                attacker.launch_position,
                occurrence.event.anchor.as_ref(),
            );
            let mut event = FireEvent::from_attack(
                attacker_id,
                &launch_attacker,
                target.clone(),
                damage,
                profile,
                area_damage,
                range,
            )
            .with_launch_tuning(tuning);
            event.pull = None;
            event.collides_with_all_units = true;
            event.targets_foot_of_unit = true;
            Some(event)
        });
        self.resolve_fire_events(fire_events.collect(), gameplay);
        if advance.completed_cycles > 0 {
            PositionAttackStatus::Completed
        } else {
            PositionAttackStatus::Waiting
        }
    }
}

fn position_target(position: Vec3) -> ConcreteTargetSnapshot {
    ConcreteTargetSnapshot {
        id: EntityId::INVALID,
        player_id: 0,
        position,
        aim_position: position,
        velocity: Vec3::ZERO,
        collision_radius: 0.0,
        proto_object_name: String::new(),
        damaged: false,
        unbuilt: false,
        in_cover: false,
    }
}
