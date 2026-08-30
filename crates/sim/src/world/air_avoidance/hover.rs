//! Terrain-relative hover-flight altitude control owned by the simulation.

use super::{World, is_active_flight};
use crate::entities::squads::formation_offset_to_world;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use glam::Vec3;

const SAMPLE_RADIUS_X: f32 = 4.0;
const SAMPLE_RADIUS_Z: f32 = 4.0;
const TERRAIN_CLEARANCE: f32 = 16.0;
const TERRAIN_PROBE_OFFSET: f32 = 8.0;
const MINIMUM_AGL: f32 = 5.0;
const DESCENT_RATE_MULTIPLIER: f32 = 0.5;
const LOOK_AHEAD_SECONDS: f32 = 0.2;
const LOOK_AHEAD_MULTIPLIERS: [f32; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];

#[derive(Debug, Clone, Copy)]
struct HoverContext {
    unit_id: EntityId,
    squad_id: EntityId,
    position: Vec3,
    forward: Vec3,
    velocity: Vec3,
    formation_height: f32,
    hover_altitude_offset: f32,
    vertical_avoidance_offset: f32,
    look_ahead_index: usize,
    has_move_action: bool,
}

impl World {
    pub(super) fn update_aircraft_hover(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            let Some(context) = self.aircraft_hover_context(unit_id, gameplay) else {
                continue;
            };
            self.update_aircraft_hover_unit(context, dt);
        }
    }

    fn aircraft_hover_context(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<HoverContext> {
        let unit = self
            .units
            .get(unit_id)
            .filter(|unit| is_active_flight(unit) && unit.uses_physics_hover())?;
        let profile = self.enabled_air_avoidance_profile(unit_id, gameplay)?;
        let squad_id = unit.squad_id?;
        let squad = self.squads.get(squad_id)?;
        let formation_offset = formation_offset_to_world(squad.base.forward, unit.formation_offset);
        let mut position = squad.base.position + formation_offset;
        // Squad pathing is planar in retail. Until the shared squad mover is
        // fully split into planar and vertical controllers, retain the unit's
        // pre-movement altitude so an order's ground-space Y cannot fight the
        // hover controller during this substep.
        position.y = unit.base.position.y;
        Some(HoverContext {
            unit_id,
            squad_id,
            position,
            forward: squad.base.forward,
            velocity: squad.base.velocity,
            formation_height: formation_offset.y,
            hover_altitude_offset: profile.hover_altitude_offset(),
            vertical_avoidance_offset: unit.air_avoidance.vertical_avoidance_offset,
            look_ahead_index: unit.air_avoidance.hover_look_ahead_index(),
            has_move_action: squad.move_target.is_some(),
        })
    }

    fn update_aircraft_hover_unit(&mut self, context: HoverContext, dt: f32) {
        let current_height = self.current_hover_height(context);
        let future_height = self.future_hover_height(context);
        let Some(unit) = self.units.get_mut(context.unit_id) else {
            return;
        };
        let remembered_height = unit.air_avoidance.update_hover_look_ahead(future_height);
        let Some(base_height) = current_height.map(|height| height.max(remembered_height)) else {
            return;
        };
        let goal_altitude =
            base_height + context.hover_altitude_offset + context.vertical_avoidance_offset;
        let altitude_change = goal_altitude - context.position.y;
        let goal_climb_rate = if altitude_change < 0.0 {
            altitude_change * DESCENT_RATE_MULTIPLIER
        } else {
            altitude_change
        };
        let throttle = goal_climb_rate - unit.air_avoidance.hover_vertical_velocity;
        unit.air_avoidance.hover_vertical_velocity += throttle * dt;
        let vertical_velocity = unit.air_avoidance.hover_vertical_velocity;
        let next_unit_altitude = context.position.y + vertical_velocity * dt;
        let Some(squad) = self.squads.get_mut(context.squad_id) else {
            return;
        };
        squad.base.position.y = next_unit_altitude - context.formation_height;
        squad.base.velocity.y = vertical_velocity;
    }

    fn current_hover_height(&self, context: HoverContext) -> Option<f32> {
        let forward = planar_direction_or(context.forward, Vec3::Z);
        let right = Vec3::Y.cross(forward);
        [
            context.position + forward * SAMPLE_RADIUS_Z,
            context.position - right * SAMPLE_RADIUS_X - forward * SAMPLE_RADIUS_Z,
            context.position + right * SAMPLE_RADIUS_X - forward * SAMPLE_RADIUS_Z,
        ]
        .into_iter()
        .filter_map(|position| self.base_hover_height(position, context.hover_altitude_offset))
        .reduce(f32::max)
    }

    fn future_hover_height(&self, context: HoverContext) -> Option<f32> {
        if !context.has_move_action {
            return None;
        }
        let look_ahead_time = LOOK_AHEAD_SECONDS * LOOK_AHEAD_MULTIPLIERS[context.look_ahead_index];
        let position = context.position + planar(context.velocity) * look_ahead_time;
        self.base_hover_height(position, context.hover_altitude_offset)
    }

    fn base_hover_height(&self, position: Vec3, hover_altitude_offset: f32) -> Option<f32> {
        let terrain = self.terrain_height(position, true)?;
        let flight_height = terrain + TERRAIN_CLEARANCE;
        let minimum = terrain + TERRAIN_PROBE_OFFSET + MINIMUM_AGL - hover_altitude_offset;
        Some(flight_height.max(minimum))
    }
}

fn planar_direction_or(value: Vec3, fallback: Vec3) -> Vec3 {
    let direction = planar(value).normalize_or_zero();
    if direction == Vec3::ZERO {
        fallback
    } else {
        direction
    }
}

fn planar(value: Vec3) -> Vec3 {
    Vec3::new(value.x, 0.0, value.z)
}
