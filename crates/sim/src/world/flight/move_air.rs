//! Retail `BUnitActionMoveAir` navigation baseline.

use super::super::World;
use crate::entities::units::{MoveAirActionState, MoveAirState, Unit};
use crate::entity_id::EntityId;
use glam::Vec3;

const ALTITUDE_CHANGE_SECONDS: f32 = 4.0;
const FLOOD_ALTITUDE_CHANGE_SECONDS: f32 = 2.0;
const FLOOD_SPEED_CHANGE_SECONDS: f32 = 1.0;
const FORWARD_TERRAIN_PROBE_SCALE: f32 = 3.0;

#[derive(Debug, Clone, Copy)]
struct MoveAirContext {
    position: Vec3,
    forward: Vec3,
    velocity: Vec3,
    goal_position: Vec3,
    maximum_speed: f32,
    maximum_turn_rate: f32,
    flood: bool,
    suspended: bool,
    state: MoveAirState,
}

#[derive(Debug, Clone, Copy)]
struct MoveAirResult {
    position: Vec3,
    forward: Vec3,
    velocity: Vec3,
    state: MoveAirState,
}

impl World {
    pub(in crate::world) fn update_move_air(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            self.initialize_unlinked_move_air(unit_id);
            let Some(action) = self
                .units
                .get(unit_id)
                .and_then(Unit::move_air_state)
                .map(|state| state.action)
            else {
                continue;
            };
            match action {
                MoveAirActionState::None => {
                    self.update_parked_move_air(unit_id);
                    continue;
                }
                MoveAirActionState::Pathing => {
                    self.finish_move_air_pathing(unit_id);
                    continue;
                }
                MoveAirActionState::Working => {}
            }
            let Some(context) = self.move_air_context(unit_id) else {
                continue;
            };
            if context.suspended {
                if let Some(unit) = self.units.get_mut(unit_id) {
                    unit.base.velocity = Vec3::ZERO;
                }
                continue;
            }
            let result = self.calculate_move_air(context, dt);
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            unit.base.position = result.position;
            unit.base.set_forward(result.forward);
            unit.base.velocity = result.velocity;
            unit.set_move_air_state(result.state);
        }
    }

    fn initialize_unlinked_move_air(&mut self, unit_id: EntityId) {
        let Some(unit) = self.units.get(unit_id) else {
            return;
        };
        let Some(mut state) = unit.move_air_state() else {
            return;
        };
        if state.lifecycle.initialized() || unit.squad_id.is_none() {
            return;
        }
        state.lifecycle.set_initialized(true);
        state.base_position = unit.base.position;
        state.pad_position = unit.base.position;
        state.lifecycle.set_pad_position_valid(true);
        state.spot_forward = unit.base.forward;
        state.goal_position = unit.base.position;
        state.goal_position_valid = true;
        state.lifecycle.set_launch_requested(true);
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.set_move_air_state(state);
        }
    }

    fn update_parked_move_air(&mut self, unit_id: EntityId) {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        let Some(mut state) = unit.move_air_state() else {
            return;
        };
        let maximum_ammunition = unit.ammunition.maximum();
        unit.ammunition.set_current(maximum_ammunition);
        state.lifecycle.set_returning_to_base(false);
        unit.base.velocity = Vec3::ZERO;
        if state.lifecycle.pad_position_valid() {
            unit.base.position = state.pad_position;
            unit.base.set_forward(state.spot_forward);
        }
        if state.lifecycle.launch_requested() || state.air_base.is_none() {
            state.action = MoveAirActionState::Pathing;
        }
        unit.set_move_air_state(state);
    }

    fn finish_move_air_pathing(&mut self, unit_id: EntityId) {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        let Some(mut state) = unit.move_air_state() else {
            return;
        };
        state.action = MoveAirActionState::Working;
        unit.base.velocity = Vec3::ZERO;
        unit.set_move_air_state(state);
    }

    fn move_air_context(&self, unit_id: EntityId) -> Option<MoveAirContext> {
        let unit = self.units.get(unit_id).filter(|unit| {
            super::super::air_avoidance::is_active_flight(unit) && unit.is_move_air_working()
        })?;
        let squad = self.squads.get(unit.squad_id?)?;
        let state = unit.move_air_state()?;
        Some(MoveAirContext {
            position: unit.base.position,
            forward: unit.base.forward,
            velocity: unit.base.velocity,
            goal_position: if state.goal_position_valid {
                state.goal_position
            } else {
                squad.base.position
            },
            maximum_speed: unit.speed * unit.effective_velocity_scalar(),
            maximum_turn_rate: unit.turn_rate_degrees.to_radians().max(0.0),
            flood: unit.is_object_type("Flood"),
            suspended: squad.is_being_pulled() || squad.is_jumping() || squad.is_cryo_frozen(),
            state,
        })
    }

    fn calculate_move_air(&mut self, context: MoveAirContext, dt: f32) -> MoveAirResult {
        let mut state = context.state;
        select_speed_goal(&mut self.sim_rng, context, &mut state, dt);
        select_altitude_goal(&mut self.sim_rng, context, &mut state);
        let heading = move_air_heading(context, &mut state, dt);
        let speed = move_air_speed(context, state, dt);
        let altitude_change = self.move_air_altitude_change(context, &mut state, dt);
        state.altitude_select_timer -= dt;
        state.previous_altitude_change = altitude_change;

        let velocity = heading * speed;
        let position = context.position + velocity * dt + Vec3::Y * altitude_change;
        let mut forward = heading;
        forward.y = state.previous_altitude_change;
        MoveAirResult {
            position,
            forward: forward.normalize_or_zero(),
            velocity,
            state,
        }
    }

    fn move_air_altitude_change(
        &self,
        context: MoveAirContext,
        state: &mut MoveAirState,
        dt: f32,
    ) -> f32 {
        state.current_altitude_increment +=
            2.0 * (state.goal_altitude_increment - state.current_altitude_increment) * dt;
        let Some(current_height) = self.terrain_height(context.position, true) else {
            return 0.0;
        };
        let forward_position = context.position
            + planar_direction_or(context.forward, Vec3::Z) * FORWARD_TERRAIN_PROBE_SCALE;
        let forward_height = self
            .terrain_height(forward_position, true)
            .unwrap_or(current_height);
        let new_height = current_height.max(forward_height) + state.height_displacement;
        let difference =
            2.0 * new_height + state.current_altitude_increment - 2.0 * context.position.y;
        difference * dt
    }
}

