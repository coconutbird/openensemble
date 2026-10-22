//! Retail secondary- and slave-turret orchestration.

use std::cmp::Ordering;

use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, Weapon};

use crate::entities::{AttackAdvance, TurretAttackKind, TurretAttackPhase, Unit};
use crate::entity_id::EntityId;
use crate::gameplay::{
    AttackProfile, AttackQuery, AttackQueryFlags, GameplayCatalog, ObjectGameplay, TacticRelation,
};
use crate::player::TeamRelation;

use super::fire::FireEvent;
use super::helpers::{
    attack_aim_position, scaled_launch_damage, selected_range, unit_world_transform,
};
use super::range::{AttackRange, AttackRangeRelation};
use super::{AttackerSnapshot, ConcreteTargetSnapshot, World};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
struct TurretAction<'gameplay> {
    kind: TurretAttackKind,
    action: &'gameplay Action,
    weapon: &'gameplay Weapon,
    profile: &'gameplay AttackProfile,
}

#[derive(Debug, Clone)]
struct TurretEngagement {
    attacker_id: EntityId,
    action_name: String,
    kind: TurretAttackKind,
    target: ConcreteTargetSnapshot,
    can_hit: bool,
    timing: TurretEngagementTiming,
}

#[derive(Debug, Clone, Copy)]
struct TurretEngagementTiming {
    can_animate: bool,
    reset_attack_wait: bool,
    wait_timer_enabled: bool,
}

#[derive(Debug, Clone)]
struct ScanCandidate {
    target: ConcreteTargetSnapshot,
    priority: f32,
    dot: f32,
}

