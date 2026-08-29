//! Authoritative attack-order targeting and pursuit.

mod area_damage;
mod damage;
mod deviation;
mod helpers;
#[cfg(test)]
mod test_catalog;

use super::World;
use crate::entities::projectiles::{ProjectileLaunch, launch_target_position};
use crate::entities::{Projectile, Squad, SquadMode, SquadState, Unit, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AreaDamageProfile, AttackAccuracyProfile, AttackProfile, AttackQuery, AttackQueryFlags,
    GameplayCatalog, RangedAction, TacticRelation,
};
use crate::player::{PlayerId, TeamRelation};
pub(crate) use area_damage::AttackDamage;
use glam::Vec3;
use helpers::{face_position, scaled_launch_damage, selected_range, xz_distance_squared};

const MIN_TARGET_RADIUS: f32 = 0.5;
const MOVEMENT_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone, Copy)]
struct TargetSnapshot {
    id: EntityId,
    player_id: PlayerId,
    position: Vec3,
}

#[derive(Debug, Clone, Copy)]
enum CombatMotion {
    Clear,
    Hold(TargetSnapshot),
    Chase(TargetSnapshot),
}

#[derive(Debug, Clone, Copy)]
struct AttackEngagement {
    attacker_id: EntityId,
    ordered_target_id: EntityId,
    range_override: f32,
}

#[derive(Debug, Clone)]
struct AttackerSnapshot {
    player_id: PlayerId,
    position: Vec3,
    launch_position: Vec3,
    damage_multiplier: f32,
    range_scalar: f32,
    authored_range: f32,
    proto_object_name: String,
    accuracy_scalar: f32,
    dodge_scalar: f32,
    moving_at_full_speed: bool,
}

#[derive(Debug, Clone)]
struct ConcreteTargetSnapshot {
    id: EntityId,
    player_id: PlayerId,
    position: Vec3,
    aim_position: Vec3,
    velocity: Vec3,
    collision_radius: f32,
    proto_object_name: String,
    damaged: bool,
    unbuilt: bool,
}

#[derive(Debug, Clone)]
struct FireEvent {
    source_id: EntityId,
    source_player_id: PlayerId,
    source_position: Vec3,
    launch_position: Vec3,
    target: ConcreteTargetSnapshot,
    damage: f32,
    weapon_type: Option<String>,
    projectile_name: Option<String>,
    area_damage: Option<AreaDamageProfile>,
    max_range: f32,
    max_velocity_lead: f32,
    accuracy: deviation::LaunchAccuracy,
    friendly_fire: bool,
    collides_with_all_units: bool,
    targets_foot_of_unit: bool,
}

impl FireEvent {
    fn from_attack(
        source_id: EntityId,
        attacker: &AttackerSnapshot,
        target: ConcreteTargetSnapshot,
        damage: f32,
        profile: &AttackProfile,
        area_damage: Option<AreaDamageProfile>,
    ) -> Self {
        Self {
            source_id,
            source_player_id: attacker.player_id,
            source_position: attacker.position,
            launch_position: attacker.launch_position,
            target,
            damage,
            weapon_type: profile.weapon_type.clone(),
            projectile_name: profile.projectile.clone(),
            area_damage,
            max_range: profile.max_range,
            max_velocity_lead: profile.max_velocity_lead,
            accuracy: deviation::LaunchAccuracy::new(profile.accuracy, false, 1.0, 1.0),
            friendly_fire: profile.friendly_fire,
            collides_with_all_units: !profile.targets_foot_of_unit,
            targets_foot_of_unit: profile.targets_foot_of_unit,
        }
    }

    fn with_launch_tuning(mut self, tuning: LaunchTuning) -> Self {
        self.max_range = tuning.max_range;
        self.max_velocity_lead = tuning.max_velocity_lead;
        self.accuracy = tuning.accuracy;
        self
    }
}

#[derive(Debug, Clone, Copy)]
struct LaunchTuning {
    max_range: f32,
    max_velocity_lead: f32,
    accuracy: deviation::LaunchAccuracy,
}