fn select_speed_goal(
    random: &mut crate::random::SimRandom,
    context: MoveAirContext,
    state: &mut MoveAirState,
    dt: f32,
) {
    if !context.flood {
        return;
    }
    if state.speed_select_timer <= 0.0 {
        state.goal_speed = random
            .range_float(0.7 * context.maximum_speed, context.maximum_speed)
            .max(0.4 * context.maximum_speed);
        state.speed_select_timer = FLOOD_SPEED_CHANGE_SECONDS;
    }
    state.speed_select_timer -= dt;
}

fn select_altitude_goal(
    random: &mut crate::random::SimRandom,
    context: MoveAirContext,
    state: &mut MoveAirState,
) {
    if state.altitude_select_timer > 0.0 {
        return;
    }
    state.altitude_select_timer = if context.flood {
        FLOOD_ALTITUDE_CHANGE_SECONDS
    } else {
        ALTITUDE_CHANGE_SECONDS
    };
    state.goal_altitude_increment = random.range_float(
        state.height_displacement - 5.0,
        state.height_displacement + 15.0,
    );
}

fn move_air_speed(context: MoveAirContext, state: MoveAirState, dt: f32) -> f32 {
    let current_speed = planar(context.velocity).length();
    let goal_speed = if context.flood {
        state.goal_speed
    } else {
        0.6 * context.maximum_speed
    };
    current_speed + (goal_speed - current_speed) * dt
}

fn move_air_heading(context: MoveAirContext, state: &mut MoveAirState, dt: f32) -> Vec3 {
    let heading = planar_direction_or(context.forward, Vec3::Z);
    let goal_heading = planar_direction_or(context.goal_position - context.position, heading);
    let cross = heading.cross(goal_heading);
    let dot = heading.dot(goal_heading);
    let mut goal_turn_rate = cross.length().sqrt();
    if dot < 0.0 {
        goal_turn_rate = 1.0;
    }
    goal_turn_rate *= context.maximum_turn_rate;
    if cross.y < 0.0 {
        goal_turn_rate = -goal_turn_rate;
    }
    let maximum_delta = context.maximum_turn_rate * dt;
    state.turn_rate += (goal_turn_rate - state.turn_rate).clamp(-maximum_delta, maximum_delta);
    state.turn_rate = state
        .turn_rate
        .clamp(-context.maximum_turn_rate, context.maximum_turn_rate);
    rotate_planar(heading, state.turn_rate * dt)
}

fn rotate_planar(vector: Vec3, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    Vec3::new(
        vector.x.mul_add(cos, vector.z * sin),
        0.0,
        (-vector.x).mul_add(sin, vector.z * cos),
    )
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