impl World {
    pub(in crate::world) fn grant_secondary_turret_scan_tokens(
        &mut self,
        gameplay: &GameplayCatalog,
    ) {
        let squad_ids = self
            .players
            .iter_mut()
            .filter_map(crate::player::Player::next_secondary_turret_scan_squad)
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            let unit_ids = self
                .squads
                .get(squad_id)
                .map(|squad| squad.unit_ids.clone())
                .unwrap_or_default();
            for unit_id in unit_ids {
                let hardpoints = self
                    .units
                    .get(unit_id)
                    .and_then(|unit| gameplay.object(&unit.proto_object_name))
                    .map(|object| {
                        secondary_actions(object)
                            .into_iter()
                            .filter_map(|action| {
                                action
                                    .profile
                                    .hardpoint
                                    .as_ref()
                                    .map(|hardpoint| hardpoint.name.clone())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if let Some(unit) = self.units.get_mut(unit_id) {
                    for hardpoint in hardpoints {
                        unit.turret_combat.grant_secondary_scan_token(&hardpoint);
                    }
                }
            }
        }
    }

    pub(super) fn advance_turret_attacks(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let unit_ids = self.units.iter().map(|(id, _)| id).collect::<Vec<_>>();
        let mut engagements = Vec::new();
        for unit_id in unit_ids {
            engagements.extend(self.collect_turret_engagements(unit_id, gameplay));
        }
        engagements.sort_by(|left, right| {
            left.attacker_id.cmp(&right.attacker_id).then_with(|| {
                left.action_name
                    .to_ascii_lowercase()
                    .cmp(&right.action_name.to_ascii_lowercase())
            })
        });

        let mut fire_events = Vec::new();
        for engagement in engagements {
            fire_events.extend(self.advance_turret_engagement(dt, gameplay, &engagement));
        }
        self.resolve_fire_events(fire_events, gameplay);
        for (_, unit) in self.units.iter_mut() {
            unit.turret_combat.advance_attack_wait_timers(dt);
        }
    }

    fn collect_turret_engagements(
        &mut self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Vec<TurretEngagement> {
        let Some((proto_object_name, action_updates)) = self.units.get(unit_id).map(|unit| {
            (
                unit.proto_object_name.clone(),
                unit.is_operational() || unit.is_garrisoned(),
            )
        }) else {
            return Vec::new();
        };
        let Some(object) = gameplay.object(&proto_object_name) else {
            return Vec::new();
        };
        if !action_updates {
            self.stop_inactive_turret_tracks(unit_id, &[], object);
            return Vec::new();
        }
        let Some(unit) = self.units.get(unit_id) else {
            return Vec::new();
        };
        let mut actions = secondary_actions(object);
        if let Some(slave) = slave_action(object, unit) {
            actions.push(slave);
        }
        actions.retain(|action| self.turret_action_enabled(unit, action.action));
        let active_names = actions
            .iter()
            .map(|action| action.action.name.clone())
            .collect::<Vec<_>>();
        self.stop_inactive_turret_tracks(unit_id, &active_names, object);

        actions
            .into_iter()
            .filter_map(|action| self.collect_turret_engagement(unit_id, action, gameplay))
            .collect()
    }

    fn collect_turret_engagement(
        &mut self,
        unit_id: EntityId,
        action: TurretAction<'_>,
        gameplay: &GameplayCatalog,
    ) -> Option<TurretEngagement> {
        if !self.ensure_turret_track(unit_id, action) {
            self.stop_turret_track(unit_id, &action.action.name);
            return None;
        }
        let waits = match action.kind {
            TurretAttackKind::Secondary => self.secondary_turret_action_waits(unit_id, action),
            TurretAttackKind::Slave => self.auxiliary_turret_action_blocked(unit_id, action.action),
        };
        if waits {
            if action.kind == TurretAttackKind::Slave {
                self.stop_turret_track(unit_id, &action.action.name);
            }
            return None;
        }
        let phase_before = self.secondary_turret_phase(unit_id, &action.action.name);
        let (wait_timer_on, was_animating) = self
            .units
            .get(unit_id)
            .and_then(|unit| unit.turret_combat.get(&action.action.name))
            .map_or((false, false), |state| {
                (state.attack_wait_on(), state.is_attacking())
            });
        let target = match action.kind {
            TurretAttackKind::Secondary => self.secondary_turret_target(unit_id, action, gameplay),
            TurretAttackKind::Slave => self.slave_turret_target(unit_id, action, gameplay),
        };
        let Some(target) = target else {
            self.stop_turret_track(unit_id, &action.action.name);
            return None;
        };
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.turret_combat
                .attack_mut(&action.action.name, action.kind)
                .set_target(target.id);
        }
        if action.kind == TurretAttackKind::Secondary
            && phase_before == Some(TurretAttackPhase::None)
        {
            return None;
        }
        let timer_enabled = action.action.disable_attack_wait_timer != Some(true);
        let can_hit = match action.kind {
            TurretAttackKind::Secondary => {
                wait_timer_on
                    || !self
                        .unit_squad(unit_id)
                        .is_some_and(crate::entities::Squad::is_changing_mode)
            }
            TurretAttackKind::Slave => self.units.get(unit_id).is_some_and(|unit| {
                unit.combat.is_animating() && unit.combat.engaged_target() == Some(target.id)
            }),
        };
        Some(TurretEngagement {
            attacker_id: unit_id,
            action_name: action.action.name.clone(),
            kind: action.kind,
            target,
            can_hit,
            timing: TurretEngagementTiming {
                can_animate: was_animating || !wait_timer_on,
                reset_attack_wait: timer_enabled
                    && !wait_timer_on
                    && (action.kind == TurretAttackKind::Secondary || !was_animating),
                wait_timer_enabled: timer_enabled,
            },
        })
    }

    fn ensure_turret_track(&mut self, unit_id: EntityId, action: TurretAction<'_>) -> bool {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        let anchor = unit
            .turret_combat
            .attack_mut(&action.action.name, action.kind)
            .timing
            .hardpoint_anchor(action.profile);
        unit.combat
            .claim_auxiliary_hardpoint(action.profile, anchor.as_ref())
    }

    fn stop_inactive_turret_tracks(
        &mut self,
        unit_id: EntityId,
        active_names: &[String],
        object: &ObjectGameplay,
    ) {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        let stale = unit
            .turret_combat
            .attacks()
            .filter(|attack| {
                !active_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(attack.action_name()))
            })
            .map(|attack| attack.action_name().to_owned())
            .collect::<Vec<_>>();
        for action_name in stale {
            if let Some(profile) = object.attack_profile(&action_name) {
                unit.combat.release_auxiliary_hardpoint(profile);
            }
        }
        unit.turret_combat.stop_except(active_names);
    }

    fn stop_turret_track(&mut self, unit_id: EntityId, action_name: &str) {
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.turret_combat.stop(action_name);
        }
    }

    fn turret_action_enabled(&self, unit: &Unit, action: &Action) -> bool {
        let authored_enabled = action.start_disabled != Some(true);
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        unit.logical_proto_object_name(),
                        &action.name,
                        authored_enabled,
                    )
                });
        unit.actions.is_enabled(&action.name, !player_enabled)
    }

    fn secondary_turret_action_waits(
        &mut self,
        unit_id: EntityId,
        action: TurretAction<'_>,
    ) -> bool {
        let Some(mut phase) = self.secondary_turret_phase(unit_id, &action.action.name) else {
            return true;
        };
        let requires_lockdown = action.action.requires_lockdown == Some(true);
        let locked_down = self.unit_squad(unit_id).is_some_and(|squad| {
            squad.mode == crate::entities::SquadMode::Lockdown && !squad.is_changing_mode()
        });
        if requires_lockdown && !locked_down {
            self.set_secondary_turret_phase(
                unit_id,
                &action.action.name,
                TurretAttackPhase::Paused,
            );
            return true;
        }
        if phase == TurretAttackPhase::Paused {
            self.set_secondary_turret_phase(unit_id, &action.action.name, TurretAttackPhase::None);
            phase = TurretAttackPhase::None;
        }

        if self.turret_attack_is_blocked(unit_id) {
            match phase {
                TurretAttackPhase::None => {
                    if self.consume_secondary_scan_token(unit_id, action.profile) {
                        self.set_secondary_turret_phase(
                            unit_id,
                            &action.action.name,
                            TurretAttackPhase::Blocked,
                        );
                    }
                }
                TurretAttackPhase::Working => self.set_secondary_turret_phase(
                    unit_id,
                    &action.action.name,
                    TurretAttackPhase::Blocked,
                ),
                TurretAttackPhase::Blocked | TurretAttackPhase::Paused => {}
            }
            return true;
        }
        if phase == TurretAttackPhase::Blocked {
            self.set_secondary_turret_phase(unit_id, &action.action.name, TurretAttackPhase::None);
            return true;
        }
        false
    }

    fn secondary_turret_phase(
        &self,
        unit_id: EntityId,
        action_name: &str,
    ) -> Option<TurretAttackPhase> {
        self.units
            .get(unit_id)?
            .turret_combat
            .get(action_name)
            .map(crate::entities::TurretAttackState::phase)
    }

    fn set_secondary_turret_phase(
        &mut self,
        unit_id: EntityId,
        action_name: &str,
        phase: TurretAttackPhase,
    ) {
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.turret_combat.set_phase(action_name, phase);
        }
    }

    fn consume_secondary_scan_token(&mut self, unit_id: EntityId, profile: &AttackProfile) -> bool {
        let Some(hardpoint) = profile.hardpoint.as_ref() else {
            return false;
        };
        self.units.get_mut(unit_id).is_some_and(|unit| {
            unit.turret_combat
                .consume_secondary_scan_token(&hardpoint.name)
        })
    }

    fn turret_attack_is_blocked(&self, unit_id: EntityId) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return true;
        };
        unit.is_move_air_attack_blocked() || unit.garrison.is_contained()
    }

    fn auxiliary_turret_action_blocked(&self, unit_id: EntityId, action: &Action) -> bool {
        self.turret_attack_is_blocked(unit_id)
            || !self.turret_parent_order_allows_attacking(unit_id)
            || action.requires_lockdown == Some(true)
                && !self.unit_squad(unit_id).is_some_and(|squad| {
                    squad.mode == crate::entities::SquadMode::Lockdown && !squad.is_changing_mode()
                })
    }

    fn turret_parent_order_allows_attacking(&self, unit_id: EntityId) -> bool {
        self.unit_squad(unit_id)
            .is_none_or(|squad| squad.gather_target().is_none())
    }

    fn unit_squad(&self, unit_id: EntityId) -> Option<&crate::entities::Squad> {
        self.units
            .get(unit_id)?
            .squad_id
            .and_then(|squad_id| self.squads.get(squad_id))
    }

    fn secondary_turret_target(
        &mut self,
        unit_id: EntityId,
        action: TurretAction<'_>,
        gameplay: &GameplayCatalog,
    ) -> Option<ConcreteTargetSnapshot> {
        let phase = self.secondary_turret_phase(unit_id, &action.action.name)?;
        if !self.turret_parent_order_allows_attacking(unit_id) {
            if phase == TurretAttackPhase::None {
                self.consume_secondary_scan_token(unit_id, action.profile);
            }
            return None;
        }
        let (current, preferred, wait_timer_on) = self.units.get(unit_id).map(|unit| {
            (
                unit.turret_combat
                    .get(&action.action.name)
                    .and_then(crate::entities::TurretAttackState::target),
                unit.combat.engaged_target(),
                unit.turret_combat
                    .get(&action.action.name)
                    .is_some_and(crate::entities::TurretAttackState::attack_wait_on),
            )
        })?;

        if let Some(current) = current {
            let target = self.secondary_candidate(
                unit_id,
                current,
                action,
                gameplay,
                Some(current) == preferred,
            )?;
            if !wait_timer_on
                && let Some(preferred) = preferred.filter(|preferred| *preferred != current)
                && self
                    .secondary_candidate(unit_id, preferred, action, gameplay, true)
                    .is_some()
            {
                return None;
            }
            return Some(target.target);
        }

        let has_scan_token = self.consume_secondary_scan_token(unit_id, action.profile);
        if !has_scan_token {
            return None;
        }

        self.secondary_scan_candidates(unit_id, preferred, action, gameplay)
            .into_iter()
            .max_by(compare_scan_candidates)
            .map(|candidate| candidate.target)
    }

    fn secondary_scan_candidates(
        &self,
        unit_id: EntityId,
        preferred: Option<EntityId>,
        action: TurretAction<'_>,
        gameplay: &GameplayCatalog,
    ) -> Vec<ScanCandidate> {
        let Some(attacker) = self.units.get(unit_id) else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for (squad_id, squad) in self.squads.iter() {
            if !self.players_are_enemies(attacker.base.player_id, squad.base.player_id)
                || !self.is_entity_visible_from_player(attacker.base.player_id, squad_id)
            {
                continue;
            }
            let target_id = preferred
                .filter(|preferred| squad.unit_ids.contains(preferred))
                .or_else(|| squad.unit_ids.first().copied());
            let Some(target_id) = target_id else {
                continue;
            };
            if let Some(candidate) = self.secondary_candidate(
                unit_id,
                target_id,
                action,
                gameplay,
                Some(target_id) == preferred,
            ) {
                candidates.push(candidate);
            }
        }
        candidates
    }

    fn secondary_candidate(
        &self,
        unit_id: EntityId,
        target_id: EntityId,
        action: TurretAction<'_>,
        gameplay: &GameplayCatalog,
        preferred: bool,
    ) -> Option<ScanCandidate> {
        let attacker = self.units.get(unit_id)?;
        let target_unit = self.units.get(target_id)?;
        if !target_unit.is_auto_attackable()
            || !self.players_are_enemies(attacker.base.player_id, target_unit.base.player_id)
        {
            return None;
        }
        let target = self.concrete_attack_target(attacker.base.player_id, target_id)?;
        if !self.turret_target_in_range(unit_id, &target, action.profile) {
            return None;
        }
        let query = self.turret_attack_query(attacker, &target);
        if !gameplay.attack_action_matches_target(
            &attacker.proto_object_name,
            &action.action.name,
            &query,
        ) {
            return None;
        }
        let (anchor, aim_position, origin, forward) =
            self.turret_aim_frame(unit_id, &target, action.profile, gameplay)?;
        let unit_world = unit_world_transform(attacker)?;
        if !action.profile.orientation.skips_orientation_check()
            && !attacker.combat.can_orient_auxiliary_hardpoint(
                action.profile,
                anchor.as_ref(),
                unit_world,
                aim_position,
            )
        {
            return None;
        }
        let direction = (aim_position - origin).try_normalize().unwrap_or(Vec3::Z);
        let priority = if preferred {
            2_147_483_648.0
        } else {
            gameplay.attack_target_priority(&target.proto_object_name, action.weapon)
        };
        (priority >= 0.0).then_some(ScanCandidate {
            target,
            priority,
            dot: forward.dot(direction),
        })
    }

    fn slave_turret_target(
        &self,
        unit_id: EntityId,
        action: TurretAction<'_>,
        gameplay: &GameplayCatalog,
    ) -> Option<ConcreteTargetSnapshot> {
        let unit = self.units.get(unit_id)?;
        let target_id = unit.combat.engaged_target()?;
        let target = self.concrete_attack_target(unit.base.player_id, target_id)?;
        (gameplay.attack_target_priority(&target.proto_object_name, action.weapon) > 0.0
            && self.turret_target_in_range(unit_id, &target, action.profile))
        .then_some(target)
    }

    fn turret_attack_query<'target>(
        &self,
        attacker: &Unit,
        target: &'target ConcreteTargetSnapshot,
    ) -> AttackQuery<'target> {
        let mut flags = AttackQueryFlags::empty();
        flags.insert(AttackQueryFlags::AUTO_TARGET);
        if target.player_id == 0 {
            flags.insert(AttackQueryFlags::TARGET_GAIA);
        }
        if target.damaged {
            flags.insert(AttackQueryFlags::TARGET_DAMAGED);
        }
        if target.unbuilt {
            flags.insert(AttackQueryFlags::TARGET_UNBUILT);
        }
        if target.in_cover {
            flags.insert(AttackQueryFlags::TARGET_IN_COVER);
        }
        AttackQuery {
            relation: if attacker.base.player_id == target.player_id {
                TacticRelation::SelfPlayer
            } else {
                match self.player_relation(attacker.base.player_id, target.player_id) {
                    Some(TeamRelation::Ally) => TacticRelation::Ally,
                    Some(TeamRelation::Enemy) => TacticRelation::Enemy,
                    Some(TeamRelation::Neutral) | None => TacticRelation::Neutral,
                }
            },
            squad_mode: attacker
                .squad_id
                .and_then(|squad_id| self.squads.get(squad_id))
                .map_or(crate::entities::SquadMode::Normal, |squad| squad.mode),
            ability_id: None,
            target_proto_object_name: Some(&target.proto_object_name),
            tactic_state: attacker.tactic_state(),
            flags,
        }
    }

    fn turret_target_in_range(
        &self,
        attacker_id: EntityId,
        target: &ConcreteTargetSnapshot,
        profile: &AttackProfile,
    ) -> bool {
        let Some(unit) = self.units.get(attacker_id) else {
            return false;
        };
        let range = self
            .get_player(unit.base.player_id)
            .map_or(profile.max_range, |player| {
                player.technologies.weapon_range(
                    unit.logical_proto_object_name(),
                    &profile.weapon_name,
                    profile.max_range,
                )
            });
        let range = selected_range(0.0, range, unit.weapon_range_scalar);
        AttackRange::new(profile.min_range, range).relation(
            unit.base.position,
            unit.obstruction_radius(),
            target.position,
            target.collision_radius,
        ) == AttackRangeRelation::Within
    }

    fn turret_aim_frame(
        &self,
        attacker_id: EntityId,
        target: &ConcreteTargetSnapshot,
        profile: &AttackProfile,
        gameplay: &GameplayCatalog,
    ) -> Option<(
        Option<crate::gameplay::AttackAnimationAnchor>,
        Vec3,
        Vec3,
        Vec3,
    )> {
        let unit = self.units.get(attacker_id)?;
        let state = unit.turret_combat.get(&profile.action_name)?;
        let anchor = state.timing.hardpoint_anchor(profile);
        let attacker = self.turret_attacker_snapshot(attacker_id, profile, anchor.as_ref())?;
        let tuning = self.launch_tuning(&attacker, profile);
        let aim_position = attack_aim_position(&attacker, target, profile, gameplay, &tuning);
        let unit_world = unit_world_transform(unit)?;
        let origin = attacker.hardpoint_position.unwrap_or(attacker.position);
        let forward = unit
            .combat
            .auxiliary_hardpoint_forward(profile, anchor.as_ref(), unit_world)
            .unwrap_or(unit.base.forward);
        Some((anchor, aim_position, origin, forward))
    }

    fn turret_attacker_snapshot(
        &self,
        attacker_id: EntityId,
        profile: &AttackProfile,
        anchor: Option<&crate::gameplay::AttackAnimationAnchor>,
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
            unit.combat
                .hardpoint_yaw_origin(profile, anchor, unit_world)
        });
        Some(AttackerSnapshot {
            player_id: unit.base.player_id,
            position: unit.base.position,
            obstruction_radius: unit.obstruction_radius(),
            launch_position: unit.simulation_center(),
            hardpoint_position,
            damage_multiplier: unit.effective_damage_multiplier(),
            range_scalar: unit.weapon_range_scalar,
            authored_range,
            logical_proto_object_name: unit.logical_proto_object_name().to_owned(),
            accuracy_scalar: unit.accuracy_scalar,
            dodge_scalar: unit.dodge_scalar,
            moving_at_full_speed: speed > super::MOVEMENT_EPSILON && speed >= desired_speed * 0.9,
        })
    }

    fn advance_turret_engagement(
        &mut self,
        dt: f32,
        gameplay: &GameplayCatalog,
        engagement: &TurretEngagement,
    ) -> Vec<FireEvent> {
        let Some(object) = self
            .units
            .get(engagement.attacker_id)
            .and_then(|unit| gameplay.object(&unit.proto_object_name))
        else {
            return Vec::new();
        };
        let Some(profile) = object.attack_profile(&engagement.action_name) else {
            return Vec::new();
        };
        let anchor = self
            .units
            .get(engagement.attacker_id)
            .and_then(|unit| unit.turret_combat.get(&engagement.action_name))
            .and_then(|state| state.timing.hardpoint_anchor(profile));
        let Some(attacker) =
            self.turret_attacker_snapshot(engagement.attacker_id, profile, anchor.as_ref())
        else {
            return Vec::new();
        };
        let tuning = self.launch_tuning(&attacker, profile);
        let aim_position =
            attack_aim_position(&attacker, &engagement.target, profile, gameplay, &tuning);
        let normal_range = tuning.max_range;
        let authored_damage =
            self.authored_attack_damage(engagement.attacker_id, &attacker, profile);
        let Some(advance) = self.advance_turret_timeline(
            dt,
            engagement,
            profile,
            anchor.as_ref(),
            aim_position,
            authored_damage,
        ) else {
            return Vec::new();
        };
        let damage = scaled_launch_damage(
            authored_damage,
            attacker.damage_multiplier,
            attacker.position,
            engagement.target.position,
            profile.uses_height_bonus_damage,
            gameplay.height_bonus_damage(),
        );
        let area_damage = self.launch_area_damage(
            attacker.player_id,
            &attacker.logical_proto_object_name,
            profile,
        );
        let mut fire_events = Vec::new();
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
                    engagement.target.clone(),
                    damage,
                    profile,
                    area_damage,
                    normal_range,
                )
                .with_launch_tuning(tuning),
            );
        }
        fire_events
    }

    fn advance_turret_timeline(
        &mut self,
        dt: f32,
        engagement: &TurretEngagement,
        profile: &AttackProfile,
        anchor: Option<&crate::gameplay::AttackAnimationAnchor>,
        aim_position: Vec3,
        authored_damage: f32,
    ) -> Option<AttackAdvance> {
        let (units, rng) = (&mut self.units, &mut self.rng);
        let unit = units.get_mut(engagement.attacker_id)?;
        let unit_world = unit_world_transform(unit)?;
        let oriented = match engagement.kind {
            TurretAttackKind::Secondary => unit.combat.orient_auxiliary_hardpoint(
                profile,
                anchor,
                unit_world,
                aim_position,
                dt,
            ),
            TurretAttackKind::Slave => {
                unit.combat
                    .orient_slave_hardpoint(profile, anchor, unit_world, aim_position, dt)
            }
        };
        if !oriented || !engagement.can_hit {
            unit.turret_combat.stop(&engagement.action_name);
            return None;
        }
        if !engagement.timing.can_animate {
            return Some(AttackAdvance::default());
        }
        let (turrets, ammunition) = (&mut unit.turret_combat, &mut unit.ammunition);
        let state = turrets.attack_mut(&engagement.action_name, engagement.kind);
        state.set_target(engagement.target.id);
        let advance = state.timing.advance(
            dt,
            engagement.target.id,
            profile,
            ammunition,
            authored_damage,
            rng,
        );
        if advance.result.is_some() {
            state.timing.reset();
        }
        let attack_tag_resets_wait = engagement.kind == TurretAttackKind::Slave
            && advance.events.iter().any(|occurrence| {
                matches!(
                    occurrence.event.kind,
                    crate::gameplay::AttackAnimationEventKind::Attack { .. }
                )
            });
        if engagement.timing.wait_timer_enabled
            && (engagement.timing.reset_attack_wait || attack_tag_resets_wait)
        {
            let animation_index = advance
                .events
                .last()
                .map(|occurrence| occurrence.animation_index)
                .or_else(|| state.animation_index())
                .unwrap_or_default();
            let duration = profile
                .animations
                .get(animation_index)
                .map_or(0.0, |animation| animation.duration);
            state.set_attack_wait(duration);
        }
        Some(advance)
    }
}

