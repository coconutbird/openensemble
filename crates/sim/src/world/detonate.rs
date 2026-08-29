//! Retail squad arming, contact opportunities, and unit Detonate explosions.

mod passive;
mod replacement;

use super::World;
use crate::entities::squads::DetonateOrder;
use crate::entities::{SquadDetonatePhase, SquadMode, Unit, UnitDetonatePhase};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{
    AttackQuery, AttackQueryFlags, DetonateActionProfile, GameplayCatalog, TacticRelation,
    TacticStateId,
};
use crate::player::{PlayerId, TeamRelation};
use glam::Vec3;

const CONTACT_EPSILON: f32 = 0.001;

#[derive(Debug, Clone)]
struct DetonateTargetSnapshot {
    position: Vec3,
    bounds_min: Vec3,
    bounds_max: Vec3,
    obstruction_radius: f32,
    player_id: PlayerId,
    proto_object_name: String,
    flags: AttackQueryFlags,
}

impl DetonateTargetSnapshot {
    fn contains(&self, position: Vec3) -> bool {
        let epsilon = Vec3::splat(CONTACT_EPSILON);
        position.cmpge(self.bounds_min - epsilon).all()
            && position.cmple(self.bounds_max + epsilon).all()
    }
}

#[derive(Debug, Clone)]
struct DetonateExplosion {
    unit_id: EntityId,
    squad_id: Option<EntityId>,
    source_player_id: PlayerId,
    instigator_player_id: PlayerId,
    position: Vec3,
    proto_object_name: String,
    profile: DetonateActionProfile,
    damage_multiplier: f32,
}

impl World {
    /// Issue a targeted Detonate order to one live, owned squad.
    pub fn issue_detonate_order(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        target_id: EntityId,
        requested_ability_id: Option<u8>,
    ) -> bool {
        if self.get_player(player_id).is_none() || squad_id == target_id {
            return false;
        }
        let Some(target_position) = self
            .detonate_target_snapshot(target_id)
            .map(|target| target.position)
        else {
            return false;
        };
        self.squads.get_mut(squad_id).is_some_and(|squad| {
            squad.base.player_id == player_id
                && squad.begin_detonate_order(
                    DetonateOrder {
                        target_id,
                        requested_ability_id,
                    },
                    target_position,
                )
        })
    }

