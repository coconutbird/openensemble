//! Lethal aircraft crash targeting, movement, collision, and detonation.

use super::World;
use crate::entities::AircraftCrashPhase;
use crate::entities::units::UnitDeathState;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AirAvoidanceActionProfile, GameplayCatalog};
use crate::world::combat::AttackDamage;
use crate::world::events::ImpactEffectRequestData;
use crate::{ImpactEffectRequest, ImpactSurface};
use glam::Vec3;

const CRASH_DELAY_MIN_MS: u32 = 2_500;
const CRASH_DELAY_SPREAD_MS: u32 = 2_500;
const IMMEDIATE_CRASH_DELAY_CUTOFF_MS: u32 = 3_000;
const RANDOM_CRASH_FORWARD: f32 = 40.0;
const RANDOM_CRASH_RIGHT: f32 = 40.0;
const RANDOM_CRASH_DEPTH: f32 = 20.0;

#[derive(Debug, Clone, Copy)]
struct CrashSource {
    position: Vec3,
    forward: Vec3,
    player_id: u8,
}

#[derive(Debug, Clone, Copy)]
struct CrashMovement {
    unit_id: EntityId,
    squad_id: EntityId,
    start: Vec3,
    destination: Vec3,
    speed: f32,
    player_id: u8,
    half_extents: Vec3,
}

impl World {
    pub(super) fn prepare_aircraft_crashes(&mut self, gameplay: &GameplayCatalog) {
        let pending = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (unit.aircraft_crash_phase() == AircraftCrashPhase::PendingTarget)
                    .then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in pending {
            self.prepare_aircraft_crash(unit_id, gameplay);
        }
    }

    fn prepare_aircraft_crash(&mut self, unit_id: EntityId, gameplay: &GameplayCatalog) {
        let Some((source, profile)) = self.units.get(unit_id).and_then(|unit| {
            let profile = Self::current_air_avoidance_profile(unit, gameplay)?.clone();
            Some((
                CrashSource {
                    position: unit.base.position,
                    forward: unit.base.forward,
                    player_id: unit.base.player_id,
                },
                profile,
            ))
        }) else {
            self.kill_unresolved_crashing_aircraft(unit_id);
            return;
        };
        let target = profile
            .kamikaze_weapon()
            .and_then(|weapon| self.find_kamikaze_target(unit_id, source, weapon.max_range()));
        let crash_position = target
            .and_then(|target_id| self.units.get(target_id).map(|unit| unit.base.position))
            .unwrap_or_else(|| self.random_crash_position(source));
        if let Some(unit) = self.units.get_mut(unit_id) {
            let _started = unit.air_avoidance.start_crashing(target, crash_position);
        }
    }

    fn find_kamikaze_target(
        &self,
        source_id: EntityId,
        source: CrashSource,
        max_range: f32,
    ) -> Option<EntityId> {
        let viewer_team = self.get_player(source.player_id)?.team_id;
        let mut best = None;
        let mut best_dot = 0.0;
        for (candidate_id, candidate) in self.units.iter() {
            if candidate_id == source_id
                || !candidate.is_alive()
                || candidate.flying
                || !self.players_are_enemies(source.player_id, candidate.base.player_id)
                || !self.is_entity_visible_to_team(viewer_team, candidate_id)
                || candidate.object_state.attached_to() == Some(source_id)
            {
                continue;
            }
            let to_target = candidate.base.position - source.position;
            if to_target.length() > max_range {
                continue;
            }
            let dot = source.forward.dot(to_target.normalize_or_zero());
            if dot > best_dot {
                best_dot = dot;
                best = Some(candidate_id);
            }
        }
        best
    }

    fn random_crash_position(&mut self, source: CrashSource) -> Vec3 {
        let forward_distance = self.trigger_random_float(0.0, RANDOM_CRASH_FORWARD);
        let right_distance = self.trigger_random_float(-RANDOM_CRASH_RIGHT, RANDOM_CRASH_RIGHT);
        let forward = source.forward.normalize_or_zero();
        let right = Vec3::Y.cross(forward).normalize_or_zero();
        let mut crash = source.position + forward * forward_distance + right * right_distance;
        crash.y = self.terrain_height(crash, true).unwrap_or(crash.y) - RANDOM_CRASH_DEPTH;
        crash
    }