fn secondary_actions(object: &ObjectGameplay) -> Vec<TurretAction<'_>> {
    let tactics = object.tactics();
    let persistent = tactics
        .tactic
        .as_ref()
        .map(|rules| rules.persistent_actions.as_slice())
        .unwrap_or_default();
    tactics
        .actions
        .iter()
        .filter(|action| {
            is_action_type(action, "SecondaryTurretAttack")
                || persistent.iter().any(|name| {
                    name.eq_ignore_ascii_case(&action.name)
                        && action
                            .persistent_action_type
                            .as_deref()
                            .is_some_and(|kind| kind.eq_ignore_ascii_case("SecondaryTurretAttack"))
                })
        })
        .filter_map(|action| resolve_turret_action(object, action, TurretAttackKind::Secondary))
        .collect()
}

fn slave_action<'gameplay>(
    object: &'gameplay ObjectGameplay,
    unit: &Unit,
) -> Option<TurretAction<'gameplay>> {
    let parent_name = unit.combat.action_name()?;
    unit.combat.engaged_target()?;
    let parent = object
        .tactics()
        .actions
        .iter()
        .find(|action| action.name.eq_ignore_ascii_case(parent_name))?;
    let slave_name = parent.slave_attack_action.as_deref()?;
    let slave = object
        .tactics()
        .actions
        .iter()
        .find(|action| action.name.eq_ignore_ascii_case(slave_name))?;
    is_action_type(slave, "SlaveTurretAttack")
        .then(|| resolve_turret_action(object, slave, TurretAttackKind::Slave))?
}

fn resolve_turret_action<'gameplay>(
    object: &'gameplay ObjectGameplay,
    action: &'gameplay Action,
    kind: TurretAttackKind,
) -> Option<TurretAction<'gameplay>> {
    let weapon_name = action.weapon.as_deref()?;
    let weapon = object
        .tactics()
        .weapons
        .iter()
        .find(|weapon| weapon.name.eq_ignore_ascii_case(weapon_name))?;
    let profile = object.attack_profile(&action.name)?;
    let hardpoint = profile.hardpoint.as_ref()?;
    match kind {
        TurretAttackKind::Secondary => hardpoint.yaw_attachment()?,
        TurretAttackKind::Slave => hardpoint.pitch_attachment()?,
    };
    Some(TurretAction {
        kind,
        action,
        weapon,
        profile,
    })
}

fn is_action_type(action: &Action, expected: &str) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case(expected))
}

fn compare_scan_candidates(left: &ScanCandidate, right: &ScanCandidate) -> Ordering {
    left.priority
        .total_cmp(&right.priority)
        .then_with(|| left.dot.total_cmp(&right.dot))
        .then_with(|| right.target.id.cmp(&left.target.id))
}