impl World {
    /// Assign an owned unit or squad an attack target.
    ///
    /// A clicked squad member is canonicalized to its parent squad so the
    /// order survives that member's death and can later retarget another member.
    pub fn issue_attack_order(
        &mut self,
        player_id: PlayerId,
        recipient_id: EntityId,
        requested_target_id: EntityId,
        range: f32,
    ) -> bool {
        self.issue_attack_order_with_context(
            player_id,
            recipient_id,
            requested_target_id,
            range,
            None,
            None,
        )
    }

    /// Assign an attack target plus the tactic context carried by a work command.
    pub fn issue_attack_order_with_context(
        &mut self,
        player_id: PlayerId,
        recipient_id: EntityId,
        requested_target_id: EntityId,
        range: f32,
        squad_mode: Option<SquadMode>,
        ability_id: Option<u8>,
    ) -> bool {
        let Some(target) = self.attack_target_snapshot(requested_target_id) else {
            return false;
        };
        if target.id == recipient_id || !self.players_are_enemies(player_id, target.player_id) {
            return false;
        }

        if self.squads.get(recipient_id).is_some_and(|squad| {
            squad.base.player_id == player_id && !self.is_squad_incapacitated(recipient_id)
        }) {
            return self
                .squads
                .get_mut(recipient_id)
                .is_some_and(|squad| squad.attack(target.id, range, squad_mode, ability_id));
        }
        if self
            .units
            .get(recipient_id)
            .is_some_and(|unit| unit.base.player_id == player_id)
        {
            return self
                .units
                .get_mut(recipient_id)
                .is_some_and(|unit| unit.attack(target.id, range, ability_id));
        }
        false
    }

    pub(super) fn update_combat_orders(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let mut engagements = self.update_squad_combat_orders(gameplay);
        engagements.extend(self.update_standalone_combat_orders(gameplay));
        engagements.sort_by_key(|engagement| engagement.attacker_id);
        engagements.dedup_by_key(|engagement| engagement.attacker_id);
        self.advance_attacks(dt, gameplay, engagements);
    }

