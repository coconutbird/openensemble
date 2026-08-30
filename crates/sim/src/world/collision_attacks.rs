//! Deterministic contact-driven vehicle Ram resolution.

use super::{GeneralEvent, GeneralEventType, World, combat::AttackDamage};
use crate::entities::{Squad, SquadMode};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AbilityRecoveryStart, CollisionAttackProfile, GameplayCatalog};
use crate::physics::{UnitCollisionContact, resolve_unit_collisions};
use crate::player::{GAIA_PLAYER, PlayerId};
use glam::Vec3;

const RAMMER_IMPULSE_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
const RAMMER_IMPULSE_FORCE: f32 = 10.0;
const RAMMED_IMPULSE_ANGLE: f32 = std::f32::consts::FRAC_PI_2 * 0.75;
const RAMMED_IMPULSE_FORCE: f32 = 7.0;
const RAMMED_MEDIUM_UP_VELOCITY: f32 = 3.0;
const RAMMED_TOP_UP_VELOCITY: f32 = 6.0;

#[derive(Debug, Clone)]
struct RammerSnapshot {
    id: EntityId,
    squad_id: EntityId,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    proto_object_name: String,
    ordered_target_id: EntityId,
    ability_id: Option<u8>,
    damage_multiplier: f32,
}

#[derive(Debug, Clone)]
struct RamTargetSnapshot {
    id: EntityId,
    squad_id: Option<EntityId>,
    player_id: PlayerId,
    position: Vec3,
    forward: Vec3,
    velocity: Vec3,
    proto_object_name: String,
    mode: SquadMode,
    hitpoints: f32,
    obstruction_radius: f32,
    in_cover: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollisionDisposition {
    Bowlable,
    Rammable,
}

#[derive(Debug, Clone, Copy)]
struct RamWeaponTuning {
    damage_cap: f32,
    reflect_damage_factor: f32,
}

impl World {
    pub(in crate::world) fn resolve_collisions_and_attacks(
        &mut self,
        gameplay: Option<&GameplayCatalog>,
    ) {
        let excluded_squads = self
            .squads
            .iter()
            .filter_map(|(id, squad)| (squad.is_being_pulled() || squad.is_jumping()).then_some(id))
            .collect();
        let contacts = resolve_unit_collisions(&mut self.units, &excluded_squads);
        self.activate_physics_detonation_contacts(&contacts);
        let Some(gameplay) = gameplay else {
            self.finish_all_collision_attacks();
            return;
        };
        self.reconcile_collision_attacks(gameplay);
        self.resolve_collision_contacts(&contacts, gameplay);
    }

    pub(in crate::world) fn collision_attack_range(
        &self,
        squad: &Squad,
        gameplay: &GameplayCatalog,
    ) -> Option<f32> {
        (squad.mode == SquadMode::HitAndRun
            && squad.unit_ids.iter().any(|unit_id| {
                self.units.get(*unit_id).is_some_and(|unit| {
                    unit.is_operational()
                        && gameplay.collision_attack(&unit.proto_object_name).is_some()
                })
            }))
        .then_some(0.0)
    }

    fn reconcile_collision_attacks(&mut self, gameplay: &GameplayCatalog) {
        let states = self
            .units
            .iter()
            .map(|(unit_id, unit)| {
                let profile = gameplay.collision_attack(&unit.proto_object_name);
                let active = profile.is_some()
                    && unit.is_operational()
                    && unit
                        .squad_id
                        .and_then(|squad_id| self.squads.get(squad_id))
                        .is_some_and(|squad| squad.mode == SquadMode::HitAndRun);
                (
                    unit_id,
                    profile.map(|profile| (active, profile.new_tactic_state)),
                )
            })
            .collect::<Vec<_>>();
        for (unit_id, profile) in states {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            let Some((active, new_tactic_state)) = profile else {
                if unit.collision_attack.is_active() {
                    unit.collision_attack.finish();
                    unit.clear_tactic_state();
                }
                continue;
            };
            if active {
                if let Some(state) = new_tactic_state {
                    unit.set_tactic_state(state);
                }
                if unit.collision_attack.begin() {
                    unit.ammunition.set_current(unit.ammunition.maximum());
                }
            } else {
                unit.collision_attack.finish();
                unit.clear_tactic_state();
            }
        }
    }

    fn finish_all_collision_attacks(&mut self) {
        for (_, unit) in self.units.iter_mut() {
            if unit.collision_attack.is_active() {
                unit.collision_attack.finish();
                unit.clear_tactic_state();
            }
        }
    }

