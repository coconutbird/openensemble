//! Authoritative attack-order targeting and pursuit.

mod ammunition;
mod area_damage;
mod damage;
mod deviation;
mod fire;
mod hardpoints;
mod helpers;
mod impulses;
mod position;
mod pull;
mod selection;
#[cfg(test)]
mod test_catalog;

use super::World;
use crate::entities::projectiles::{ProjectileLaunch, launch_target_position};
use crate::entities::{AttackAdvance, Projectile, Squad, SquadMode, SquadState, Unit, UnitState};
use crate::entity::Entity;
use crate::entity_id::{EntityClass, EntityId};
use crate::gameplay::{AreaDamageProfile, AttackAnimationAnchor, AttackProfile, GameplayCatalog};
use crate::player::PlayerId;
pub(crate) use area_damage::AttackDamage;
use fire::{FireEvent, LaunchTuning};
use glam::Vec3;
use helpers::{
    animation_anchor_world_transform, attack_aim_position, combat_motion,
    effective_attack_accuracy, scaled_launch_damage, selected_range, unit_world_transform,
    xz_distance_squared,
};
pub(in crate::world) use position::PositionAttackStatus;

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

#[derive(Debug, Clone, Copy)]
struct AttackAdvanceSettings {
    elapsed: f32,
    aim_position: Vec3,
    charged_cycle: bool,
    orientation_tolerance: f32,
    authored_damage: f32,
}

#[derive(Debug, Clone)]
struct AttackerSnapshot {
    player_id: PlayerId,
    position: Vec3,
    launch_position: Vec3,
    hardpoint_position: Option<Vec3>,
    damage_multiplier: f32,
    range_scalar: f32,
    authored_range: f32,
    logical_proto_object_name: String,
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
    in_cover: bool,
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
        let Some(target) = self.attack_target_snapshot(player_id, requested_target_id) else {
            return false;
        };
        if target.id == recipient_id || !self.players_are_enemies(player_id, target.player_id) {
            return false;
        }

