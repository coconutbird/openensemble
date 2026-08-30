//! Retail `BUnitActionMoveAir::updateTactics` combat cycle.

use super::super::World;
use crate::entities::units::{MoveAirState, MoveAirTacticState};
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use glam::Vec3;

const RETURN_SECONDS: f32 = 2.0;
const NAVIGATE_SECONDS: f32 = 1.5;
const HOVER_SECONDS: f32 = 2.0;
const MAX_STRAY_DISTANCE: f32 = 35.0;
const STRAY_RETURN_SECONDS: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
struct TacticContext {
    squad_position: Vec3,
    distance_from_squad: f32,
    leash_distance: f32,
    attack_target: Option<Vec3>,
    state: MoveAirState,
}

impl World {
    pub(in crate::world) fn update_move_air_tactics(
        &mut self,
        dt: f32,
        gameplay: &GameplayCatalog,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            let Some(context) = self.move_air_tactic_context(unit_id, gameplay) else {
                continue;
            };
            let state = advance_tactic(context, dt);
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.set_move_air_state(state);
            }
        }
    }

    fn move_air_tactic_context(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<TacticContext> {
        let unit = self.units.get(unit_id).filter(|unit| {
            super::super::air_avoidance::is_active_flight(unit) && unit.is_move_air_working()
        })?;
        let squad = self.squads.get(unit.squad_id?)?;
        let attack_target_id = unit.attack_target.or(squad.attack_target);
        let attack_target = squad.carpet_bomb_attack_position().or_else(|| {
            attack_target_id.and_then(|target_id| {
                self.active_ranged_attack_target_position(unit_id, target_id, gameplay)
            })
        });
        Some(TacticContext {
            squad_position: squad.base.position,
            distance_from_squad: planar(unit.base.position - squad.base.position).length(),
            leash_distance: squad.leash_distance,
            attack_target,
            state: unit.move_air_state()?,
        })
    }
}

fn advance_tactic(context: TacticContext, dt: f32) -> MoveAirState {
    let mut state = context.state;
    state.hover_timer = (state.hover_timer - dt).max(0.0);
    let maximum_stray = MAX_STRAY_DISTANCE.max(context.leash_distance * 0.5);
    if context.distance_from_squad > maximum_stray {
        state.tactic = MoveAirTacticState::ReturnToSquad;
        state.hover_timer = STRAY_RETURN_SECONDS;
    }
    match state.tactic {
        MoveAirTacticState::ReturnToSquad => return_to_squad(&mut state, context.squad_position),
        MoveAirTacticState::Navigate => navigate(&mut state, context),
        MoveAirTacticState::Strafe => strafe(&mut state, context.attack_target),
        MoveAirTacticState::LaunchHover => launch_hover(&mut state),
    }
    state.goal_position_valid = true;
    state
}

fn return_to_squad(state: &mut MoveAirState, squad_position: Vec3) {
    state.attack_blocked = true;
    state.goal_position = squad_position;
    if state.hover_timer <= 0.0 {
        state.tactic = MoveAirTacticState::Navigate;
        state.hover_timer = NAVIGATE_SECONDS;
    }
}

fn navigate(state: &mut MoveAirState, context: TacticContext) {
    state.attack_blocked = true;
    state.goal_position = context
        .attack_target
        .map_or(context.squad_position, |target| {
            Vec3::midpoint(target, context.squad_position)
        });
    if context.attack_target.is_some() {
        state.tactic = MoveAirTacticState::Strafe;
    }
}

fn strafe(state: &mut MoveAirState, attack_target: Option<Vec3>) {
    if attack_target.is_none() {
        state.tactic = MoveAirTacticState::Navigate;
    } else if state.hover_timer <= 0.0 {
        state.tactic = MoveAirTacticState::LaunchHover;
        state.hover_timer = HOVER_SECONDS;
    }
}

fn launch_hover(state: &mut MoveAirState) {
    if state.hover_timer > 0.0 {
        state.goal_speed = 0.0;
        state.attack_blocked = false;
    } else {
        state.hover_timer = RETURN_SECONDS;
        state.tactic = MoveAirTacticState::ReturnToSquad;
        state.attack_blocked = true;
    }
}

fn planar(value: Vec3) -> Vec3 {
    Vec3::new(value.x, 0.0, value.z)
}