    fn resolve_collision_contacts(
        &mut self,
        contacts: &[UnitCollisionContact],
        gameplay: &GameplayCatalog,
    ) {
        let attacker_ids = self
            .units
            .iter()
            .filter_map(|(id, unit)| unit.collision_attack.is_active().then_some(id))
            .collect::<Vec<_>>();
        for attacker_id in attacker_ids {
            let mut target_ids = contacts
                .iter()
                .filter_map(|contact| contact_target(attacker_id, contact.first, contact.second))
                .filter(|target_id| {
                    self.units
                        .get(attacker_id)
                        .is_some_and(|attacker| !attacker.collision_attack.has_impacted(*target_id))
                })
                .collect::<Vec<_>>();
            target_ids.sort_by(|left, right| {
                let left_hp = self
                    .units
                    .get(*left)
                    .map_or(f32::INFINITY, |unit| unit.hitpoints);
                let right_hp = self
                    .units
                    .get(*right)
                    .map_or(f32::INFINITY, |unit| unit.hitpoints);
                left_hp.total_cmp(&right_hp).then_with(|| left.cmp(right))
            });
            for target_id in target_ids {
                if self.resolve_collision_attack(attacker_id, target_id, gameplay) {
                    break;
                }
            }
        }
    }

    fn resolve_collision_attack(
        &mut self,
        attacker_id: EntityId,
        target_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(attacker) = self.rammer_snapshot(attacker_id) else {
            return false;
        };
        let Some(target) = self.ram_target_snapshot(target_id) else {
            return false;
        };
        let Some(profile) = gameplay.collision_attack(&attacker.proto_object_name) else {
            return false;
        };
        let direction = planar_direction(attacker.position, target.position, attacker.forward);
        let Some(disposition) =
            self.collision_disposition(&attacker, &target, profile, direction, gameplay)
        else {
            return false;
        };
        if let Some(unit) = self.units.get_mut(attacker_id) {
            unit.collision_attack.mark_impacted(target_id);
        }
        let tuning = self.ram_weapon_tuning(&attacker, profile);
        let requested_damage = self.spend_ram_ammunition(&attacker, &target, tuning.damage_cap);
        let dealt = self.apply_attack_damage(
            &AttackDamage {
                attacker_id,
                attacker_player_id: attacker.player_id,
                primary_target_id: Some(target_id),
                ground_zero: target.position,
                direction,
                damage: requested_damage,
                weapon_type: profile.weapon_type.map(str::to_owned),
                area_damage: None,
            },
            Some(gameplay),
        );
        let killed = !self.units.get(target_id).is_some_and(Entity::is_alive);
        if killed {
            self.fire_ram_kill_event(&attacker, &target);
        } else {
            self.apply_rammed_target_motion(&target, direction, disposition);
        }
        self.apply_ram_reflection(&attacker, &target, dealt, tuning, direction, gameplay);
        if dealt > 0.0 {
            self.finish_collision_ability(&attacker, gameplay);
        }
        if !killed && disposition == CollisionDisposition::Rammable {
            self.finish_rammable_collision(&attacker, direction);
            return true;
        }
        false
    }

    fn rammer_snapshot(&self, unit_id: EntityId) -> Option<RammerSnapshot> {
        let unit = self
            .units
            .get(unit_id)
            .filter(|unit| unit.is_operational() && unit.collision_attack.is_active())?;
        let squad_id = unit.squad_id?;
        let squad = self.squads.get(squad_id)?;
        Some(RammerSnapshot {
            id: unit_id,
            squad_id,
            player_id: unit.base.player_id,
            position: unit.base.position,
            forward: unit.base.forward,
            proto_object_name: unit.proto_object_name.clone(),
            ordered_target_id: squad.attack_target?,
            ability_id: squad.attack_ability_id,
            damage_multiplier: unit.effective_damage_multiplier().max(0.0),
        })
    }

    fn ram_target_snapshot(&self, unit_id: EntityId) -> Option<RamTargetSnapshot> {
        let unit = self
            .units
            .get(unit_id)
            .filter(|unit| unit.is_attackable())?;
        let mode = unit
            .squad_id
            .and_then(|squad_id| self.squads.get(squad_id))
            .map_or(SquadMode::Normal, |squad| squad.mode);
        Some(RamTargetSnapshot {
            id: unit_id,
            squad_id: unit.squad_id,
            player_id: unit.base.player_id,
            position: unit.base.position,
            forward: unit.base.forward,
            velocity: unit.base.velocity,
            proto_object_name: unit.proto_object_name.clone(),
            mode,
            hitpoints: unit.hitpoints,
            obstruction_radius: unit.obstruction_radius(),
            in_cover: mode == SquadMode::Cover,
        })
    }

