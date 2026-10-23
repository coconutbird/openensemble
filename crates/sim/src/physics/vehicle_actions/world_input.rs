//! Builds the [`VehicleActionInput`] of one unit from the sim `World`.

use glam::Vec3;

use super::hover::SAMPLE_POINTS;
use super::input::{
    AirInput, CrashInput, HeightSample, MoveActionInput, RangedTarget, SquadInput, UnitInput,
    VehicleActionInput,
};
use super::{VehicleAction, unit_right};
use crate::entities::{AircraftCrashPhase, Squad, Unit};
use crate::entity_id::EntityId;
use crate::world::World;

/// The flight height above the terrain that retail uses without flight and camera heights
/// (`getHeight` plus 16).
const FALLBACK_FLIGHT_CLEARANCE: f32 = 16.0;
/// The look-ahead time of slot `i` is `0.2 · (1 + i)` seconds.
const LOOK_AHEAD_STEP_SECONDS: f32 = 0.2;
/// `1 + i` for the five look-ahead slots.
const LOOK_AHEAD_STEPS: [f32; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];

/// Builds the input of the action of `unit_id`. A missing unit gives an input at the origin
/// with no squad, which the hook never asks for.
pub(super) fn build(
    world: &World,
    unit_id: EntityId,
    action: &VehicleAction,
) -> VehicleActionInput {
    let Some(unit) = world.units.get(unit_id) else {
        return VehicleActionInput {
            unit: unit_input(world, None),
            squad: None,
            air: None,
        };
    };
    let squad = unit
        .squad_id
        .and_then(|id| world.squads.get(id))
        .map(|squad| squad_input(world, squad));
    let air = action
        .is_hover()
        .then(|| air_input(world, unit, squad.as_ref()));
    VehicleActionInput {
        unit: unit_input(world, Some(unit)),
        squad,
        air,
    }
}

fn unit_input(world: &World, unit: Option<&Unit>) -> UnitInput {
    let Some(unit) = unit else {
        return UnitInput {
            position: Vec3::ZERO,
            forward: Vec3::Z,
            right: Vec3::X,
            obstruction_radius: 0.0,
            proto_desired_velocity: 0.0,
            max_velocity: 0.0,
            reverse_speed: -1.0,
            turn_rate_degrees: 0.0,
            has_physics_action: false,
            doing_fatality: false,
            ranged_target: None,
        };
    };
    let scalar = unit.velocity_scalar;
    let ranged_target = unit.combat.engaged_target().map(|target| {
        let entity_position = world.entity_position(target);
        RangedTarget {
            entity_position,
            position: entity_position.unwrap_or(unit.base.position),
        }
    });
    UnitInput {
        position: unit.base.position,
        forward: unit.base.forward,
        right: unit_right(unit.base.forward, unit.base.up),
        obstruction_radius: unit.obstruction_radius(),
        proto_desired_velocity: unit.desired_velocity,
        max_velocity: unit.speed * scalar,
        reverse_speed: unit.reverse_speed() * scalar,
        turn_rate_degrees: unit.turn_rate_degrees,
        has_physics_action: unit
            .physics_object
            .as_ref()
            .is_some_and(|state| !state.is_fixed()),
        doing_fatality: false,
        ranged_target,
    }
}

/// The squad and its move action. The sim squad has one move order at a time; its target is
/// the move target, and the move action exists while the squad has one.
fn squad_input(world: &World, squad: &Squad) -> SquadInput {
    let leader = squad.unit_ids.first().and_then(|id| world.units.get(*id));
    let desired_velocity = leader.map_or(0.0, |leader| {
        leader.desired_velocity * leader.velocity_scalar
    });
    let (turn_radius_position, turn_radius_forward) = squad
        .turn_radius()
        .unwrap_or((squad.base.position, squad.base.forward));
    let move_action = squad.move_target.map(|target| {
        let speed = Vec3::new(squad.base.velocity.x, 0.0, squad.base.velocity.z).length();
        let path_speed = if speed > 0.0 { speed } else { desired_velocity };
        MoveActionInput {
            target,
            working: true,
            waiting: false,
            velocity: speed,
            future_positions: future_positions(squad.base.position, target, path_speed),
        }
    });
    SquadInput {
        position: squad.base.position,
        turn_radius_position,
        turn_radius_forward,
        desired_velocity,
        stasis_speed_multiplier: squad.stasis_speed_multiplier(),
        move_action,
    }
}

/// `BSquadActionMove::getFuturePosition`: the point `speed · t` along the path from the squad
/// to the target, or the target past its end.
fn future_positions(start: Vec3, target: Vec3, speed: f32) -> [Vec3; 5] {
    let offset = target - start;
    let length = Vec3::new(offset.x, 0.0, offset.z).length();
    LOOK_AHEAD_STEPS.map(|steps| {
        let distance = speed * LOOK_AHEAD_STEP_SECONDS * steps;
        if length > 0.0 && distance < length {
            start + offset * (distance / length)
        } else {
            target
        }
    })
}

fn air_input(world: &World, unit: &Unit, squad: Option<&SquadInput>) -> AirInput {
    let state = &unit.air_avoidance;
    let crash = (unit.aircraft_crash_phase() == AircraftCrashPhase::Crashing).then(|| CrashInput {
        position: state.crash_position,
        kamikaze_target: state
            .kamikaze_target
            .filter(|&target| {
                world
                    .units
                    .get(target)
                    .is_some_and(|unit| unit.base.is_alive())
            })
            .and_then(|target| world.entity_position(target)),
    });
    let forward = unit.base.forward;
    let up = unit.base.up;
    let right = unit_right(forward, up);
    let position = unit.base.position;
    let sample = |point: Vec3| {
        let terrain = world.terrain_height(point, true).unwrap_or(point.y);
        HeightSample {
            terrain,
            flight: terrain + FALLBACK_FLIGHT_CLEARANCE,
        }
    };
    let body_samples = SAMPLE_POINTS
        .map(|local| sample(position + right * local.x + up * local.y + forward * local.z));
    let future = squad
        .and_then(|squad| squad.move_action)
        .map_or([position; 5], |move_action| move_action.future_positions);
    AirInput {
        hover_altitude_offset: state.hover_altitude_offset(),
        max_pitch_roll: state.max_pitch_roll(),
        crash,
        avoiding_obstacle_altitude: state.avoiding_obstacle_altitude(),
        speed_limited: unit.is_air_speed_limited(),
        transport_hover_offset: None,
        body_samples,
        future_samples: future.map(sample),
    }
}
