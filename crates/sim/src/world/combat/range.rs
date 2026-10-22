//! Retail attack-range intervals and combat-order movement decisions.

use super::{AttackerSnapshot, CombatMotion, ConcreteTargetSnapshot, TargetSnapshot, World};
use crate::EntityId;
use crate::entities::{Squad, SquadMode, Unit};
use crate::gameplay::GameplayCatalog;
use glam::Vec3;

const RANGE_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AttackRange {
    min: f32,
    max: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttackRangeRelation {
    InsideMinimum,
    Within,
    BeyondMaximum,
}

impl AttackRange {
    pub(super) fn new(min: f32, max: f32) -> Self {
        Self {
            min: finite_nonnegative(min),
            max: finite_nonnegative(max),
        }
    }

    fn with_max(self, max: f32) -> Self {
        Self::new(self.min, max)
    }

    pub(super) fn relation(
        self,
        source_position: Vec3,
        source_radius: f32,
        target_position: Vec3,
        target_radius: f32,
    ) -> AttackRangeRelation {
        let center_distance = planar_distance(source_position, target_position);
        let source_radius = finite_nonnegative(source_radius);
        let target_radius = finite_nonnegative(target_radius);
        let minimum_distance = (center_distance - source_radius).max(0.0);
        if self.min > RANGE_EPSILON && minimum_distance < self.min - RANGE_EPSILON {
            return AttackRangeRelation::InsideMinimum;
        }
        let maximum_distance = (minimum_distance - target_radius).max(0.0);
        if maximum_distance <= self.max + RANGE_EPSILON {
            AttackRangeRelation::Within
        } else {
            AttackRangeRelation::BeyondMaximum
        }
    }
}

pub(super) fn attack_profile_range_relation(
    attacker: &AttackerSnapshot,
    target: &ConcreteTargetSnapshot,
    min_range: f32,
    max_range: f32,
) -> AttackRangeRelation {
    AttackRange::new(min_range, max_range).relation(
        attacker.position,
        attacker.obstruction_radius,
        target.position,
        target.collision_radius,
    )
}

impl World {
    pub(super) fn squad_combat_motion(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> CombatMotion {
        let Some(squad) = self.squads.get(squad_id) else {
            return CombatMotion::Clear;
        };
        if self.is_squad_incapacitated(squad_id) {
            return CombatMotion::Clear;
        }
        let Some(target_id) = squad.attack_target else {
            return CombatMotion::Clear;
        };
        let Some(target) = self.attack_target_snapshot(squad.base.player_id, target_id) else {
            return CombatMotion::Clear;
        };
        if !self.players_are_enemies(squad.base.player_id, target.player_id)
            || attack_exceeds_leash(squad, target)
        {
            return CombatMotion::Clear;
        }
        let Some(concrete_target) = self.concrete_attack_target(squad.base.player_id, target.id)
        else {
            return CombatMotion::Clear;
        };
        let range = self.squad_attack_range(squad, &concrete_target, gameplay, None);
        combat_motion(
            squad.base.position,
            self.squad_obstruction_radius(squad_id),
            target,
            range,
        )
    }

    pub(super) fn unit_combat_motion(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> CombatMotion {
        let Some(unit) = self.units.get(unit_id).filter(|unit| unit.is_operational()) else {
            return CombatMotion::Clear;
        };
        let Some(target_id) = unit.attack_target else {
            return CombatMotion::Clear;
        };
        let Some(target) = self.attack_target_snapshot(unit.base.player_id, target_id) else {
            return CombatMotion::Clear;
        };
        if !self.players_are_enemies(unit.base.player_id, target.player_id) {
            return CombatMotion::Clear;
        }
        let Some(concrete_target) = self.concrete_attack_target(unit.base.player_id, target.id)
        else {
            return CombatMotion::Clear;
        };
        let range = self.unit_attack_range(unit, &concrete_target, gameplay);
        combat_motion(unit.base.position, unit.obstruction_radius(), target, range)
    }

    pub(in crate::world) fn squad_attack_target_inside_minimum_for_mode(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
        mode: SquadMode,
    ) -> bool {
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        let Some(target_id) = squad.attack_target else {
            return false;
        };
        let Some(target) = self.attack_target_snapshot(squad.base.player_id, target_id) else {
            return false;
        };
        let Some(concrete_target) = self.concrete_attack_target(squad.base.player_id, target.id)
        else {
            return false;
        };
        let Some(range) = self.squad_tactic_range(squad, &concrete_target, gameplay, Some(mode))
        else {
            return false;
        };
        range.relation(
            squad.base.position,
            self.squad_obstruction_radius(squad_id),
            target.position,
            target.obstruction_radius,
        ) == AttackRangeRelation::InsideMinimum
    }

    pub(super) fn squad_can_auto_attack_target(
        &self,
        squad: &Squad,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let source_radius = self.squad_obstruction_radius(squad.base.id);
        squad.unit_ids.iter().any(|&unit_id| {
            let Some(unit) = self.units.get(unit_id) else {
                return false;
            };
            let Some(range) = self.unit_tactic_range(unit, target, gameplay, true, None) else {
                return false;
            };
            range.relation(
                squad.base.position,
                source_radius,
                target.position,
                target.collision_radius,
            ) != AttackRangeRelation::InsideMinimum
        })
    }

    fn squad_attack_range(
        &self,
        squad: &Squad,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
        mode_override: Option<SquadMode>,
    ) -> Option<AttackRange> {
        let range = self
            .collision_attack_range(squad, gameplay)
            .map(|max| AttackRange::new(0.0, max))
            .or_else(|| self.squad_tactic_range(squad, target, gameplay, mode_override));
        if squad.attack_range > 0.0 {
            Some(
                range
                    .unwrap_or_else(|| AttackRange::new(0.0, squad.attack_range))
                    .with_max(squad.attack_range),
            )
        } else {
            range
        }
    }

    fn unit_attack_range(
        &self,
        unit: &Unit,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) -> Option<AttackRange> {
        let range = self.unit_tactic_range(unit, target, gameplay, false, None);
        if unit.attack_range > 0.0 {
            Some(
                range
                    .unwrap_or_else(|| AttackRange::new(0.0, unit.attack_range))
                    .with_max(unit.attack_range),
            )
        } else {
            range
        }
    }

    fn squad_tactic_range(
        &self,
        squad: &Squad,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
        mode_override: Option<SquadMode>,
    ) -> Option<AttackRange> {
        let automatic = squad.is_auto_attack_engagement();
        squad
            .unit_ids
            .iter()
            .filter_map(|&unit_id| self.units.get(unit_id))
            .filter_map(|unit| {
                self.unit_tactic_range(unit, target, gameplay, automatic, mode_override)
            })
            .min_by(|left, right| left.max.total_cmp(&right.max))
    }

    fn unit_tactic_range(
        &self,
        unit: &Unit,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
        automatic: bool,
        mode_override: Option<SquadMode>,
    ) -> Option<AttackRange> {
        let action =
            self.selected_ranged_action_for_mode(unit, target, gameplay, automatic, mode_override)?;
        let authored_max = action
            .weapon
            .max_range
            .filter(|range| range.is_finite() && *range >= 0.0)?;
        let maximum = self
            .get_player(unit.base.player_id)
            .map_or(authored_max, |player| {
                player.technologies.weapon_range(
                    unit.logical_proto_object_name(),
                    &action.weapon.name,
                    authored_max,
                )
            });
        let maximum =
            self.charged_action_range(unit, target, &action.action.name, maximum, gameplay)
                * unit.weapon_range_scalar;
        Some(AttackRange::new(
            action.weapon.min_range.unwrap_or_default(),
            maximum,
        ))
    }
}

fn attack_exceeds_leash(squad: &Squad, target: TargetSnapshot) -> bool {
    let Some(origin) = squad.auto_attack_origin() else {
        return false;
    };
    !squad.ignores_leash()
        && squad.leash_distance.is_finite()
        && squad.leash_distance > 0.0
        && planar_distance_squared(origin, target.position)
            > squad.leash_distance * squad.leash_distance
}

fn combat_motion(
    source_position: Vec3,
    source_radius: f32,
    target: TargetSnapshot,
    range: Option<AttackRange>,
) -> CombatMotion {
    let Some(range) = range else {
        return CombatMotion::Hold(target);
    };
    match range.relation(
        source_position,
        source_radius,
        target.position,
        target.obstruction_radius,
    ) {
        AttackRangeRelation::InsideMinimum => CombatMotion::InsideMinimum(target),
        AttackRangeRelation::Within => CombatMotion::Hold(target),
        AttackRangeRelation::BeyondMaximum => CombatMotion::Chase(target),
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn planar_distance(left: Vec3, right: Vec3) -> f32 {
    planar_distance_squared(left, right).sqrt()
}

fn planar_distance_squared(left: Vec3, right: Vec3) -> f32 {
    let delta = left - right;
    delta.x.mul_add(delta.x, delta.z * delta.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_range_uses_attacker_edge_to_target_center() {
        let range = AttackRange::new(10.0, 30.0);
        assert_eq!(
            range.relation(Vec3::ZERO, 2.0, Vec3::X * 11.0, 4.0),
            AttackRangeRelation::InsideMinimum
        );
        assert_eq!(
            range.relation(Vec3::ZERO, 2.0, Vec3::X * 12.0, 4.0),
            AttackRangeRelation::Within
        );
    }

    #[test]
    fn maximum_range_uses_edge_to_edge_distance() {
        let range = AttackRange::new(0.0, 30.0);
        assert_eq!(
            range.relation(Vec3::ZERO, 2.0, Vec3::X * 36.0, 4.0),
            AttackRangeRelation::Within
        );
        assert_eq!(
            range.relation(Vec3::ZERO, 2.0, Vec3::X * 36.01, 4.0),
            AttackRangeRelation::BeyondMaximum
        );
    }
}