    fn collision_disposition(
        &self,
        attacker: &RammerSnapshot,
        target: &RamTargetSnapshot,
        profile: CollisionAttackProfile<'_>,
        direction: Vec3,
        gameplay: &GameplayCatalog,
    ) -> Option<CollisionDisposition> {
        if target.in_cover
            || (target.player_id != GAIA_PLAYER
                && !self.players_are_enemies(attacker.player_id, target.player_id))
        {
            return None;
        }
        let traits = gameplay.collision_target_profile(
            profile.weapon_type,
            &target.proto_object_name,
            direction,
            target.forward,
            target.mode,
        );
        let ordered_target = target.id == attacker.ordered_target_id
            || target.squad_id == Some(attacker.ordered_target_id);
        if !ordered_target
            && (!traits.bowlable
                || !self.target_is_in_collision_area(attacker, target, profile.area_radius))
        {
            return None;
        }
        if traits.bowlable {
            Some(CollisionDisposition::Bowlable)
        } else if ordered_target && traits.rammable {
            Some(CollisionDisposition::Rammable)
        } else {
            None
        }
    }

    fn target_is_in_collision_area(
        &self,
        attacker: &RammerSnapshot,
        target: &RamTargetSnapshot,
        radius: f32,
    ) -> bool {
        let Some(center) = self.entity_position(attacker.ordered_target_id) else {
            return false;
        };
        center.distance_squared(target.position) <= radius * radius
    }

    fn ram_weapon_tuning(
        &self,
        attacker: &RammerSnapshot,
        profile: CollisionAttackProfile<'_>,
    ) -> RamWeaponTuning {
        let base_cap = profile.max_damage_per_ram;
        let (damage_cap, reflect_damage_factor) = self.get_player(attacker.player_id).map_or(
            (base_cap, profile.reflect_damage_factor),
            |player| {
                (
                    player.technologies.weapon_max_damage_per_ram(
                        &attacker.proto_object_name,
                        profile.weapon_name,
                        base_cap,
                    ),
                    player.technologies.weapon_reflect_damage_factor(
                        &attacker.proto_object_name,
                        profile.weapon_name,
                        profile.reflect_damage_factor,
                    ),
                )
            },
        );
        RamWeaponTuning {
            damage_cap: finite_nonnegative(damage_cap),
            reflect_damage_factor: finite_nonnegative(reflect_damage_factor),
        }
    }

    fn spend_ram_ammunition(
        &mut self,
        attacker: &RammerSnapshot,
        target: &RamTargetSnapshot,
        damage_cap: f32,
    ) -> f32 {
        let Some(unit) = self.units.get_mut(attacker.id) else {
            return 0.0;
        };
        let mut requested = unit.ammunition.current() * attacker.damage_multiplier;
        requested = requested.min(damage_cap);
        requested = finite_nonnegative(requested);
        unit.ammunition
            .adjust(-requested.min(target.hitpoints.max(0.0)));
        requested
    }

    fn fire_ram_kill_event(&mut self, attacker: &RammerSnapshot, target: &RamTargetSnapshot) {
        let event = GeneralEvent::new(
            GeneralEventType::GameEntityRammed,
            i32::from(target.player_id),
        )
        .with_entities(Some(target.id), Some(attacker.id));
        self.fire_general_event(&event);
    }

    fn apply_rammed_target_motion(
        &mut self,
        target: &RamTargetSnapshot,
        direction: Vec3,
        disposition: CollisionDisposition,
    ) {
        let mass = self
            .units
            .get(target.id)
            .and_then(|unit| unit.physics.as_ref())
            .map(|body| body.material().mass);
        if let Some(mass) = mass {
            let impulse = rammed_impulse(direction, mass, target.velocity.y);
            if let Some(unit) = self.units.get_mut(target.id) {
                let _applied = unit.apply_impulse(impulse);
            }
            return;
        }
        if disposition == CollisionDisposition::Bowlable {
            self.displace_bowlable_target(target, direction);
        }
    }

