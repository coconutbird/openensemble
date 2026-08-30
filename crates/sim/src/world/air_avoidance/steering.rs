//! Deterministic claimed-air-spot avoidance and attack-depression steering.

use super::{World, is_active_flight};
use crate::entities::squads::formation_offset_to_world;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::TeamId;
use glam::Vec3;

const POSITION_EPSILON: f32 = 0.000_001;

#[derive(Debug, Clone)]
struct AirSpot {
    unit_id: EntityId,
    proto_object_id: i32,
    team_id: TeamId,
    position: Vec3,
    velocity: Vec3,
    radius: f32,
    flying: bool,
}

#[derive(Debug, Clone)]
struct SteeringContext {
    unit_id: EntityId,
    squad_id: EntityId,
    proto_object_id: i32,
    team_id: TeamId,
    position: Vec3,
    radius: f32,
    speed: f32,
    can_reverse: bool,
    stationary: bool,
    has_move_action: bool,
    attack_target: Option<Vec3>,
    max_depression_angle: f32,
    leash_position: Vec3,
    anchor_position: Vec3,
    leash_distance: f32,
    leash_deadzone: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct SteeringFlags(u8);

impl SteeringFlags {
    const AVOIDING: u8 = 1 << 0;
    const AVOIDING_FRIENDLY: u8 = 1 << 1;
    const TOO_CLOSE: u8 = 1 << 2;
    const BLOCK_ANCHOR_RETURN: u8 = 1 << 3;

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn insert(&mut self, flag: u8) {
        self.0 |= flag;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SteeringResult {
    avoidance: Vec3,
    nearest_altitude: f32,
    flags: SteeringFlags,
}

impl SteeringResult {
    const fn avoiding(self) -> bool {
        self.flags.contains(SteeringFlags::AVOIDING)
    }

    const fn avoiding_friendly(self) -> bool {
        self.flags.contains(SteeringFlags::AVOIDING_FRIENDLY)
    }

    const fn too_close(self) -> bool {
        self.flags.contains(SteeringFlags::TOO_CLOSE)
    }

    const fn blocks_anchor_return(self) -> bool {
        self.flags.contains(SteeringFlags::BLOCK_ANCHOR_RETURN)
    }
}

impl World {
    pub(super) fn update_aircraft_steering(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in &unit_ids {
            let squad_id = self
                .enabled_air_avoidance_profile(*unit_id, gameplay)
                .and_then(|_| self.units.get(*unit_id))
                .and_then(|unit| unit.squad_id);
            if let Some(squad_id) = squad_id
                && let Some(squad) = self.squads.get_mut(squad_id)
            {
                squad.initialize_air_anchor();
            }
        }
        let spots = self.claimed_air_spots(gameplay);
        for unit_id in unit_ids {
            let Some(context) = self.aircraft_steering_context(unit_id, gameplay) else {
                continue;
            };
            let result = self.calculate_aircraft_steering(&context, &spots);
            self.commit_aircraft_steering(&context, result, &spots, dt);
        }
    }

    fn claimed_air_spots(&self, gameplay: &GameplayCatalog) -> Vec<AirSpot> {
        self.units
            .iter()
            .filter_map(|(unit_id, unit)| {
                let profile = self.enabled_air_avoidance_profile(unit_id, gameplay)?;
                if profile.avoid_only() || !unit.is_alive() || unit.is_garrisoned() {
                    return None;
                }
                Some(AirSpot {
                    unit_id,
                    proto_object_id: unit.proto_object_id,
                    team_id: self.get_player(unit.base.player_id)?.team_id,
                    position: self.predicted_aircraft_position(unit_id),
                    velocity: unit.base.velocity,
                    radius: unit.obstruction_radius(),
                    flying: unit.flying,
                })
            })
            .collect()
    }

    fn aircraft_steering_context(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<SteeringContext> {
        let unit = self.units.get(unit_id).filter(|unit| {
            is_active_flight(unit) && (!unit.uses_move_air() || unit.is_move_air_working())
        })?;
        let profile = self.enabled_air_avoidance_profile(unit_id, gameplay)?;
        let squad_id = unit.squad_id?;
        let squad = self.squads.get(squad_id)?;
        let attack_target_id = unit.attack_target.or(squad.attack_target);
        Some(SteeringContext {
            unit_id,
            squad_id,
            proto_object_id: unit.proto_object_id,
            team_id: self.get_player(unit.base.player_id)?.team_id,
            position: self.predicted_aircraft_position(unit_id),
            radius: unit.obstruction_radius(),
            speed: unit.speed * unit.effective_velocity_scalar(),
            can_reverse: unit.can_reverse_for_air_avoidance(),
            stationary: profile.stationary(),
            has_move_action: squad.move_target.is_some() || squad.is_carpet_bombing(),
            attack_target: squad
                .carpet_bomb_attack_position()
                .or_else(|| attack_target_id.and_then(|target| self.entity_position(target))),
            max_depression_angle: profile.max_target_depression_angle(),
            leash_position: squad.leash_position(),
            anchor_position: squad.anchor_position(),
            leash_distance: if squad.ignores_leash() {
                f32::INFINITY
            } else {
                squad.leash_distance
            },
            leash_deadzone: squad.leash_deadzone(),
        })
    }

    fn predicted_aircraft_position(&self, unit_id: EntityId) -> Vec3 {
        let Some(unit) = self.units.get(unit_id) else {
            return Vec3::ZERO;
        };
        unit.squad_id
            .and_then(|squad_id| self.squads.get(squad_id))
            .map_or(unit.base.position, |squad| {
                squad.base.position
                    + formation_offset_to_world(squad.base.forward, unit.formation_offset)
            })
    }

    fn calculate_aircraft_steering(
        &mut self,
        context: &SteeringContext,
        spots: &[AirSpot],
    ) -> SteeringResult {
        let mut result = SteeringResult {
            nearest_altitude: 0.0,
            ..SteeringResult::default()
        };
        let mut closest_range = f32::MAX;
        for spot in spots {
            if Self::skip_air_spot(context, spot) {
                continue;
            }
            let (repel, range) = self.air_spot_repel(context, spot);
            let radius_sum = context.radius + spot.radius;
            if range > radius_sum {
                continue;
            }
            let friendly = context.team_id == spot.team_id;
            result.flags.insert(SteeringFlags::AVOIDING);
            if friendly {
                result.flags.insert(SteeringFlags::AVOIDING_FRIENDLY);
            }
            result.avoidance += moving_air_spot_detour(spot, repel)
                .unwrap_or_else(|| 2.0 * repel * (radius_sum - range));
            if range < closest_range {
                result.nearest_altitude = spot.position.y;
                closest_range = range;
            }
        }
        Self::apply_target_depression(context, &mut result);
        result
    }

    fn skip_air_spot(context: &SteeringContext, spot: &AirSpot) -> bool {
        spot.unit_id == context.unit_id
            || (context.stationary
                && spot.proto_object_id != context.proto_object_id
                && spot.flying)
    }

    fn air_spot_repel(&mut self, context: &SteeringContext, spot: &AirSpot) -> (Vec3, f32) {
        let own = Vec3::new(context.position.x, 0.0, context.position.z);
        let nearby = Vec3::new(spot.position.x, 0.0, spot.position.z);
        let mut repel = own - nearby;
        let range = repel.length();
        if repel.length_squared() <= POSITION_EPSILON {
            repel.x = self.trigger_random_float(-1.0, 1.0);
            repel.z = self.trigger_random_float(-1.0, 1.0);
        }
        (repel.normalize_or_zero(), range)
    }

    fn apply_target_depression(context: &SteeringContext, result: &mut SteeringResult) {
        let Some(target) = context.attack_target else {
            return;
        };
        let planar = Vec3::new(
            target.x - context.position.x,
            0.0,
            target.z - context.position.z,
        );
        let xz_distance = planar.length().max(f32::EPSILON);
        let y_distance = (context.position.y - target.y).abs();
        let angle = (y_distance / xz_distance).atan().to_degrees();
        if context.max_depression_angle < 89.0 && angle > context.max_depression_angle {
            let tangent = context.max_depression_angle.to_radians().tan();
            let desired = if tangent.abs() > f32::EPSILON {
                1.2 * y_distance / tangent
            } else {
                0.0
            };
            let toward = planar.normalize_or_zero();
            result.avoidance = if context.can_reverse {
                -desired * toward
            } else {
                desired * toward.cross(Vec3::Y)
            };
            result.flags.insert(SteeringFlags::TOO_CLOSE);
        } else if result.avoiding() {
            remove_component_away_from_target(&mut result.avoidance, planar);
        }
        if context.leash_position.distance(context.anchor_position) > 3.0 * context.radius {
            let anchor_planar = Vec3::new(
                target.x - context.anchor_position.x,
                0.0,
                target.z - context.anchor_position.z,
            );
            let anchor_xz_distance = anchor_planar.length().max(f32::EPSILON);
            let anchor_angle = (y_distance / anchor_xz_distance).atan().to_degrees();
            if context.max_depression_angle < 89.0 && anchor_angle > context.max_depression_angle {
                result.flags.insert(SteeringFlags::BLOCK_ANCHOR_RETURN);
            }
        }
    }

    fn commit_aircraft_steering(
        &mut self,
        context: &SteeringContext,
        result: SteeringResult,
        spots: &[AirSpot],
        dt: f32,
    ) {
        let Some(unit) = self.units.get_mut(context.unit_id) else {
            return;
        };
        unit.air_avoidance.set_avoidance(
            result.avoidance,
            result.nearest_altitude,
            result.avoiding(),
            result.avoiding_friendly(),
        );
        unit.air_avoidance
            .update_vertical_avoidance(context.position.y);
        self.move_aircraft_squad(context, result, spots, dt);
    }

    fn move_aircraft_squad(
        &mut self,
        context: &SteeringContext,
        result: SteeringResult,
        spots: &[AirSpot],
        dt: f32,
    ) {
        let avoidance_branch =
            !context.has_move_action && (result.avoiding() || result.too_close());
        let should_steer =
            !context.has_move_action && (result.too_close() || result.avoiding_friendly());
        let desired_location = context.position + result.avoidance;
        let destination_open = self.air_destination_open(context, desired_location);
        let return_to_anchor = !avoidance_branch
            && !result.blocks_anchor_return()
            && context.leash_position.distance(context.anchor_position)
                > context.leash_distance + context.leash_deadzone
            && Self::air_anchor_open(context, spots);
        let bounds = self.effective_playable_bounds();
        let Some(squad) = self.squads.get_mut(context.squad_id) else {
            return;
        };
        let mut dragged_leash = false;
        if should_steer && result.avoidance != Vec3::ZERO && destination_open {
            let step = result.avoidance.normalize_or_zero()
                * (context.speed * dt).min(result.avoidance.length());
            squad.base.position += step;
            squad.base.velocity += step / dt;
            dragged_leash = true;
        }
        if let Some(bounds) = bounds {
            clamp_squad_to_bounds(squad, bounds, context.radius);
        }
        if dragged_leash {
            squad.set_leash_position(squad.base.position, false);
        } else if return_to_anchor {
            squad.base.position = context.anchor_position;
            squad.base.velocity = Vec3::ZERO;
            squad.set_leash_position(context.anchor_position, false);
        }
    }

    fn air_destination_open(&self, context: &SteeringContext, destination: Vec3) -> bool {
        if self.effective_playable_bounds().is_some_and(|bounds| {
            destination.x < bounds.min_x() + context.radius
                || destination.x > bounds.max_x() - context.radius
                || destination.z < bounds.min_z() + context.radius
                || destination.z > bounds.max_z() - context.radius
        }) {
            return false;
        }
        self.units.iter().all(|(candidate_id, candidate)| {
            candidate_id == context.unit_id
                || !candidate.is_alive()
                || candidate.is_garrisoned()
                || !candidate.obstructs_air()
                || planar_distance(destination, candidate.base.position)
                    >= context.radius + candidate.obstruction_radius()
        })
    }

    fn air_anchor_open(context: &SteeringContext, spots: &[AirSpot]) -> bool {
        spots.iter().all(|spot| {
            spot.unit_id == context.unit_id
                || spot.team_id != context.team_id
                || spot.position.distance(context.anchor_position) >= context.radius + spot.radius
        })
    }
}

fn moving_air_spot_detour(spot: &AirSpot, repel: Vec3) -> Option<Vec3> {
    if spot.velocity.length() <= 1.0 {
        return None;
    }
    let velocity = Vec3::new(spot.velocity.x, 0.0, spot.velocity.z).normalize_or_zero();
    if -velocity.dot(repel) <= 0.0 {
        return None;
    }
    let mut normal = velocity.cross(Vec3::Y).normalize_or_zero();
    if normal.dot(repel) < 0.0 {
        normal = -normal;
    }
    Some(normal)
}

fn remove_component_away_from_target(avoidance: &mut Vec3, planar_to_target: Vec3) {
    let toward = planar_to_target.normalize_or_zero();
    let normalized_avoidance = avoidance.normalize_or_zero();
    let dot = toward.dot(normalized_avoidance);
    if dot < 0.0 {
        *avoidance -= toward * dot * avoidance.length();
    }
}

fn clamp_squad_to_bounds(
    squad: &mut crate::entities::Squad,
    bounds: super::super::WorldBounds,
    radius: f32,
) {
    let min_x = bounds.min_x() + radius;
    let max_x = bounds.max_x() - radius;
    let min_z = bounds.min_z() + radius;
    let max_z = bounds.max_z() - radius;
    squad.base.position.x = clamp_axis(squad.base.position.x, min_x, max_x);
    squad.base.position.z = clamp_axis(squad.base.position.z, min_z, max_z);
}

fn clamp_axis(value: f32, minimum: f32, maximum: f32) -> f32 {
    if minimum <= maximum {
        value.clamp(minimum, maximum)
    } else {
        f32::midpoint(minimum, maximum)
    }
}

fn planar_distance(first: Vec3, second: Vec3) -> f32 {
    Vec3::new(first.x - second.x, 0.0, first.z - second.z).length()
}
