//! Fixed-step movement and combat-value healing for `RepairOther`.

use super::effects::RepairEffectContext;
use super::{RepairTarget, World, planar_direction};
use crate::entities::{RepairOtherPhase, SquadState, UnitState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{GameplayCatalog, RepairOtherActionProfile};
use crate::player::PlayerId;
use glam::Vec3;
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct RepairUpdateContext {
    source_player_id: PlayerId,
    source_leader_id: EntityId,
    source_proto_object_name: String,
    target: RepairTarget,
    profile: RepairOtherActionProfile,
}

impl World {
    pub(in crate::world) fn update_repair_other(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let source_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| {
                matches!(
                    squad.repair_other.phase(),
                    RepairOtherPhase::Moving | RepairOtherPhase::Working
                )
                .then_some(id)
            })
            .collect::<Vec<_>>();
        for source_id in source_ids {
            self.update_repair_other_squad(source_id, dt, database, gameplay);
        }
    }

    fn update_repair_other_squad(
        &mut self,
        source_squad_id: EntityId,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) {
        let Some(context) = self.repair_update_context(source_squad_id, gameplay) else {
            self.finish_repair_other_order(source_squad_id, RepairOtherPhase::Done);
            return;
        };
        if !self.players_are_allied(context.source_player_id, context.target.player_id)
            || self.squad_hitpoint_fraction(context.target.squad_id, database) <= 0.0
        {
            self.finish_repair_other_order(source_squad_id, RepairOtherPhase::Done);
            return;
        }
        if !self.repair_other_in_range(source_squad_id, &context) {
            self.move_repair_other_into_range(source_squad_id, &context);
            return;
        }
        self.hold_repair_other_position(source_squad_id, &context.target);
        let effect = RepairEffectContext {
            source_leader_id: context.source_leader_id,
            target_leader_id: context.target.leader_id,
            target_player_id: context.target.player_id,
            profile: &context.profile,
        };
        self.ensure_repair_other_effect(source_squad_id, &effect, database);
        let work_rate = self.repair_other_work_rate(&context);
        let excess = self.repair_squad_by_combat_value(
            database,
            context.target.squad_id,
            finite_nonnegative(work_rate * dt),
            context.profile.allow_reinforce(),
        );
        if excess > 0.0 {
            self.finish_repair_other_order(source_squad_id, RepairOtherPhase::Done);
        }
    }

    fn repair_update_context(
        &self,
        source_squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<RepairUpdateContext> {
        let state = &self.squads.get(source_squad_id)?.repair_other;
        let source_player_id = state.source_player_id()?;
        let target_squad_id = state.target_id()?;
        let action_name = state.action_name()?;
        let source_leader_id = self.repair_other_source(source_player_id, source_squad_id)?;
        let source = self.units.get(source_leader_id)?;
        let profile = gameplay
            .repair_other_action(&source.proto_object_name, action_name)?
            .clone();
        if !self.repair_other_profile_enabled(source, &profile) {
            return None;
        }
        Some(RepairUpdateContext {
            source_player_id,
            source_leader_id,
            source_proto_object_name: source.proto_object_name.clone(),
            target: self.repair_target(target_squad_id)?,
            profile,
        })
    }

    pub(super) fn repair_other_profile_enabled(
        &self,
        source: &crate::entities::Unit,
        profile: &RepairOtherActionProfile,
    ) -> bool {
        let authored_enabled = !profile.starts_disabled();
        let player_enabled =
            self.get_player(source.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &source.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    )
                });
        source
            .actions
            .is_enabled(profile.action_name(), !player_enabled)
    }

    fn repair_other_work_rate(&self, context: &RepairUpdateContext) -> f32 {
        self.get_player(context.source_player_id)
            .map_or(context.profile.work_rate(), |player| {
                player.technologies.action_work_rate(
                    &context.source_proto_object_name,
                    context.profile.action_name(),
                    context.profile.work_rate(),
                )
            })
    }

    fn repair_other_in_range(
        &self,
        source_squad_id: EntityId,
        context: &RepairUpdateContext,
    ) -> bool {
        let Some(source) = self.squads.get(source_squad_id) else {
            return false;
        };
        let delta = context.target.position - source.base.position;
        let center_distance = delta.x.hypot(delta.z);
        let surface_distance = (center_distance
            - self.squad_obstruction_radius(source_squad_id)
            - self.squad_obstruction_radius(context.target.squad_id))
        .max(0.0);
        surface_distance < context.profile.work_range()
    }

    fn move_repair_other_into_range(
        &mut self,
        source_squad_id: EntityId,
        context: &RepairUpdateContext,
    ) {
        let effect_ids = self
            .squads
            .get_mut(source_squad_id)
            .map(|squad| squad.repair_other.take_effect_ids())
            .unwrap_or_default();
        self.remove_repair_other_effects(effect_ids);
        let Some((source_position, source_forward)) = self
            .squads
            .get(source_squad_id)
            .map(|squad| (squad.base.position, squad.base.forward))
        else {
            return;
        };
        let mut away = planar_direction(source_position, context.target.position);
        if away == Vec3::ZERO {
            away = -Vec3::new(source_forward.x, 0.0, source_forward.z).normalize_or_zero();
        }
        if away == Vec3::ZERO {
            away = Vec3::NEG_Z;
        }
        let inside_range = (context.profile.work_range() - 0.001).max(0.0);
        let distance = self.squad_obstruction_radius(source_squad_id)
            + self.squad_obstruction_radius(context.target.squad_id)
            + inside_range;
        let mut destination = context.target.position + away * distance;
        destination.y = source_position.y;
        if let Some(squad) = self.squads.get_mut(source_squad_id) {
            squad.start_direct_move(destination);
            squad.repair_other.set_phase(RepairOtherPhase::Moving);
        }
    }

    fn hold_repair_other_position(&mut self, source_squad_id: EntityId, target: &RepairTarget) {
        let member_ids = self
            .squads
            .get(source_squad_id)
            .map(|squad| squad.unit_ids.clone())
            .unwrap_or_default();
        if let Some(squad) = self.squads.get_mut(source_squad_id) {
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
            squad.state = SquadState::Working;
            squad.repair_other.set_phase(RepairOtherPhase::Working);
            let facing = planar_direction(target.position, squad.base.position);
            if facing != Vec3::ZERO {
                squad.base.set_forward(facing);
            }
        }
        for member_id in member_ids {
            if let Some(unit) = self.units.get_mut(member_id) {
                unit.move_target = None;
                unit.base.velocity = Vec3::ZERO;
                if unit.is_alive() {
                    unit.state = UnitState::Idle;
                }
            }
        }
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}
