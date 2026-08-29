//! Authoritative attack-order targeting and pursuit.

mod helpers;
#[cfg(test)]
mod test_catalog;

use super::World;
use crate::entities::projectiles::{ProjectileLaunch, ProjectileStep};
use crate::entities::{Projectile, Squad, SquadMode, SquadState, Unit, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AttackProfile, AttackQuery, AttackQueryFlags, GameplayCatalog, RangedAction, TacticRelation,
};
use crate::player::{PlayerId, TeamRelation};
use glam::Vec3;
use helpers::{face_position, scaled_launch_damage, selected_range, xz_distance_squared};

const MIN_TARGET_RADIUS: f32 = 0.5;

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
struct ConcreteTargetSnapshot {
    id: EntityId,
    player_id: PlayerId,
    position: Vec3,
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
    target: ConcreteTargetSnapshot,
    damage: f32,
    weapon_type: Option<String>,
    projectile_name: Option<String>,
}

#[derive(Debug, Clone)]
struct ProjectileImpact {
    target_id: EntityId,
    damage: f32,
    weapon_type: Option<String>,
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

        if self
            .squads
            .get(recipient_id)
            .is_some_and(|squad| squad.base.player_id == player_id)
        {
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
            let Some((
                source_player_id,
                source_position,
                damage_multiplier,
                range_scalar,
                authored_range,
                source_proto,
            )) = self
                .units
                .get(engagement.attacker_id)
                .filter(|unit| unit.is_alive() && !unit.is_garrisoned())
                .map(|unit| {
                    let authored_range =
                        self.get_player(unit.base.player_id)
                            .map_or(profile.max_range, |player| {
                                player.technologies.weapon_range(
                                    &unit.proto_object_name,
                                    &profile.weapon_name,
                                    profile.max_range,
                                )
                            });
                    (
                        unit.base.player_id,
                        unit.base.position,
                        unit.damage_multiplier,
                        unit.weapon_range_scalar,
                        authored_range,
                        unit.proto_object_name.clone(),
                    )
                })
            else {
                continue;
            };
            let range = selected_range(engagement.range_override, authored_range, range_scalar);
            if !self.players_are_enemies(source_player_id, target.player_id)
                || xz_distance_squared(source_position, target.position) > range * range
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
                self.get_player(source_player_id)
                    .map_or(profile.damage_per_attack, |player| {
                        player.technologies.weapon_damage(
                            &source_proto,
                            &profile.weapon_name,
                            profile.damage_per_attack,
                        )
                    });
            let damage = scaled_launch_damage(
                authored_damage,
                damage_multiplier,
                source_position,
                target.position,
                profile.uses_height_bonus_damage,
                gameplay.height_bonus_damage(),
            );
            fire_events.extend((0..advance.hit_count).map(|_| FireEvent {
                source_id: engagement.attacker_id,
                source_player_id,
                source_position,
                target: target.clone(),
                damage,
                weapon_type: profile.weapon_type.clone(),
                projectile_name: profile.projectile.clone(),
            }));
        }
        self.finish_completed_ability_attacks(gameplay);
        for event in fire_events {
            self.resolve_fire_event(event, gameplay);
        }
    }

    fn resolve_fire_event(&mut self, event: FireEvent, gameplay: &GameplayCatalog) {
        let Some(projectile_name) = event.projectile_name.as_deref() else {
            self.apply_weapon_damage(
                event.target.id,
                event.damage,
                event.weapon_type.as_deref(),
                Some(gameplay),
            );
            return;
        };
        let Some(profile) = gameplay.projectile(projectile_name) else {
            return;
        };
        let id = self.projectiles.allocate_id();
        let projectile = Projectile::new(
            id,
            event.source_player_id,
            ProjectileLaunch {
                source_id: event.source_id,
                target_id: event.target.id,
                source_position: event.source_position,
                target_position: event.target.position,
                target_radius: event.target.collision_radius,
                damage: event.damage,
                weapon_type: event.weapon_type,
            },
            profile,
        );
        self.projectiles.insert(id, projectile);
    }

    pub(super) fn update_projectiles(&mut self, dt: f32, gameplay: Option<&GameplayCatalog>) {
        let gravity = gameplay.map_or(0.0, GameplayCatalog::projectile_gravity);
        let projectile_ids = self.projectiles.ids().collect::<Vec<_>>();
        let mut impacts = Vec::new();
        let mut finished = Vec::new();
        for projectile_id in projectile_ids {
            let live_target_position = self
                .projectiles
                .get(projectile_id)
                .and_then(|projectile| self.units.get(projectile.target_id))
                .filter(|unit| unit.is_alive() && !unit.is_garrisoned())
                .map(|unit| unit.base.position);
            let Some(projectile) = self.projectiles.get_mut(projectile_id) else {
                continue;
            };
            match projectile.advance(dt, live_target_position, gravity) {
                ProjectileStep::Flying => {}
                ProjectileStep::Impact => {
                    impacts.push(ProjectileImpact {
                        target_id: projectile.target_id,
                        damage: projectile.damage,
                        weapon_type: projectile.weapon_type.clone(),
                    });
                    finished.push(projectile_id);
                }
                ProjectileStep::Expired => finished.push(projectile_id),
            }
        }
        for projectile_id in finished {
            let _removed = self.projectiles.remove(projectile_id);
        }
        for impact in impacts {
            self.apply_weapon_damage(
                impact.target_id,
                impact.damage,
                impact.weapon_type.as_deref(),
                gameplay,
            );
        }
    }

    pub(super) fn apply_weapon_damage(
        &mut self,
        target_id: EntityId,
        damage: f32,
        weapon_type: Option<&str>,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let Some(target) = self
            .units
            .get(target_id)
            .filter(|target| target.is_alive() && !target.is_garrisoned())
        else {
            return;
        };
        let weapon_modifier = gameplay.map_or(1.0, |catalog| {
            catalog.weapon_damage_modifier(weapon_type, &target.proto_object_name)
        });
        let construction_modifier = if target.is_building() && !target.built {
            self.construction_damage_multiplier
        } else {
            1.0
        };
        let final_damage =
            damage * weapon_modifier * construction_modifier * target.damage_taken_multiplier;
        if !final_damage.is_finite() || final_damage <= 0.0 {
            return;
        }
        let _damaged = self.damage_unit(target_id, final_damage);
    }

    fn stop_unit_firing(&mut self, unit_ids: &[EntityId]) {
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
            .filter(|unit| unit.is_alive() && !unit.is_garrisoned())
        {
            return Some(concrete_target_snapshot(requested_id, unit));
        }
        let squad = self
            .squads
            .get(requested_id)
            .filter(|squad| squad.is_alive() && !squad.garrison.is_garrisoned())?;
        squad
            .unit_ids
            .iter()
            .copied()
            .filter_map(|unit_id| {
                self.units
                    .get(unit_id)
                    .filter(|unit| unit.is_alive() && !unit.is_garrisoned())
                    .map(|unit| (unit_id, unit))
            })
            .min_by_key(|(unit_id, _)| *unit_id)
            .map(|(unit_id, unit)| concrete_target_snapshot(unit_id, unit))
    }

    fn attack_target_snapshot(&self, requested_id: EntityId) -> Option<TargetSnapshot> {
        if let Some(unit) = self
            .units
            .get(requested_id)
            .filter(|unit| unit.is_alive() && !unit.is_garrisoned())
        {
            if let Some(squad_id) = unit.squad_id
                && let Some(squad) = self.squads.get(squad_id).filter(|squad| {
                    squad.is_alive()
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
            squad.is_alive() && !squad.garrison.is_garrisoned() && !squad.unit_ids.is_empty()
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
        collision_radius: unit
            .obstruction_half_extents
            .abs()
            .max_element()
            .max(MIN_TARGET_RADIUS),
        proto_object_name: unit.proto_object_name.clone(),
        damaged: unit.hitpoints < unit.max_hitpoints,
        unbuilt: unit.is_building() && !unit.built,
    }
}

#[cfg(test)]
mod tests {
    use super::test_catalog::ability_catalog;
    use super::*;
    use crate::gameplay::AttackAnimation;
    use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
    use pipeline::database::hw1::weapontypes::DamageModifier;
    use pipeline::database::hw1::{Database, ProtoObject, WeaponType};

    fn combat_catalog() -> GameplayCatalog {
        let mut database = Database::new();
        database.objects.extend([
            ProtoObject {
                name: "test_attacker".to_owned(),
                tactics: Some("test_attacker.tactics".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "test_target".to_owned(),
                damage_type: Some("Light".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "test_bullet".to_owned(),
                dbid: Some(70),
                object_class: Some("Projectile".to_owned()),
                velocity: Some(10.0),
                lifespan: Some(2.0),
                ..ProtoObject::default()
            },
        ]);
        database.weapon_types.push(WeaponType {
            name: "SmallArms".to_owned(),
            damage_modifiers: vec![DamageModifier {
                damage_type: "Light".to_owned(),
                modifier: 2.0,
                ..DamageModifier::default()
            }],
            ..WeaponType::default()
        });
        let tactics = TacticData {
            actions: vec![Action {
                name: "RifleAttack".to_owned(),
                action_type: Some("RangedAttack".to_owned()),
                weapon: Some("Rifle".to_owned()),
                ..Action::default()
            }],
            weapons: vec![Weapon {
                name: "Rifle".to_owned(),
                max_range: Some(10.0),
                ..Weapon::default()
            }],
            ..TacticData::default()
        };
        let profile = AttackProfile {
            action_name: "RifleAttack".to_owned(),
            weapon_name: "Rifle".to_owned(),
            weapon_type: Some("SmallArms".to_owned()),
            projectile: Some("test_bullet".to_owned()),
            max_range: 10.0,
            damage_per_attack: 5.0,
            animations: vec![AttackAnimation {
                asset_path: "test_attack.uax".to_owned(),
                weight: 1,
                duration: 0.1,
                attack_positions: vec![0.0],
            }],
            pre_attack_cooldown: [0.0, 0.0],
            post_attack_cooldown: [0.0, 0.0],
            reload_duration: 0.0,
            visual_ammo: 0,
            uses_height_bonus_damage: false,
        };
        GameplayCatalog::from_test_profiles(
            &database,
            [("test_attacker".to_owned(), tactics)],
            [("test_attacker".to_owned(), profile)],
        )
    }

    fn combat_world() -> (World, EntityId, EntityId) {
        let mut world = World::with_seed(41);
        world.init_players(2);
        world.get_player_mut(1).expect("player 1").team_id = 1;
        world.get_player_mut(2).expect("player 2").team_id = 2;
        world.configure_standard_team_relations();
        let attacker_id = world.create_unit_at(1, Vec3::ZERO);
        let target_id = world.create_unit_at(2, Vec3::X);
        world
            .get_unit_mut(attacker_id)
            .expect("attacker")
            .proto_object_name = "test_attacker".to_owned();
        let target = world.get_unit_mut(target_id).expect("target");
        target.proto_object_name = "test_target".to_owned();
        target.damage_taken_multiplier = 0.5;
        assert!(world.issue_attack_order(1, attacker_id, target_id, 0.0));
        (world, attacker_id, target_id)
    }

    #[test]
    fn authored_tag_launches_projectile_and_impact_mutates_sim_hitpoints() {
        let gameplay = combat_catalog();
        let (mut world, attacker_id, target_id) = combat_world();

        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(world.projectiles.len(), 1);
        assert!(
            (world.get_unit(target_id).expect("target").hitpoints - 100.0).abs() < f32::EPSILON
        );
        let projectile = world.projectiles.iter().next().expect("projectile").1;
        assert_eq!(projectile.source_id, attacker_id);
        assert_eq!(projectile.target_id, target_id);

        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.projectiles.is_empty());
        assert!((world.get_unit(target_id).expect("target").hitpoints - 95.0).abs() < f32::EPSILON);

        let (mut repeat, _, repeat_target_id) = combat_world();
        repeat.update_entities_with_gameplay(0.05, &gameplay);
        repeat.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(world.checksum_with_rng(), repeat.checksum_with_rng());
        assert!(
            (repeat
                .get_unit(repeat_target_id)
                .expect("repeat target")
                .hitpoints
                - 95.0)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn attack_move_acquires_an_enemy_then_resumes_its_destination() {
        let gameplay = combat_catalog();
        let mut world = World::with_seed(51);
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let attacker_id = world.create_unit_at(1, Vec3::ZERO);
        world.get_unit_mut(attacker_id).unwrap().proto_object_name = "test_attacker".to_owned();
        assert!(world.attach_unit_to_squad(attacker_id, squad_id));
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.aggro_distance = 20.0;
        squad.leash_distance = 30.0;
        let target_id = world.create_unit_at(2, Vec3::new(5.0, 0.0, 0.0));
        world.get_unit_mut(target_id).unwrap().proto_object_name = "test_target".to_owned();
        let destination = Vec3::new(40.0, 0.0, 0.0);
        assert!(world.issue_squad_move_order_to_position(1, squad_id, destination, true, false,));

        world.update_entities_with_gameplay(0.05, &gameplay);
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.state, SquadState::Attacking);
        assert!(squad.is_auto_attack_engagement());
        assert_eq!(squad.attack_target, Some(target_id));

        world.get_unit_mut(target_id).unwrap().kill();
        world.update_entities_with_gameplay(0.05, &gameplay);
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.state, SquadState::Moving);
        assert_eq!(squad.move_target, Some(destination));
        assert_eq!(squad.attack_target, None);
    }

    #[test]
    fn unbuilt_targets_use_the_database_construction_damage_multiplier() {
        let mut world = World::new();
        let target_id = world.create_building(1);
        let target = world.get_building_mut(target_id).unwrap();
        target.built = false;
        world.set_construction_damage_multiplier(Some(3.0));

        world.apply_weapon_damage(target_id, 10.0, None, None);
        assert!((world.get_building(target_id).unwrap().hitpoints - 70.0).abs() < f32::EPSILON);

        world.get_building_mut(target_id).unwrap().built = true;
        world.apply_weapon_damage(target_id, 10.0, None, None);
        assert!((world.get_building(target_id).unwrap().hitpoints - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn ability_context_changes_pursuit_range_and_runtime_attack_action() {
        let gameplay = ability_catalog();
        let (mut world, attacker_id, target_id) = combat_world();
        world.get_unit_mut(target_id).unwrap().base.position = Vec3::new(30.0, 0.0, 0.0);

        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.get_unit(attacker_id).unwrap().base.position.x > 0.0);
        assert!((world.get_unit(target_id).unwrap().hitpoints - 100.0).abs() < f32::EPSILON);

        assert!(world.issue_attack_order_with_context(
            1,
            attacker_id,
            target_id,
            0.0,
            None,
            Some(0),
        ));
        world.update_entities_with_gameplay(0.05, &gameplay);

        assert_eq!(
            world.get_unit(attacker_id).unwrap().combat.action_name(),
            Some("GrenadeAttack")
        );
        assert!(world.get_unit(target_id).unwrap().hitpoints < 100.0);
    }
}