    pub(super) fn update_detonations(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.resolve_dead_unit_detonations(gameplay);
        self.reconcile_unit_detonate_state();
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| squad.is_detonating().then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.update_squad_detonate(squad_id, dt, gameplay);
        }
        self.advance_active_unit_detonations(dt, gameplay);
        self.resolve_dead_unit_detonations(gameplay);
    }

    fn update_squad_detonate(&mut self, squad_id: EntityId, dt: f32, gameplay: &GameplayCatalog) {
        let Some((order, phase, action_name, source_position, source_radius)) =
            self.detonate_squad_snapshot(squad_id)
        else {
            self.cancel_detonate(squad_id);
            return;
        };
        let Some(target) = self.detonate_target_snapshot(order.target_id) else {
            self.cancel_detonate(squad_id);
            return;
        };
        let Some((profile, newly_selected)) =
            self.squad_detonate_profile(squad_id, order, action_name.as_deref(), &target, gameplay)
        else {
            self.cancel_detonate(squad_id);
            return;
        };
        if newly_selected {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.detonate.select_action(profile.action_name());
                squad.follow_detonate_target(target.position);
            }
            return;
        }
        if phase != SquadDetonatePhase::Attacking
            && let Some(squad) = self.squads.get_mut(squad_id)
        {
            squad.follow_detonate_target(target.position);
        }

        let center_distance = planar_distance(source_position, target.position);
        let edge_distance = center_distance - source_radius - target.obstruction_radius;
        match phase {
            SquadDetonatePhase::Inactive => self.cancel_detonate(squad_id),
            SquadDetonatePhase::Moving => {
                if edge_distance <= profile.glowy_range() {
                    self.start_detonate_glow(squad_id, &profile, gameplay);
                }
            }
            SquadDetonatePhase::Glowing => {
                if edge_distance > profile.glowy_range() {
                    self.cancel_detonate(squad_id);
                } else if center_distance <= target.obstruction_radius + profile.work_range() * 0.5
                {
                    self.enter_detonate_attack(squad_id, target.position);
                }
            }
            SquadDetonatePhase::Attacking => {
                if edge_distance > profile.glowy_range() {
                    self.cancel_detonate(squad_id);
                } else {
                    self.advance_detonate_attack_members(squad_id, dt);
                    self.create_contact_detonate_actions(squad_id, &target, &profile);
                }
            }
        }
    }

    fn detonate_squad_snapshot(
        &self,
        squad_id: EntityId,
    ) -> Option<(DetonateOrder, SquadDetonatePhase, Option<String>, Vec3, f32)> {
        let squad = self.squads.get(squad_id)?;
        if !squad.is_alive() || squad.unit_ids.is_empty() {
            return None;
        }
        Some((
            squad.detonate.order()?,
            squad.detonate.phase(),
            squad.detonate.action_name().map(str::to_owned),
            squad.base.position,
            self.detonate_squad_obstruction_radius(squad_id),
        ))
    }

    fn squad_detonate_profile(
        &self,
        squad_id: EntityId,
        order: DetonateOrder,
        action_name: Option<&str>,
        target: &DetonateTargetSnapshot,
        gameplay: &GameplayCatalog,
    ) -> Option<(DetonateActionProfile, bool)> {
        let squad = self.squads.get(squad_id)?;
        let unit = squad.unit_ids.iter().find_map(|unit_id| {
            self.units
                .get(*unit_id)
                .filter(|unit| unit.is_operational())
        })?;
        if let Some(action_name) = action_name {
            return gameplay
                .detonate_action(&unit.proto_object_name, action_name)
                .map(|profile| (profile, false));
        }
        let query = AttackQuery {
            relation: self.detonate_relation(squad.base.player_id, target.player_id),
            squad_mode: SquadMode::Normal,
            ability_id: order.requested_ability_id,
            target_proto_object_name: Some(&target.proto_object_name),
            tactic_state: unit.tactic_state(),
            flags: target.flags,
        };
        gameplay
            .select_detonate_action(&unit.proto_object_name, &query, |action| {
                self.detonate_action_enabled(unit, action)
            })
            .map(|profile| (profile, true))
    }

    fn detonate_action_enabled(
        &self,
        unit: &Unit,
        action: &pipeline::database::hw1::tactics::Action,
    ) -> bool {
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
    }

    fn start_detonate_glow(
        &mut self,
        squad_id: EntityId,
        profile: &DetonateActionProfile,
        gameplay: &GameplayCatalog,
    ) {
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.detonate.enter_glowing();
        }
        let state_zero = TacticStateId::from_index(0).expect("state zero is representable");
        for unit_id in unit_ids {
            let Some(unit) = self
                .units
                .get_mut(unit_id)
                .filter(|unit| unit.is_operational())
            else {
                continue;
            };
            unit.arm_detonate(profile.action_name(), profile.velocity_scalar());
            unit.combat.stop_firing();
            if gameplay
                .tactic_state(&unit.proto_object_name, state_zero)
                .is_some()
            {
                unit.set_tactic_state(state_zero);
            }
        }
    }

    fn enter_detonate_attack(&mut self, squad_id: EntityId, target_position: Vec3) {
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.enter_detonate_attacking(target_position);
        }
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.collision_attack.finish();
            }
        }
    }

    fn create_contact_detonate_actions(
        &mut self,
        squad_id: EntityId,
        target: &DetonateTargetSnapshot,
        profile: &DetonateActionProfile,
    ) {
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return;
        };
        for unit_id in unit_ids {
            let in_target = self
                .units
                .get(unit_id)
                .is_some_and(|unit| unit.is_operational() && target.contains(unit.base.position));
            if in_target {
                let _started =
                    self.begin_profile_detonate_action(unit_id, profile, true, false, None);
            }
        }
    }

    fn advance_detonate_attack_members(&mut self, squad_id: EntityId, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return;
        };
        for unit_id in unit_ids {
            let Some((offset, movement)) = self.units.get(unit_id).and_then(|unit| {
                unit.is_operational().then(|| {
                    (
                        unit.formation_offset,
                        unit.speed * unit.effective_velocity_scalar() * dt,
                    )
                })
            }) else {
                continue;
            };
            let planar = Vec3::new(offset.x, 0.0, offset.z);
            let distance = planar.length();
            if distance <= CONTACT_EPSILON {
                continue;
            }
            let next_planar = if movement >= distance {
                Vec3::ZERO
            } else {
                planar * ((distance - movement.max(0.0)) / distance)
            };
            let _updated = self.set_squad_member_formation_offset(
                unit_id,
                Vec3::new(next_planar.x, offset.y, next_planar.z),
            );
        }
    }

    fn cancel_detonate(&mut self, squad_id: EntityId) {
        let Some(unit_ids) = self
            .squads
            .get(squad_id)
            .map(|squad| squad.unit_ids.clone())
        else {
            return;
        };
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.cancel_detonate_action();
                unit.clear_tactic_state();
                unit.collision_attack.finish();
            }
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.finish_detonate_order();
        }
    }

    fn reconcile_unit_detonate_state(&mut self) {
        let stale = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                let squad_armed = unit.is_detonate_armed()
                    && unit.detonate_phase() == UnitDetonatePhase::Inactive;
                let parent_active = unit
                    .squad_id
                    .and_then(|squad_id| self.squads.get(squad_id))
                    .is_some_and(crate::entities::Squad::is_detonating);
                (squad_armed && !parent_active).then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in stale {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.cancel_detonate_action();
                unit.clear_tactic_state();
            }
        }
    }

    fn detonate_target_snapshot(&self, target_id: EntityId) -> Option<DetonateTargetSnapshot> {
        if let Some(unit) = self
            .units
            .get(target_id)
            .filter(|unit| unit.is_attackable())
        {
            return Some(detonate_unit_target(unit));
        }
        let squad = self
            .squads
            .get(target_id)
            .filter(|squad| squad.is_alive())?;
        let units = squad
            .unit_ids
            .iter()
            .filter_map(|unit_id| self.units.get(*unit_id))
            .filter(|unit| unit.is_attackable())
            .collect::<Vec<_>>();
        detonate_squad_target(squad.base.position, &units)
    }

    fn detonate_squad_obstruction_radius(&self, squad_id: EntityId) -> f32 {
        self.squads.get(squad_id).map_or(0.0, |squad| {
            squad
                .unit_ids
                .iter()
                .filter_map(|unit_id| self.units.get(*unit_id))
                .filter(|unit| unit.is_alive())
                .map(|unit| {
                    let delta = unit.base.position - squad.base.position;
                    planar_length(delta) + unit.obstruction_radius()
                })
                .fold(0.0, f32::max)
        })
    }

    fn detonate_relation(&self, source: PlayerId, target: PlayerId) -> TacticRelation {
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

fn detonate_unit_target(unit: &Unit) -> DetonateTargetSnapshot {
    let (center, half_extents) = unit.simulation_bounds();
    DetonateTargetSnapshot {
        position: unit.base.position,
        bounds_min: center - half_extents,
        bounds_max: center + half_extents,
        obstruction_radius: unit.obstruction_radius(),
        player_id: unit.base.player_id,
        proto_object_name: unit.proto_object_name.clone(),
        flags: target_flags(unit),
    }
}

fn detonate_squad_target(position: Vec3, units: &[&Unit]) -> Option<DetonateTargetSnapshot> {
    let first = *units.first()?;
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
    let mut obstruction_radius = 0.0_f32;
    let mut flags = AttackQueryFlags::empty();
    for unit in units {
        let (center, half_extents) = unit.simulation_bounds();
        bounds_min = bounds_min.min(center - half_extents);
        bounds_max = bounds_max.max(center + half_extents);
        obstruction_radius = obstruction_radius
            .max(planar_distance(position, unit.base.position) + unit.obstruction_radius());
        flags = merge_target_flags(flags, target_flags(unit));
    }
    Some(DetonateTargetSnapshot {
        position,
        bounds_min,
        bounds_max,
        obstruction_radius,
        player_id: first.base.player_id,
        proto_object_name: first.proto_object_name.clone(),
        flags,
    })
}

fn target_flags(unit: &Unit) -> AttackQueryFlags {
    let mut flags = AttackQueryFlags::empty();
    if unit.base.player_id == 0 {
        flags.insert(AttackQueryFlags::TARGET_GAIA);
    }
    if unit.hitpoints < unit.max_hitpoints {
        flags.insert(AttackQueryFlags::TARGET_DAMAGED);
    }
    if unit.is_building() && !unit.built {
        flags.insert(AttackQueryFlags::TARGET_UNBUILT);
    }
    flags
}

fn merge_target_flags(mut left: AttackQueryFlags, right: AttackQueryFlags) -> AttackQueryFlags {
    for flag in [
        AttackQueryFlags::TARGET_GAIA,
        AttackQueryFlags::TARGET_DAMAGED,
        AttackQueryFlags::TARGET_UNBUILT,
    ] {
        if right.contains(flag) {
            left.insert(flag);
        }
    }
    left
}

fn planar_distance(left: Vec3, right: Vec3) -> f32 {
    planar_length(left - right)
}

fn planar_length(vector: Vec3) -> f32 {
    vector.x.hypot(vector.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{RecoveryType, UnitDetonatePhase};
    use pipeline::database::hw1::tactics::{Action, TacticData, TacticState, Weapon};
    use pipeline::database::hw1::{Ability, Database, ProtoObject};

    #[test]
    fn contact_runs_state_zero_two_stage_explosion_and_recovery() {
        let gameplay = fixture();
        let mut world = World::new();
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let source_squad = world.create_squad_at(1, Vec3::ZERO);
        let source_unit = world.create_unit_at(1, Vec3::ZERO);
        configure_unit(&mut world, source_unit, "suicide", Vec3::splat(0.5));
        assert!(world.attach_unit_to_squad(source_unit, source_squad));
        let survivor = world.create_unit_at(1, Vec3::X * 5.0);
        configure_unit(&mut world, survivor, "suicide", Vec3::splat(0.5));
        assert!(world.attach_unit_to_squad(survivor, source_squad));
        let target_squad = world.create_squad_at(2, Vec3::ZERO);
        let target_unit = world.create_unit_at(2, Vec3::ZERO);
        configure_unit(&mut world, target_unit, "target", Vec3::splat(1.0));
        world
            .get_unit_mut(target_unit)
            .unwrap()
            .set_max_hitpoints(2_000.0);
        assert!(world.attach_unit_to_squad(target_unit, target_squad));

        assert!(world.issue_detonate_order(1, source_squad, target_squad, Some(0)));
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world
                .get_squad(source_squad)
                .unwrap()
                .detonate_action_name(),
            Some("SuicideBomb")
        );
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world.get_unit(source_unit).unwrap().tactic_state(),
            TacticStateId::from_index(0)
        );
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world.get_squad(source_squad).unwrap().detonate_phase(),
            SquadDetonatePhase::Attacking
        );
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world.get_unit(source_unit).unwrap().detonate_phase(),
            UnitDetonatePhase::Working
        );
        world.update_entities_with_gameplay(0.05, &gameplay);

        assert!(world.get_unit(source_unit).is_none());
        assert!(world.get_unit(survivor).is_some());
        let source = world.get_squad(source_squad).unwrap();
        assert_eq!(source.recovery.recovery_type(), Some(RecoveryType::Ability));
        assert_eq!(source.recovery.ability_id(), Some(1));
        assert_close(source.recovery.remaining(), 19.95);
        assert_close(world.get_unit(target_unit).unwrap().hitpoints, 600.0);
    }

    #[test]
    fn losing_glowy_range_disarms_and_clears_state_zero() {
        let gameplay = fixture();
        let mut world = World::new();
        world.init_players(2);
        let source_squad = world.create_squad_at(1, Vec3::ZERO);
        let source_unit = world.create_unit_at(1, Vec3::ZERO);
        configure_unit(&mut world, source_unit, "suicide", Vec3::splat(0.5));
        assert!(world.attach_unit_to_squad(source_unit, source_squad));
        let target = world.create_unit_at(2, Vec3::ZERO);
        configure_unit(&mut world, target, "target", Vec3::splat(1.0));
        assert!(world.issue_detonate_order(1, source_squad, target, None));
        world.update_entities_with_gameplay(0.05, &gameplay);
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.get_unit(source_unit).unwrap().is_detonate_armed());
        world.get_unit_mut(target).unwrap().base.position = Vec3::X * 100.0;
        world.update_entities_with_gameplay(0.05, &gameplay);
        let source = world.get_unit(source_unit).unwrap();
        assert!(!source.is_detonate_armed());
        assert_eq!(source.tactic_state(), None);
        assert_eq!(
            world.get_squad(source_squad).unwrap().detonate_phase(),
            SquadDetonatePhase::Inactive
        );
    }

    fn configure_unit(world: &mut World, unit_id: EntityId, proto: &str, extents: Vec3) {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = proto.to_owned();
        unit.obstruction_half_extents = extents;
    }

    fn fixture() -> GameplayCatalog {
        let mut database = Database::new();
        database.abilities.extend([
            Ability {
                name: "Command".to_owned(),
                ..Ability::default()
            },
            Ability {
                name: "SuicideAbility".to_owned(),
                recover_start: Some("Attack".to_owned()),
                recover_type: Some("Ability".to_owned()),
                recover_time: Some(20.0),
                ..Ability::default()
            },
        ]);
        database.objects.extend([
            ProtoObject {
                name: "suicide".to_owned(),
                tactics: Some("suicide.tactics".to_owned()),
                ability_command: Some("SuicideAbility".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "target".to_owned(),
                ..ProtoObject::default()
            },
        ]);
        let tactics = TacticData {
            weapons: vec![Weapon {
                name: "Bomb".to_owned(),
                damage_per_second: Some(1_400.0),
                weapon_type: Some("Basic".to_owned()),
                aoe_radius: Some(12.0),
                ..Weapon::default()
            }],
            states: vec![TacticState {
                run_anim: Some("SuicideRun".to_owned()),
                ..TacticState::default()
            }],
            actions: vec![Action {
                name: "SuicideBomb".to_owned(),
                action_type: Some("Detonate".to_owned()),
                weapon: Some("Bomb".to_owned()),
                ..Action::default()
            }],
            ..TacticData::default()
        };
        GameplayCatalog::from_tactics(&database, [("suicide".to_owned(), tactics)])
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }
}