        if self.squads.get(recipient_id).is_some_and(|squad| {
            squad.base.player_id == player_id && !self.is_squad_incapacitated(recipient_id)
        }) {
            let _cancelled = self.cancel_capture_order(recipient_id);
            let _repair_cancelled = self.cancel_repair_other_order(recipient_id);
            let accepted = self
                .squads
                .get_mut(recipient_id)
                .is_some_and(|squad| squad.attack(target.id, range, squad_mode, ability_id));
            if accepted {
                self.cancel_incoming_power_transport(recipient_id);
            }
            return accepted;
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
        for (_, unit) in self.units.iter_mut() {
            hardpoints::advance_auto_center(unit, dt);
        }
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
                    && !squad.is_cryo_frozen()
                    && squad.aggro_distance.is_finite()
                    && squad.aggro_distance > 0.0)
                    .then_some(id)
            })
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let Some(player_id) = self.squads.get(squad_id).map(|squad| squad.base.player_id)
            else {
                continue;
            };
            let Some(target_id) = self.attack_move_target(squad_id, gameplay) else {
                continue;
            };
            let Some(target_id) = self
                .attack_target_snapshot(player_id, target_id)
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
                    && !self.entity_hidden_by_cloak_from_player(squad.base.player_id, *unit_id)
            })
            .filter_map(|(unit_id, unit)| {
                let distance_squared = xz_distance_squared(squad.base.position, unit.base.position);
                if distance_squared > maximum_distance_squared {
                    return None;
                }
                let target = concrete_target_snapshot(unit_id, unit, self.unit_is_in_cover(unit));
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
        let game_time_ms = self.game_time_ms;
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| {
                (squad.state == SquadState::Attacking && !squad.is_carpet_bombing()).then_some(id)
            })
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let Some(squad) = self.squads.get(squad_id) else {
                continue;
            };
            let member_ids = squad.unit_ids.clone();
            let range_override = squad.attack_range;
            if squad.is_cryo_frozen() {
                self.stop_unit_firing(&member_ids);
                continue;
            }
            let motion = self.squad_combat_motion(squad_id, gameplay);
            let completed_attack = matches!(motion, CombatMotion::Clear)
                && self
                    .squads
                    .get(squad_id)
                    .and_then(|squad| squad.attack_target)
                    .is_some_and(|target_id| self.attack_target_is_defeated(target_id));
            if completed_attack {
                self.apply_squad_experience_bank(squad_id, gameplay);
            }
            let Some(squad) = self.squads.get_mut(squad_id) else {
                continue;
            };
            match motion {
                CombatMotion::Clear => squad.clear_attack_order(),
                CombatMotion::Hold(target) => {
                    squad.last_attacked_time = game_time_ms;
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

    fn attack_target_is_defeated(&self, target_id: EntityId) -> bool {
        match target_id.class() {
            Some(EntityClass::Unit) => self
                .units
                .get(target_id)
                .is_none_or(|unit| !unit.is_alive()),
            Some(EntityClass::Squad) => self.squads.get(target_id).is_none_or(|squad| {
                !squad.is_alive()
                    || !squad
                        .unit_ids
                        .iter()
                        .any(|unit_id| self.units.get(*unit_id).is_some_and(Entity::is_alive))
            }),
            _ => false,
        }
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
            let Some(attacker_player_id) = self.unblocked_attacker_player(engagement.attacker_id)
            else {
                continue;
            };
            let Some(target) =
                self.concrete_attack_target(attacker_player_id, engagement.ordered_target_id)
            else {
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
            let (normal_range, range) =
                self.engagement_attack_range(&engagement, &attacker, &target, profile);
            if !self.players_are_enemies(attacker.player_id, target.player_id)
                || xz_distance_squared(attacker.position, target.position) > range * range
            {
                self.stop_unit_firing(&[engagement.attacker_id]);
                continue;
            }
            let charged_cycle = profile.pull.as_ref().is_some_and(|pull| {
                self.can_resolve_charged_pull(engagement.attacker_id, &target, pull, normal_range)
            });
            let orientation_tolerance = gameplay.attack_orientation_tolerance(
                profile,
                target.velocity != Vec3::ZERO,
                ability_squad_id.is_some(),
            );

            let authored_damage =
                self.authored_attack_damage(engagement.attacker_id, &attacker, profile);
            let mut tuning = self.launch_tuning(&attacker, profile);
            let aim_position = attack_aim_position(&attacker, &target, profile, gameplay, &tuning);
            tuning.max_range = range;
            let settings = AttackAdvanceSettings {
                elapsed: dt,
                aim_position,
                charged_cycle,
                orientation_tolerance,
                authored_damage,
            };
            let Some(advance) =
                self.advance_unit_attack(engagement.attacker_id, profile, &target, settings)
            else {
                continue;
            };
            if advance.completed_cycles > 0
                && let Some(squad_id) = ability_squad_id
                && let Some(squad) = self.squads.get_mut(squad_id)
            {
                squad.mark_unit_ability_complete(engagement.attacker_id);
            }
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
            for occurrence in &advance.events {
                if self.apply_physics_impulse_event(engagement.attacker_id, &occurrence.event) {
                    continue;
                }
                let mut launch_attacker = attacker.clone();
                launch_attacker.launch_position = self.attack_event_launch_position(
                    engagement.attacker_id,
                    attacker.launch_position,
                    occurrence.event.anchor.as_ref(),
                );
                fire_events.push(
                    FireEvent::from_attack(
                        engagement.attacker_id,
                        &launch_attacker,
                        target.clone(),
                        damage,
                        profile,
                        area_damage,
                        normal_range,
                    )
                    .with_launch_tuning(tuning),
                );
            }
        }
        self.finish_completed_ability_attacks(gameplay);
        self.resolve_fire_events(fire_events, gameplay);
    }

    fn advance_unit_attack(
        &mut self,
        attacker_id: EntityId,
        profile: &AttackProfile,
        target: &ConcreteTargetSnapshot,
        settings: AttackAdvanceSettings,
    ) -> Option<AttackAdvance> {
        let (units, rng) = (&mut self.units, &mut self.rng);
        let unit = units.get_mut(attacker_id)?;
        if !hardpoints::prepare_for_attack(
            unit,
            profile,
            target,
            settings.aim_position,
            settings.charged_cycle,
            settings.orientation_tolerance,
            settings.elapsed,
        ) {
            return None;
        }
        let (combat, unit_ammunition) = (&mut unit.combat, &mut unit.ammunition);
        Some(combat.advance(
            settings.elapsed,
            target.id,
            profile,
            unit_ammunition,
            settings.authored_damage,
            rng,
        ))
    }

    fn authored_attack_damage(
        &self,
        attacker_id: EntityId,
        attacker: &AttackerSnapshot,
        profile: &AttackProfile,
    ) -> f32 {
        self.units
            .get(attacker_id)
            .map_or(profile.damage_per_attack, |unit| {
                ammunition::effective_damage(
                    unit,
                    profile,
                    self.get_player(attacker.player_id)
                        .map(|player| &player.technologies),
                )
            })
    }

    fn attack_event_launch_position(
        &self,
        attacker_id: EntityId,
        fallback: Vec3,
        anchor: Option<&AttackAnimationAnchor>,
    ) -> Vec3 {
        self.units
            .get(attacker_id)
            .and_then(|unit| {
                anchor.and_then(|anchor| animation_anchor_world_transform(unit, anchor))
            })
            .map_or(fallback, |transform| transform.w_axis.truncate())
    }

    fn unblocked_attacker_player(&mut self, attacker_id: EntityId) -> Option<PlayerId> {
        let unit = self.units.get_mut(attacker_id)?;
        if unit.is_move_air_attack_blocked() {
            unit.combat.stop_firing();
            return None;
        }
        Some(unit.base.player_id)
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
                        unit.logical_proto_object_name(),
                        &profile.weapon_name,
                        profile.max_range,
                    )
                });
        let speed = unit.base.velocity.length();
        let desired_speed = unit.speed * unit.effective_velocity_scalar();
        let hardpoint_position = unit_world_transform(unit).and_then(|unit_world| {
            let anchor = unit.combat.hardpoint_anchor(profile);
            unit.combat
                .hardpoint_yaw_origin(profile, anchor.as_ref(), unit_world)
        });
        Some(AttackerSnapshot {
            player_id: unit.base.player_id,
            position: unit.base.position,
            launch_position: unit.simulation_center(),
            hardpoint_position,
            damage_multiplier: unit.effective_damage_multiplier(),
            range_scalar: unit.weapon_range_scalar,
            authored_range,
            logical_proto_object_name: unit.logical_proto_object_name().to_owned(),
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
                &attacker.logical_proto_object_name,
                &profile.weapon_name,
                accuracy,
            );
            max_velocity_lead = player.technologies.weapon_max_velocity_lead(
                &attacker.logical_proto_object_name,
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
            if self.try_resolve_charged_pull(&event, gameplay) {
                return;
            }
            let direction = event.target.position - event.source_position;
            self.apply_attack_damage(
                &AttackDamage {
                    attacker_id: event.source_id,
                    attacker_player_id: event.source_player_id,
                    primary_target_id: (!event.target.id.is_invalid()).then_some(event.target.id),
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
        let mut projectile = Projectile::new(
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
                impact_effect: event.impact_effect,
                friendly_fire: event.friendly_fire,
                collides_with_all_units: event.collides_with_all_units,
            },
            profile,
        );
        projectile.configure_reactions(event.projectile_reactions);
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
        let Some(target) = self.attack_target_snapshot(squad.base.player_id, target_id) else {
            return CombatMotion::Clear;
        };
        if !self.players_are_enemies(squad.base.player_id, target.player_id) {
            return CombatMotion::Clear;
        }
        if let Some(origin) = squad.auto_attack_origin()
            && !squad.ignores_leash()
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
            let Some(concrete_target) =
                self.concrete_attack_target(squad.base.player_id, target.id)
            else {
                return CombatMotion::Clear;
            };
            self.collision_attack_range(squad, gameplay)
                .or_else(|| self.squad_tactic_range(squad, &concrete_target, gameplay))
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
        let Some(target) = self.attack_target_snapshot(unit.base.player_id, target_id) else {
            return CombatMotion::Clear;
        };
        if !self.players_are_enemies(unit.base.player_id, target.player_id) {
            return CombatMotion::Clear;
        }
        let range = if unit.attack_range > 0.0 {
            Some(unit.attack_range)
        } else {
            let Some(concrete_target) = self.concrete_attack_target(unit.base.player_id, target.id)
            else {
                return CombatMotion::Clear;
            };
            self.unit_tactic_range(unit, &concrete_target, gameplay, false)
        };
        combat_motion(unit.base.position, target, range)
    }

    fn concrete_attack_target(
        &self,
        observer_player_id: PlayerId,
        requested_id: EntityId,
    ) -> Option<ConcreteTargetSnapshot> {
        if self.entity_hidden_by_cloak_from_player(observer_player_id, requested_id) {
            return None;
        }
        if let Some(unit) = self
            .units
            .get(requested_id)
            .filter(|unit| unit.is_attackable())
        {
            return Some(concrete_target_snapshot(
                requested_id,
                unit,
                self.unit_is_in_cover(unit),
            ));
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
            .map(|(unit_id, unit)| {
                concrete_target_snapshot(unit_id, unit, self.unit_is_in_cover(unit))
            })
    }

    fn unit_is_in_cover(&self, unit: &Unit) -> bool {
        unit.squad_id
            .and_then(|squad_id| self.squads.get(squad_id))
            .is_some_and(|squad| squad.mode == SquadMode::Cover)
    }

    fn attack_target_snapshot(
        &self,
        observer_player_id: PlayerId,
        requested_id: EntityId,
    ) -> Option<TargetSnapshot> {
        if self.entity_hidden_by_cloak_from_player(observer_player_id, requested_id) {
            return None;
        }
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
                            unit.logical_proto_object_name(),
                            &action.weapon.name,
                            range,
                        )
                    })
            })?;
        Some(
            self.charged_action_range(unit, target, &action.action.name, range, gameplay)
                * unit.weapon_range_scalar,
        )
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

    pub(in crate::world) fn active_ranged_attack_target_position(
        &self,
        unit_id: EntityId,
        ordered_target_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<Vec3> {
        let player_id = self.units.get(unit_id)?.base.player_id;
        let target = self.concrete_attack_target(player_id, ordered_target_id)?;
        self.selected_unit_attack_profile(unit_id, &target, gameplay)?;
        Some(target.position)
    }
}

fn concrete_target_snapshot(id: EntityId, unit: &Unit, in_cover: bool) -> ConcreteTargetSnapshot {
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
        in_cover,
    }
}

#[cfg(test)]
mod tests;
