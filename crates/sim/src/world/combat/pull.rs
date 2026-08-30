//! Charged Brute Chief range selection and damage-replacing squad pull impacts.

use super::{
    AttackDamage, AttackEngagement, AttackerSnapshot, ConcreteTargetSnapshot, FireEvent, World,
};
use crate::entities::Unit;
use crate::gameplay::{AttackProfile, GameplayCatalog, PullAttackProfile};
use glam::Vec3;

use super::helpers::selected_range;

const PULL_DISTANCE_RANGE_MULTIPLIER: f32 = 2.5;

impl World {
    pub(super) fn engagement_attack_range(
        &self,
        engagement: &AttackEngagement,
        attacker: &AttackerSnapshot,
        target: &ConcreteTargetSnapshot,
        profile: &AttackProfile,
    ) -> (f32, f32) {
        let normal = selected_range(
            engagement.range_override,
            attacker.authored_range,
            attacker.range_scalar,
        );
        let effective = if engagement.range_override > 0.0 {
            normal
        } else {
            self.charged_profile_range(engagement.attacker_id, target, profile, normal)
        };
        (normal, effective)
    }

    pub(super) fn charged_action_range(
        &self,
        unit: &Unit,
        target: &ConcreteTargetSnapshot,
        action_name: &str,
        normal_range: f32,
        gameplay: &GameplayCatalog,
    ) -> f32 {
        let pull = gameplay
            .object(&unit.proto_object_name)
            .and_then(|object| object.attack_profile(action_name))
            .and_then(|profile| profile.pull.as_ref());
        if unit.is_charge_ready()
            && pull.is_some_and(|pull| self.is_valid_pull_target(target, pull))
        {
            normal_range.max(pull.map_or(0.0, |pull| pull.max_range))
        } else {
            normal_range
        }
    }

    pub(super) fn charged_profile_range(
        &self,
        attacker_id: crate::EntityId,
        target: &ConcreteTargetSnapshot,
        profile: &AttackProfile,
        normal_range: f32,
    ) -> f32 {
        let Some(unit) = self.units.get(attacker_id) else {
            return normal_range;
        };
        if unit.is_charge_ready()
            && profile
                .pull
                .as_ref()
                .is_some_and(|pull| self.is_valid_pull_target(target, pull))
        {
            normal_range.max(
                profile
                    .pull
                    .as_ref()
                    .map_or(0.0, |pull| pull.max_range * unit.weapon_range_scalar),
            )
        } else {
            normal_range
        }
    }

    pub(super) fn try_resolve_charged_pull(
        &mut self,
        event: &FireEvent,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(pull) = event.pull.as_ref() else {
            return false;
        };
        if !self.can_resolve_charged_pull(event.source_id, &event.target, pull, event.normal_range)
        {
            return false;
        }

        let _started = self.begin_charged_squad_pull(
            event.source_id,
            event.target.id,
            &event.action_name,
            pull,
            event.normal_range,
        );
        if let Some(attacker) = self.units.get_mut(event.source_id) {
            attacker.clear_charge_after_pull();
        }
        let direction = event.target.position - event.source_position;
        let _splash_damage = self.apply_attack_splash_after_pull(
            &AttackDamage {
                attacker_id: event.source_id,
                attacker_player_id: event.source_player_id,
                primary_target_id: Some(event.target.id),
                ground_zero: event.target.position,
                direction,
                damage: event.damage,
                weapon_type: event.weapon_type.clone(),
                area_damage: event.area_damage,
            },
            gameplay,
        );
        true
    }

    pub(super) fn can_resolve_charged_pull(
        &self,
        attacker_id: crate::EntityId,
        target: &ConcreteTargetSnapshot,
        pull: &PullAttackProfile,
        normal_range: f32,
    ) -> bool {
        let Some(attacker) = self.units.get(attacker_id) else {
            return false;
        };
        if !attacker.is_charge_ready()
            || self
                .units
                .get(target.id)
                .and_then(|unit| unit.squad_id)
                .is_none()
        {
            return false;
        }
        // Retail's canPull skips type/mobile validation when no InvalidTarget list exists.
        if !pull.invalid_targets.is_empty() && !self.is_valid_pull_target(target, pull) {
            return false;
        }
        let edge_distance = planar_distance(attacker.base.position, target.position)
            - attacker.obstruction_radius()
            - target.collision_radius;
        edge_distance > normal_range * PULL_DISTANCE_RANGE_MULTIPLIER
    }

    fn is_valid_pull_target(
        &self,
        target: &ConcreteTargetSnapshot,
        pull: &PullAttackProfile,
    ) -> bool {
        let Some(unit) = self.units.get(target.id) else {
            return false;
        };
        let valid_type = ["GroundVehicle", "Infantry", "Flood"]
            .iter()
            .any(|kind| unit.is_object_type(kind));
        valid_type
            && unit.base.is_ever_mobile()
            && !pull
                .invalid_targets
                .iter()
                .any(|invalid| invalid.eq_ignore_ascii_case(&target.proto_object_name))
    }
}

fn planar_distance(left: Vec3, right: Vec3) -> f32 {
    Vec3::new(left.x - right.x, 0.0, left.z - right.z).length()
}