    fn kill_unresolved_crashing_aircraft(&mut self, unit_id: EntityId) {
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.kill();
        }
    }

    pub(super) fn update_aircraft_crashes(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let crashing = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (unit.aircraft_crash_phase() == AircraftCrashPhase::Crashing).then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in crashing {
            self.update_aircraft_crash(unit_id, dt, gameplay);
        }
    }

    fn update_aircraft_crash(&mut self, unit_id: EntityId, dt: f32, gameplay: &GameplayCatalog) {
        self.begin_aircraft_crash_update(unit_id);
        self.clear_dead_crash_target(unit_id);
        if self.crash_timer_due(unit_id) {
            self.detonate_crashing_aircraft(unit_id, None, gameplay);
            return;
        }
        let Some(movement) = self.crash_movement(unit_id) else {
            return;
        };
        let next = move_toward(movement.start, movement.destination, movement.speed * dt);
        let hit = self.crash_collision(&movement, next);
        let ground_contact = self.crash_hits_ground(&movement, next);
        self.commit_crash_movement(movement, next, dt);
        if hit.is_some() || ground_contact {
            self.detonate_crashing_aircraft(unit_id, hit, gameplay);
        }
    }

    fn begin_aircraft_crash_update(&mut self, unit_id: EntityId) {
        let first_update = self
            .units
            .get(unit_id)
            .is_some_and(|unit| unit.air_avoidance.detonate_at_ms.is_none());
        if !first_update {
            return;
        }
        let rolled = CRASH_DELAY_MIN_MS + self.trigger_random_index(CRASH_DELAY_SPREAD_MS);
        let delay = if rolled < IMMEDIATE_CRASH_DELAY_CUTOFF_MS {
            0
        } else {
            rolled
        };
        let detonate_at = self.game_time_ms.wrapping_add(delay);
        let squad_id = self.units.get(unit_id).and_then(|unit| unit.squad_id);
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.air_avoidance.begin_crash_update(detonate_at);
            unit.clear_attack_order();
        }
        if let Some(squad_id) = squad_id
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.remove_all_orders();
        }
    }

    fn clear_dead_crash_target(&mut self, unit_id: EntityId) {
        let target = self
            .units
            .get(unit_id)
            .and_then(|unit| unit.air_avoidance.kamikaze_target);
        let target_alive =
            target.is_some_and(|id| self.units.get(id).is_some_and(Entity::is_alive));
        if target.is_some()
            && !target_alive
            && let Some(unit) = self.units.get_mut(unit_id)
        {
            unit.air_avoidance.clear_dead_kamikaze_target();
        }
    }

    fn crash_timer_due(&self, unit_id: EntityId) -> bool {
        self.units.get(unit_id).is_some_and(|unit| {
            unit.air_avoidance.kamikaze_target.is_none()
                && unit
                    .air_avoidance
                    .detonate_at_ms
                    .is_some_and(|deadline| time_reached(self.game_time_ms, deadline))
        })
    }

    fn crash_movement(&self, unit_id: EntityId) -> Option<CrashMovement> {
        let unit = self.units.get(unit_id)?;
        let squad_id = unit.squad_id?;
        let squad = self.squads.get(squad_id)?;
        let target_position = unit
            .air_avoidance
            .kamikaze_target
            .and_then(|target| self.units.get(target).map(|unit| unit.base.position));
        let targeted = target_position.is_some();
        Some(CrashMovement {
            unit_id,
            squad_id,
            start: squad.base.position,
            destination: target_position.unwrap_or(unit.air_avoidance.crash_position),
            speed: unit.speed * unit.effective_velocity_scalar() * if targeted { 3.0 } else { 1.0 },
            player_id: unit.base.player_id,
            half_extents: unit.obstruction_half_extents.abs(),
        })
    }

    fn crash_collision(&self, movement: &CrashMovement, next: Vec3) -> Option<EntityId> {
        let mut first_hit = None;
        let mut first_fraction = f32::MAX;
        for (candidate_id, candidate) in self.units.iter() {
            if candidate_id == movement.unit_id
                || !candidate.is_alive()
                || !self.players_are_enemies(movement.player_id, candidate.base.player_id)
            {
                continue;
            }
            let (center, half_extents) = candidate.simulation_bounds();
            let Some(fraction) = segment_aabb_fraction(movement.start, next, center, half_extents)
            else {
                continue;
            };
            if fraction < first_fraction {
                first_fraction = fraction;
                first_hit = Some(candidate_id);
            }
        }
        first_hit
    }

    fn crash_hits_ground(&self, movement: &CrashMovement, next: Vec3) -> bool {
        self.terrain_height(next, true)
            .is_some_and(|height| next.y - 1.2 * movement.half_extents.y <= height)
    }

    fn commit_crash_movement(&mut self, movement: CrashMovement, next: Vec3, dt: f32) {
        let velocity = (next - movement.start) / dt;
        if let Some(squad) = self.squads.get_mut(movement.squad_id) {
            squad.base.position = next;
            squad.base.velocity = velocity;
            if velocity != Vec3::ZERO {
                squad.base.set_forward(velocity);
            }
        }
        if let Some(unit) = self.units.get_mut(movement.unit_id) {
            unit.base.position = next;
            unit.base.velocity = velocity;
            if velocity != Vec3::ZERO {
                unit.base.set_forward(velocity);
            }
        }
    }

    pub(super) fn detonate_crashing_aircraft(
        &mut self,
        unit_id: EntityId,
        hit: Option<EntityId>,
        gameplay: &GameplayCatalog,
    ) {
        let Some((player_id, position, direction, damage_multiplier, profile)) =
            self.units.get(unit_id).and_then(|unit| {
                Some((
                    unit.base.player_id,
                    unit.base.position,
                    unit.base.velocity.normalize_or_zero(),
                    unit.effective_damage_multiplier().max(0.0),
                    Self::current_air_avoidance_profile(unit, gameplay)?.clone(),
                ))
            })
        else {
            return;
        };
        if let Some(weapon) = profile.kamikaze_weapon() {
            let death = self.units.get(unit_id).map(|unit| {
                UnitDeathState::new(
                    unit.air_avoidance.killer_unit,
                    unit.air_avoidance.killer_player,
                    unit.air_avoidance.killer_team,
                    weapon.weapon_type(),
                )
            });
            let attack = AttackDamage {
                attacker_id: unit_id,
                attacker_player_id: player_id,
                primary_target_id: hit,
                ground_zero: position,
                direction,
                damage: weapon.damage() * damage_multiplier,
                weapon_type: weapon.weapon_type().map(str::to_owned),
                area_damage: weapon.area_damage(),
            };
            let _damage = self.apply_attack_damage(&attack, Some(gameplay));
            self.queue_crash_impact(unit_id, hit, player_id, position, &profile);
            if let Some(death) = death {
                self.apply_forced_attributed_death_damage(unit_id, 10.0, death, gameplay);
                return;
            }
        }
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.kill();
        }
    }

    fn queue_crash_impact(
        &mut self,
        unit_id: EntityId,
        hit: Option<EntityId>,
        player_id: u8,
        position: Vec3,
        profile: &AirAvoidanceActionProfile,
    ) {
        let Some(effect) = profile
            .kamikaze_weapon()
            .and_then(|weapon| weapon.impact_effect())
            .cloned()
        else {
            return;
        };
        let surface = self
            .terrain_surface_type(position)
            .map(ImpactSurface::Terrain);
        self.queue_impact_effect(ImpactEffectRequest::new(ImpactEffectRequestData {
            occurred_at_ms: self.game_time_ms,
            projectile_id: unit_id,
            primary_target_id: hit,
            player_id,
            effect,
            position,
            forward: Vec3::Z,
            emit_surface_effect: surface.is_some(),
            surface,
        }));
    }
}

fn move_toward(start: Vec3, destination: Vec3, movement: f32) -> Vec3 {
    let delta = destination - start;
    let distance = delta.length();
    if distance <= movement || distance <= f32::EPSILON {
        destination
    } else {
        start + delta * (movement.max(0.0) / distance)
    }
}

fn segment_aabb_fraction(start: Vec3, end: Vec3, center: Vec3, half: Vec3) -> Option<f32> {
    let minimum = center - half;
    let maximum = center + half;
    let delta = end - start;
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for (origin, direction, min, max) in [
        (start.x, delta.x, minimum.x, maximum.x),
        (start.y, delta.y, minimum.y, maximum.y),
        (start.z, delta.z, minimum.z, maximum.z),
    ] {
        if direction.abs() <= f32::EPSILON {
            if origin < min || origin > max {
                return None;
            }
            continue;
        }
        let inverse = direction.recip();
        let first = (min - origin) * inverse;
        let second = (max - origin) * inverse;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return None;
        }
    }
    Some(near)
}

const fn time_reached(now: u32, deadline: u32) -> bool {
    now.wrapping_sub(deadline) < (1_u32 << 31)
}