    pub(in crate::world) fn validated_squad_attack_target(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<EntityId> {
        match self.squad_combat_motion(squad_id, gameplay) {
            CombatMotion::Hold(target) => Some(target.id),
            CombatMotion::Clear | CombatMotion::Chase(_) => None,
        }
    }

    pub(super) fn update_attack_move_orders(&mut self, gameplay: &GameplayCatalog) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| {
                (squad.state == SquadState::Moving
                    && squad.is_executing_attack_move()
                    && squad.aggro_distance.is_finite()
                    && squad.aggro_distance > 0.0)
                    .then_some(id)
            })
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let Some(target_id) = self.attack_move_target(squad_id, gameplay) else {
                continue;
            };
            let Some(target_id) = self
                .attack_target_snapshot(target_id)
                .map(|target| target.id)
            else {
                continue;
            };
            if let Some(squad) = self.squads.get_mut(squad_id) {
                let _started = squad.begin_attack_move_engagement(target_id);
            }
        }
    }

    fn attack_move_target(
        &self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<EntityId> {
        let squad = self.squads.get(squad_id)?;
        let maximum_distance_squared = squad.aggro_distance * squad.aggro_distance;
        self.units
            .iter()
            .filter(|(unit_id, unit)| {
                !squad.contains_unit(*unit_id)
                    && unit.is_alive()
                    && !unit.is_garrisoned()
                    && unit.is_auto_attackable()
                    && self.players_are_enemies(squad.base.player_id, unit.base.player_id)
            })
            .filter_map(|(unit_id, unit)| {
                let distance_squared = xz_distance_squared(squad.base.position, unit.base.position);
                if distance_squared > maximum_distance_squared {
                    return None;
                }
                let target = concrete_target_snapshot(unit_id, unit);
                let can_attack = squad.unit_ids.iter().any(|member_id| {
                    self.units.get(*member_id).is_some_and(|member| {
                        self.selected_ranged_action(member, &target, gameplay, true)
                            .is_some()
                    })
                });
                can_attack.then_some((distance_squared, unit_id))
            })
            .min_by(|(left_distance, left_id), (right_distance, right_id)| {
                left_distance
                    .total_cmp(right_distance)
                    .then_with(|| left_id.cmp(right_id))
            })
            .map(|(_, unit_id)| unit_id)
    }

    fn update_squad_combat_orders(&mut self, gameplay: &GameplayCatalog) -> Vec<AttackEngagement> {
        let mut engagements = Vec::new();
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| (squad.state == SquadState::Attacking).then_some(id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let motion = self.squad_combat_motion(squad_id, gameplay);
            let Some((member_ids, range_override)) = self
                .squads
                .get(squad_id)
                .map(|squad| (squad.unit_ids.clone(), squad.attack_range))
            else {
                continue;
            };
            let Some(squad) = self.squads.get_mut(squad_id) else {
                continue;
            };
            match motion {
                CombatMotion::Clear => squad.clear_attack_order(),
                CombatMotion::Hold(target) => {
                    squad.hold_attack_position(target.position);
                    engagements.extend(member_ids.iter().copied().map(|attacker_id| {
                        AttackEngagement {
                            attacker_id,
                            ordered_target_id: target.id,
                            range_override,
                        }
                    }));
                }
                CombatMotion::Chase(target) => squad.chase_attack_target(target.position),
            }
            if !matches!(motion, CombatMotion::Hold(_)) {
                self.stop_unit_firing(&member_ids);
            }
        }
        engagements
    }

    fn update_standalone_combat_orders(
        &mut self,
        gameplay: &GameplayCatalog,
    ) -> Vec<AttackEngagement> {
        let mut engagements = Vec::new();
        let unit_ids = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (unit.state == UnitState::Attacking && unit.squad_id.is_none()).then_some(id)
            })
            .collect::<Vec<_>>();
        for unit_id in unit_ids {
            let motion = self.unit_combat_motion(unit_id, gameplay);
            let range_override = self
                .units
                .get(unit_id)
                .map_or(0.0, |unit| unit.attack_range);
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            match motion {
                CombatMotion::Clear => unit.clear_attack_order(),
                CombatMotion::Hold(target) => {
                    unit.hold_attack_position(target.position);
                    engagements.push(AttackEngagement {
                        attacker_id: unit_id,
                        ordered_target_id: target.id,
                        range_override,
                    });
                }
                CombatMotion::Chase(target) => unit.chase_attack_target(target.position),
            }
            if !matches!(motion, CombatMotion::Hold(_)) {
                unit.combat.stop_firing();
            }
        }
        engagements
    }

    fn advance_attacks(
        &mut self,
        dt: f32,
        gameplay: &GameplayCatalog,
        engagements: Vec<AttackEngagement>,
    ) {
        let mut fire_events = Vec::new();
        for engagement in engagements {
            let Some(target) = self.concrete_attack_target(engagement.ordered_target_id) else {
                self.stop_unit_firing(&[engagement.attacker_id]);
                continue;
            };
            let Some(profile) =
                self.selected_unit_attack_profile(engagement.attacker_id, &target, gameplay)
            else {
                self.stop_unit_firing(&[engagement.attacker_id]);
                continue;
            };
            let ability_squad_id = self.active_ability_squad(engagement.attacker_id, gameplay);
            let Some(attacker) = self.attacker_snapshot(engagement.attacker_id, profile) else {
                continue;
            };
            let range = selected_range(
                engagement.range_override,
                attacker.authored_range,
                attacker.range_scalar,
            );
            if !self.players_are_enemies(attacker.player_id, target.player_id)
                || xz_distance_squared(attacker.position, target.position) > range * range
            {
                self.stop_unit_firing(&[engagement.attacker_id]);
                continue;
            }

            let advance = {
                let (units, rng) = (&mut self.units, &mut self.rng);
                let Some(unit) = units.get_mut(engagement.attacker_id) else {
                    continue;
                };
                face_position(unit, target.position);
                unit.combat.advance(dt, target.id, profile, rng)
            };
            if advance.completed_cycles > 0
                && let Some(squad_id) = ability_squad_id
                && let Some(squad) = self.squads.get_mut(squad_id)
            {
                squad.mark_unit_ability_complete(engagement.attacker_id);
            }
            let authored_damage =
                self.get_player(attacker.player_id)
                    .map_or(profile.damage_per_attack, |player| {
                        player.technologies.weapon_damage(
                            &attacker.proto_object_name,
                            &profile.weapon_name,
                            profile.damage_per_attack,
                        )
                    });
            let damage = scaled_launch_damage(
                authored_damage,
                attacker.damage_multiplier,
                attacker.position,
                target.position,
                profile.uses_height_bonus_damage,
                gameplay.height_bonus_damage(),
            );
            let area_damage =
                self.launch_area_damage(attacker.player_id, &attacker.proto_object_name, profile);
            let tuning = self.launch_tuning(&attacker, profile);
            let event = FireEvent::from_attack(
                engagement.attacker_id,
                &attacker,
                target,
                damage,
                profile,
                area_damage,
            )
            .with_launch_tuning(tuning);
            fire_events.extend((0..advance.hit_count).map(|_| event.clone()));
        }
        self.finish_completed_ability_attacks(gameplay);
        self.resolve_fire_events(fire_events, gameplay);
    }

    fn attacker_snapshot(
        &self,
        attacker_id: EntityId,
        profile: &AttackProfile,
    ) -> Option<AttackerSnapshot> {
        let unit = self
            .units
            .get(attacker_id)
            .filter(|unit| unit.is_operational())?;
        let authored_range =
            self.get_player(unit.base.player_id)
                .map_or(profile.max_range, |player| {
                    player.technologies.weapon_range(
                        &unit.proto_object_name,
                        &profile.weapon_name,
                        profile.max_range,
                    )
                });
        let speed = unit.base.velocity.length();
        let desired_speed = unit.speed * unit.velocity_scalar;
        Some(AttackerSnapshot {
            player_id: unit.base.player_id,
            position: unit.base.position,
            launch_position: unit.simulation_center(),
            damage_multiplier: unit.effective_damage_multiplier(),
            range_scalar: unit.weapon_range_scalar,
            authored_range,
            proto_object_name: unit.proto_object_name.clone(),
            accuracy_scalar: unit.accuracy_scalar,
            dodge_scalar: unit.dodge_scalar,
            moving_at_full_speed: speed > MOVEMENT_EPSILON && speed >= desired_speed * 0.9,
        })
    }

    fn launch_tuning(&self, attacker: &AttackerSnapshot, profile: &AttackProfile) -> LaunchTuning {
        let mut accuracy = profile.accuracy;
        let mut max_velocity_lead = profile.max_velocity_lead;
        if let Some(player) = self.get_player(attacker.player_id) {
            accuracy = effective_attack_accuracy(
                &player.technologies,
                &attacker.proto_object_name,
                &profile.weapon_name,
                accuracy,
            );
            max_velocity_lead = player.technologies.weapon_max_velocity_lead(
                &attacker.proto_object_name,
                &profile.weapon_name,
                max_velocity_lead,
            );
        }
        LaunchTuning {
            max_range: selected_range(0.0, attacker.authored_range, attacker.range_scalar),
            max_velocity_lead,
            accuracy: deviation::LaunchAccuracy::new(
                accuracy,
                attacker.moving_at_full_speed,
                attacker.accuracy_scalar,
                attacker.dodge_scalar,
            ),
        }
    }

    fn resolve_fire_events(&mut self, fire_events: Vec<FireEvent>, gameplay: &GameplayCatalog) {
        for event in fire_events {
            self.resolve_fire_event(event, gameplay);
        }
    }

    fn resolve_fire_event(&mut self, event: FireEvent, gameplay: &GameplayCatalog) {
        let Some(projectile_name) = event.projectile_name.as_deref() else {
            let direction = event.target.position - event.source_position;
            self.apply_attack_damage(
                &AttackDamage {
                    attacker_id: event.source_id,
                    attacker_player_id: event.source_player_id,
                    primary_target_id: Some(event.target.id),
                    ground_zero: event.target.position,
                    direction,
                    damage: event.damage,
                    weapon_type: event.weapon_type,
                    area_damage: event.area_damage,
                },
                Some(gameplay),
            );
            return;
        };
        let Some(profile) = gameplay.projectile(projectile_name) else {
            return;
        };
        let aim_position = if event.targets_foot_of_unit {
            event.target.position
        } else {
            event.target.aim_position
        };
        let led_target_position = launch_target_position(
            event.launch_position,
            aim_position,
            event.target.velocity,
            profile.speed,
            event.max_velocity_lead,
        );
        let targeting_lead = led_target_position - aim_position;
        let deviation = deviation::projectile_deviation(
            &mut self.sim_rng,
            event.launch_position,
            aim_position,
            targeting_lead,
            event.max_range,
            event.accuracy,
        );
        let target_offset = aim_position - event.target.position + deviation;
        let mut target_position = led_target_position + deviation;
        if let Some(terrain_height) = self.terrain_height(target_position, true) {
            target_position.y = target_position.y.max(terrain_height);
        }
        let id = self.projectiles.allocate_id();
        let projectile = Projectile::new(
            id,
            event.source_player_id,
            ProjectileLaunch {
                source_id: event.source_id,
                target_id: event.target.id,
                source_position: event.launch_position,
                target_position,
                target_entity_position: event.target.position,
                target_offset,
                target_radius: event.target.collision_radius,
                max_range: event.max_range,
                damage: event.damage,
                weapon_type: event.weapon_type,
                area_damage: event.area_damage,
                friendly_fire: event.friendly_fire,
                collides_with_all_units: event.collides_with_all_units,
            },
            profile,
        );
        self.projectiles.insert(id, projectile);
    }

    fn launch_area_damage(
        &self,
        player_id: PlayerId,
        proto_object: &str,
        profile: &AttackProfile,
    ) -> Option<AreaDamageProfile> {
        profile.area_damage.map(|mut area_damage| {
            let primary_target_factor =
                self.get_player(player_id)
                    .map_or(area_damage.primary_target_factor, |player| {
                        player.technologies.weapon_aoe_primary_target_factor(
                            proto_object,
                            &profile.weapon_name,
                            area_damage.primary_target_factor,
                        )
                    });
            if primary_target_factor.is_finite() {
                area_damage.primary_target_factor = primary_target_factor;
            }
            area_damage
        })
    }

    pub(in crate::world) fn stop_unit_firing(&mut self, unit_ids: &[EntityId]) {
        for &unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.combat.stop_firing();
            }
        }
    }

    fn squad_combat_motion(&self, squad_id: EntityId, gameplay: &GameplayCatalog) -> CombatMotion {
        let Some(squad) = self.squads.get(squad_id) else {
            return CombatMotion::Clear;
        };
        if self.is_squad_incapacitated(squad_id) {
            return CombatMotion::Clear;
        }
        let Some(target_id) = squad.attack_target else {
            return CombatMotion::Clear;
        };
        let Some(target) = self.attack_target_snapshot(target_id) else {
            return CombatMotion::Clear;
        };
        if !self.players_are_enemies(squad.base.player_id, target.player_id) {
            return CombatMotion::Clear;
        }
        if let Some(origin) = squad.auto_attack_origin()
            && squad.leash_distance.is_finite()
            && squad.leash_distance > 0.0
            && xz_distance_squared(origin, target.position)
                > squad.leash_distance * squad.leash_distance
        {
            return CombatMotion::Clear;
        }
        let range = if squad.attack_range > 0.0 {
            Some(squad.attack_range)
        } else {
            let Some(concrete_target) = self.concrete_attack_target(target.id) else {
                return CombatMotion::Clear;
            };
            self.squad_tactic_range(squad, &concrete_target, gameplay)
        };
        combat_motion(squad.base.position, target, range)
    }

    fn unit_combat_motion(&self, unit_id: EntityId, gameplay: &GameplayCatalog) -> CombatMotion {
        let Some(unit) = self.units.get(unit_id).filter(|unit| unit.is_operational()) else {
            return CombatMotion::Clear;
        };
        let Some(target_id) = unit.attack_target else {
            return CombatMotion::Clear;
        };
        let Some(target) = self.attack_target_snapshot(target_id) else {
            return CombatMotion::Clear;
        };
        if !self.players_are_enemies(unit.base.player_id, target.player_id) {
            return CombatMotion::Clear;
        }
        let range = if unit.attack_range > 0.0 {
            Some(unit.attack_range)
        } else {
            let Some(concrete_target) = self.concrete_attack_target(target.id) else {
                return CombatMotion::Clear;
            };
            self.unit_tactic_range(unit, &concrete_target, gameplay, false)
        };
        combat_motion(unit.base.position, target, range)
    }

    fn concrete_attack_target(&self, requested_id: EntityId) -> Option<ConcreteTargetSnapshot> {
        if let Some(unit) = self
            .units
            .get(requested_id)
            .filter(|unit| unit.is_attackable())
        {
            return Some(concrete_target_snapshot(requested_id, unit));
        }
        let squad = self.squads.get(requested_id).filter(|squad| {
            squad.is_alive()
                && !self.is_squad_incapacitated(requested_id)
                && !squad.garrison.is_garrisoned()
        })?;
        squad
            .unit_ids
            .iter()
            .copied()
            .filter_map(|unit_id| {
                self.units
                    .get(unit_id)
                    .filter(|unit| unit.is_attackable())
                    .map(|unit| (unit_id, unit))
            })
            .min_by_key(|(unit_id, _)| *unit_id)
            .map(|(unit_id, unit)| concrete_target_snapshot(unit_id, unit))
    }

    fn attack_target_snapshot(&self, requested_id: EntityId) -> Option<TargetSnapshot> {
        if let Some(unit) = self
            .units
            .get(requested_id)
            .filter(|unit| unit.is_attackable())
        {
            if let Some(squad_id) = unit.squad_id
                && let Some(squad) = self.squads.get(squad_id).filter(|squad| {
                    squad.is_alive()
                        && !self.is_squad_incapacitated(squad_id)
                        && !squad.garrison.is_garrisoned()
                        && !squad.unit_ids.is_empty()
                })
            {
                return Some(TargetSnapshot {
                    id: squad_id,
                    player_id: squad.base.player_id,
                    position: squad.base.position,
                });
            }
            return Some(TargetSnapshot {
                id: requested_id,
                player_id: unit.base.player_id,
                position: unit.base.position,
            });
        }
        let squad = self.squads.get(requested_id).filter(|squad| {
            squad.is_alive()
                && !self.is_squad_incapacitated(requested_id)
                && !squad.garrison.is_garrisoned()
                && !squad.unit_ids.is_empty()
        })?;
        Some(TargetSnapshot {
            id: requested_id,
            player_id: squad.base.player_id,
            position: squad.base.position,
        })
    }

    fn squad_tactic_range(
        &self,
        squad: &Squad,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) -> Option<f32> {
        let automatic = squad.is_auto_attack_engagement();
        squad
            .unit_ids
            .iter()
            .filter_map(|&unit_id| self.units.get(unit_id))
            .filter_map(|unit| self.unit_tactic_range(unit, target, gameplay, automatic))
            .reduce(f32::max)
    }

    fn unit_tactic_range(
        &self,
        unit: &Unit,
        target: &ConcreteTargetSnapshot,
        gameplay: &GameplayCatalog,
        automatic: bool,
    ) -> Option<f32> {
        let action = self.selected_ranged_action(unit, target, gameplay, automatic)?;
        let range = action
            .weapon
            .max_range
            .filter(|range| range.is_finite() && *range >= 0.0)
            .map(|range| {
                self.get_player(unit.base.player_id)
                    .map_or(range, |player| {
                        player.technologies.weapon_range(
                            &unit.proto_object_name,
                            &action.weapon.name,
                            range,
                        )
                    })
            })?;
        Some(range * unit.weapon_range_scalar)
    }

    fn selected_unit_attack_profile<'gameplay>(
        &self,
        unit_id: EntityId,
        target: &ConcreteTargetSnapshot,
        gameplay: &'gameplay GameplayCatalog,
    ) -> Option<&'gameplay AttackProfile> {
        let unit = self.units.get(unit_id)?;
        let automatic = unit
            .squad_id
            .and_then(|squad_id| self.squads.get(squad_id))
            .is_some_and(Squad::is_auto_attack_engagement);
        let action = self.selected_ranged_action(unit, target, gameplay, automatic)?;
        gameplay
            .object(&unit.proto_object_name)?
            .attack_profile(&action.action.name)
    }

    fn selected_ranged_action<'gameplay>(
        &self,
        unit: &Unit,
        target: &ConcreteTargetSnapshot,
        gameplay: &'gameplay GameplayCatalog,
        automatic: bool,
    ) -> Option<RangedAction<'gameplay>> {
        let (squad_mode, requested_ability_id) = unit
            .squad_id
            .and_then(|id| self.squads.get(id))
            .map_or((SquadMode::Normal, unit.attack_ability_id), |squad| {
                (
                    squad.mode,
                    (!squad.unit_completed_ability(unit.base.id))
                        .then_some(squad.attack_ability_id)
                        .flatten(),
                )
            });
        let ability_id = requested_ability_id.filter(|requested| {
            gameplay
                .resolve_order_ability(&unit.proto_object_name, *requested)
                .is_some()
        });
        let mut flags = AttackQueryFlags::empty();
        if automatic {
            flags.insert(AttackQueryFlags::AUTO_TARGET);
        }
        if target.player_id == 0 {
            flags.insert(AttackQueryFlags::TARGET_GAIA);
        }
        if target.damaged {
            flags.insert(AttackQueryFlags::TARGET_DAMAGED);
        }
        if target.unbuilt {
            flags.insert(AttackQueryFlags::TARGET_UNBUILT);
        }
        let query = AttackQuery {
            relation: self.tactic_relation(unit.base.player_id, target.player_id),
            squad_mode,
            ability_id,
            target_proto_object_name: Some(&target.proto_object_name),
            flags,
        };
        gameplay.select_ranged_action(&unit.proto_object_name, &query, |action| {
            let authored_enabled = action.start_disabled != Some(true);
            let player_enabled =
                self.get_player(unit.base.player_id)
                    .map_or(authored_enabled, |player| {
                        player.technologies.action_enabled(
                            &unit.proto_object_name,
                            &action.name,
                            authored_enabled,
                        )
                    });
            unit.actions.is_enabled(&action.name, !player_enabled)
        })
    }

    fn tactic_relation(&self, source: PlayerId, target: PlayerId) -> TacticRelation {
        if source == target {
            return TacticRelation::SelfPlayer;
        }
        match self.player_relation(source, target) {
            Some(TeamRelation::Ally) => TacticRelation::Ally,
            Some(TeamRelation::Enemy) => TacticRelation::Enemy,
            Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
        }
    }
}