    fn displace_bowlable_target(&mut self, target: &RamTargetSnapshot, direction: Vec3) {
        let displacement = direction * target.obstruction_radius.max(0.5) * 2.0;
        if let Some(squad_id) = target.squad_id
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.base.position += displacement;
            squad.base.velocity = direction * RAMMED_IMPULSE_FORCE;
        } else if let Some(unit) = self.units.get_mut(target.id) {
            unit.base.position += displacement;
            unit.base.velocity = direction * RAMMED_IMPULSE_FORCE;
        }
    }

    fn apply_ram_reflection(
        &mut self,
        attacker: &RammerSnapshot,
        target: &RamTargetSnapshot,
        dealt: f32,
        tuning: RamWeaponTuning,
        direction: Vec3,
        gameplay: &GameplayCatalog,
    ) {
        let profile = gameplay.collision_attack(&attacker.proto_object_name);
        let armor_reflection = profile.map_or(0.0, |profile| {
            gameplay
                .collision_target_profile(
                    profile.weapon_type,
                    &target.proto_object_name,
                    direction,
                    target.forward,
                    target.mode,
                )
                .reflect_damage_factor
        });
        let reflected = dealt * tuning.reflect_damage_factor * armor_reflection;
        if reflected.is_finite() && reflected > 0.0 {
            let _dealt = self.apply_reflected_collision_damage(
                target.id,
                target.player_id,
                attacker.id,
                reflected,
                gameplay,
            );
        }
    }

    fn finish_collision_ability(&mut self, attacker: &RammerSnapshot, gameplay: &GameplayCatalog) {
        let Some(requested) = attacker.ability_id else {
            return;
        };
        let Some(ability) = gameplay.resolve_order_ability(&attacker.proto_object_name, requested)
        else {
            return;
        };
        let recovery_time =
            self.get_player(attacker.player_id)
                .map_or(ability.recovery_time(), |player| {
                    player
                        .technologies
                        .ability_recovery_time(ability.name(), ability.recovery_time())
                });
        let recovery_type = (ability.recovery_start() == Some(AbilityRecoveryStart::Attack))
            .then(|| ability.recovery_type())
            .flatten();
        if let Some(squad) = self.squads.get_mut(attacker.squad_id) {
            squad.finish_ability_execution(
                recovery_type,
                recovery_time,
                Some(ability.database_id()),
            );
        }
    }

    fn finish_rammable_collision(&mut self, attacker: &RammerSnapshot, direction: Vec3) {
        if let Some(squad) = self.squads.get_mut(attacker.squad_id) {
            squad.clear_attack_order();
            squad.mode = SquadMode::Normal;
        }
        let mass = self
            .units
            .get(attacker.id)
            .and_then(|unit| unit.physics.as_ref())
            .map_or(1.0, |body| body.material().mass);
        if let Some(unit) = self.units.get_mut(attacker.id) {
            unit.stop();
            let impulse =
                angled_impulse(-direction, RAMMER_IMPULSE_ANGLE, RAMMER_IMPULSE_FORCE, mass);
            let _applied = unit.apply_impulse(impulse);
            unit.collision_attack.finish();
        }
    }
}

fn contact_target(attacker: EntityId, first: EntityId, second: EntityId) -> Option<EntityId> {
    if first == attacker {
        Some(second)
    } else if second == attacker {
        Some(first)
    } else {
        None
    }
}

fn planar_direction(from: Vec3, to: Vec3, fallback: Vec3) -> Vec3 {
    let direction = Vec3::new(to.x - from.x, 0.0, to.z - from.z).normalize_or_zero();
    if direction == Vec3::ZERO {
        Vec3::new(fallback.x, 0.0, fallback.z).normalize_or_zero()
    } else {
        direction
    }
}

fn rammed_impulse(direction: Vec3, mass: f32, upward_velocity: f32) -> Vec3 {
    let mut impulse = angled_impulse(direction, RAMMED_IMPULSE_ANGLE, RAMMED_IMPULSE_FORCE, mass);
    if upward_velocity > RAMMED_TOP_UP_VELOCITY {
        impulse.y = 0.0;
    } else if upward_velocity > RAMMED_MEDIUM_UP_VELOCITY {
        let span = RAMMED_TOP_UP_VELOCITY - RAMMED_MEDIUM_UP_VELOCITY;
        impulse.y *= 1.0 - (upward_velocity - RAMMED_MEDIUM_UP_VELOCITY) / span;
    }
    impulse
}

fn angled_impulse(direction: Vec3, angle: f32, force: f32, mass: f32) -> Vec3 {
    let mut impulse = direction * angle.cos();
    impulse.y = angle.sin();
    impulse * (force * mass.max(0.0))
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