fn effective_attack_accuracy(
    technologies: &crate::player::PlayerTechState,
    proto_object: &str,
    weapon: &str,
    base: AttackAccuracyProfile,
) -> AttackAccuracyProfile {
    AttackAccuracyProfile {
        accuracy: technologies.weapon_accuracy(proto_object, weapon, base.accuracy),
        moving_accuracy: technologies.weapon_moving_accuracy(
            proto_object,
            weapon,
            base.moving_accuracy,
        ),
        max_deviation: technologies.weapon_max_deviation(proto_object, weapon, base.max_deviation),
        moving_max_deviation: technologies.weapon_moving_max_deviation(
            proto_object,
            weapon,
            base.moving_max_deviation,
        ),
        distance_factor: technologies.weapon_accuracy_distance_factor(
            proto_object,
            weapon,
            base.distance_factor,
        ),
        deviation_factor: technologies.weapon_accuracy_deviation_factor(
            proto_object,
            weapon,
            base.deviation_factor,
        ),
    }
}

fn combat_motion(position: Vec3, target: TargetSnapshot, range: Option<f32>) -> CombatMotion {
    let Some(range) = range else {
        return CombatMotion::Hold(target);
    };
    let offset = Vec3::new(
        target.position.x - position.x,
        0.0,
        target.position.z - position.z,
    );
    if offset.length_squared() <= range * range {
        CombatMotion::Hold(target)
    } else {
        CombatMotion::Chase(target)
    }
}

fn concrete_target_snapshot(id: EntityId, unit: &Unit) -> ConcreteTargetSnapshot {
    ConcreteTargetSnapshot {
        id,
        player_id: unit.base.player_id,
        position: unit.base.position,
        aim_position: unit.simulation_center(),
        velocity: unit.base.velocity,
        collision_radius: unit.obstruction_radius().max(MIN_TARGET_RADIUS),
        proto_object_name: unit.proto_object_name.clone(),
        damaged: unit.hitpoints < unit.max_hitpoints,
        unbuilt: unit.is_building() && !unit.built,
    }
}

#[cfg(test)]
mod tests;
