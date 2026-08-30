//! Legacy trigger-driven `BSquadActionCarpetBomb` orchestration.

use super::World;
use super::combat::PositionAttackStatus;
use crate::entities::squads::{CarpetBombOrder, SquadCarpetBombPhase};
use crate::entities::{SquadState, Unit};
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use glam::Vec3;

const WORKING_LEAD_RANGE: f32 = 40.0;
const RETURNING_LEAD_RANGE: f32 = 100.0;

impl World {
    /// Start retail's legacy squad carpet-bomb action.
    pub fn issue_squad_carpet_bomb(
        &mut self,
        squad_id: EntityId,
        target_position: Vec3,
        launch_position: Option<Vec3>,
        attack_run_distance: f32,
        attack_count: i32,
    ) -> bool {
        self.squads.get_mut(squad_id).is_some_and(|squad| {
            squad.begin_carpet_bomb_order(CarpetBombOrder {
                target_position,
                launch_position,
                attack_run_distance,
                attack_count,
            })
        })
    }

    pub(crate) fn prepare_squad_carpet_bombs(&mut self) {
        let squad_ids = self
            .squads
            .iter()
            .filter_map(|(id, squad)| squad.is_carpet_bombing().then_some(id))
            .collect::<Vec<_>>();
        for squad_id in squad_ids {
            self.prepare_squad_carpet_bomb(squad_id);
            self.position_carpet_bomb_squad(squad_id);
        }
    }

    fn prepare_squad_carpet_bomb(&mut self, squad_id: EntityId) {
        let phase = self
            .squads
            .get(squad_id)
            .map_or(SquadCarpetBombPhase::Inactive, |squad| {
                squad.carpet_bomb.phase()
            });
        match phase {
            SquadCarpetBombPhase::Preparing => {
                let Some((prepared, unit_ids)) = self
                    .squads
                    .get_mut(squad_id)
                    .map(|squad| (squad.carpet_bomb.prepare(), squad.unit_ids.clone()))
                else {
                    return;
                };
                if !prepared {
                    if let Some(squad) = self.squads.get_mut(squad_id) {
                        squad.finish_carpet_bomb_order();
                    }
                    return;
                }
                for unit_id in unit_ids {
                    if let Some(unit) = self.units.get_mut(unit_id) {
                        unit.request_move_air_launch();
                    }
                }
            }
            SquadCarpetBombPhase::Working => {
                let exhausted = self.squads.get_mut(squad_id).is_some_and(|squad| {
                    !squad.carpet_bomb.activate_next_location()
                        && !squad.carpet_bomb.has_remaining_locations()
                });
                if exhausted && self.carpet_bomb_return_threshold_met(squad_id) {
                    self.begin_carpet_bomb_return(squad_id);
                }
            }
            SquadCarpetBombPhase::Returning => self.request_carpet_bomb_return(squad_id),
            SquadCarpetBombPhase::Inactive => {}
        }
    }

    fn carpet_bomb_return_threshold_met(&self, squad_id: EntityId) -> bool {
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        let Some(target) = squad.carpet_bomb_target() else {
            return false;
        };
        let Some(lead) = squad
            .unit_ids
            .first()
            .and_then(|unit_id| self.units.get(*unit_id))
        else {
            return false;
        };
        planar_distance(lead.base.position, squad.base.position) < 10.0
            && planar_distance(squad.base.position, target) < 10.0
    }

    fn begin_carpet_bomb_return(&mut self, squad_id: EntityId) {
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.carpet_bomb.enter_returning();
            squad.state = SquadState::Working;
            squad.move_target = None;
            squad.base.velocity = Vec3::ZERO;
        }
        self.request_carpet_bomb_return(squad_id);
    }

    fn request_carpet_bomb_return(&mut self, squad_id: EntityId) {
        let unit_ids = self
            .squads
            .get(squad_id)
            .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.request_move_air_return();
            }
        }
    }

    fn position_carpet_bomb_squad(&mut self, squad_id: EntityId) {
        let Some((phase, target, first_unit_id)) = self.squads.get(squad_id).and_then(|squad| {
            let phase = squad.carpet_bomb.phase();
            let target = match phase {
                SquadCarpetBombPhase::Working => squad.carpet_bomb_target(),
                SquadCarpetBombPhase::Returning => squad
                    .unit_ids
                    .first()
                    .and_then(|unit_id| self.units.get(*unit_id))
                    .and_then(Unit::move_air_base_position),
                SquadCarpetBombPhase::Inactive | SquadCarpetBombPhase::Preparing => None,
            }?;
            Some((phase, target, squad.unit_ids.first().copied()))
        }) else {
            return;
        };
        let lead = first_unit_id.and_then(|unit_id| self.units.get(unit_id));
        let Some(lead) = lead else {
            return;
        };
        if !lead.uses_move_air() {
            if phase == SquadCarpetBombPhase::Working
                && let Some(squad) = self.squads.get_mut(squad_id)
            {
                squad.state = SquadState::Attacking;
                squad.move_target = squad.carpet_bomb_attack_position();
            }
            return;
        }
        let lead_range = if phase == SquadCarpetBombPhase::Returning {
            RETURNING_LEAD_RANGE
        } else {
            WORKING_LEAD_RANGE
        };
        let offset = Vec3::new(
            target.x - lead.base.position.x,
            0.0,
            target.z - lead.base.position.z,
        );
        let mut squad_position = if offset.length() > lead_range {
            lead.base.position + offset.normalize_or_zero() * lead_range
        } else {
            target
        };
        squad_position.y = target.y;
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.base.position = squad_position;
            squad.base.velocity = Vec3::ZERO;
            squad.move_target = None;
            squad.state = if phase == SquadCarpetBombPhase::Working {
                SquadState::Attacking
            } else {
                SquadState::Working
            };
        }
    }

    pub(in crate::world) fn update_squad_carpet_bomb_attacks(
        &mut self,
        dt: f32,
        gameplay: &GameplayCatalog,
    ) {
        let attacks = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                (squad.carpet_bomb.phase() == SquadCarpetBombPhase::Working).then_some((
                    squad_id,
                    squad.carpet_bomb.active_location()?,
                    squad.unit_ids.clone(),
                ))
            })
            .collect::<Vec<_>>();
        for (squad_id, target, unit_ids) in attacks {
            self.advance_carpet_bomb_attack(dt, squad_id, target, &unit_ids, gameplay);
        }
    }

    fn advance_carpet_bomb_attack(
        &mut self,
        dt: f32,
        squad_id: EntityId,
        target: Vec3,
        unit_ids: &[EntityId],
        gameplay: &GameplayCatalog,
    ) {
        let participants = unit_ids
            .iter()
            .copied()
            .filter(|unit_id| self.units.get(*unit_id).is_some_and(Unit::is_operational))
            .collect::<Vec<_>>();
        for &unit_id in &participants {
            let already_complete = self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.carpet_bomb.unit_complete(unit_id));
            if already_complete {
                continue;
            }
            let status = self.advance_position_attack(dt, unit_id, target, gameplay);
            if matches!(
                status,
                PositionAttackStatus::Completed | PositionAttackStatus::Unavailable
            ) && let Some(squad) = self.squads.get_mut(squad_id)
            {
                squad.carpet_bomb.mark_unit_complete(unit_id);
            }
        }
        let complete = participants.is_empty()
            || self
                .squads
                .get(squad_id)
                .is_some_and(|squad| squad.carpet_bomb.all_units_complete(&participants));
        if complete {
            self.stop_unit_firing(&participants);
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.carpet_bomb.complete_active_location();
                squad.state = SquadState::Working;
            }
        }
    }
}

fn planar_distance(left: Vec3, right: Vec3) -> f32 {
    Vec3::new(left.x - right.x, 0.0, left.z - right.z).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{FlightControllerKind, SquadCarpetBombPhase};
    use crate::gameplay::{
        AreaDamageProfile, AttackAccuracyProfile, AttackAmmunition, AttackAnimation, AttackProfile,
        GameplayCatalog, ProjectileReactionFlags,
    };
    use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
    use pipeline::database::hw1::{Database, ProtoObject};

    const STEP: f32 = 0.05;

    #[test]
    fn position_run_uses_shared_combat_damage_and_presentation_state() {
        let (mut world, database, gameplay, squad_id, unit_id, victim_id) = carpet_world(false);
        assert!(world.issue_squad_carpet_bomb(squad_id, Vec3::ZERO, Some(Vec3::ZERO), 0.0, 1,));

        world.update_entities_with_database_and_gameplay(STEP, &database, &gameplay);
        let attacker = world.get_unit(unit_id).unwrap();
        assert_eq!(attacker.combat.action_name(), Some("BombGround"));
        assert!(attacker.combat.is_animating());
        assert!(world.get_unit(victim_id).unwrap().hitpoints < 100.0);

        for _ in 0..3 {
            world.update_entities_with_database_and_gameplay(STEP, &database, &gameplay);
        }
        assert_eq!(
            world.get_squad(squad_id).unwrap().carpet_bomb_phase(),
            SquadCarpetBombPhase::Returning
        );
        assert!(world.get_squad(squad_id).unwrap().ignores_leash());
    }

    #[test]
    fn move_air_launch_and_return_are_deterministic_and_checksummed() {
        let (mut first, database, gameplay, squad_id, unit_id, _) = carpet_world(true);
        let (mut second, _, _, second_squad_id, _, _) = carpet_world(true);
        for (world, squad) in [(&mut first, squad_id), (&mut second, second_squad_id)] {
            assert!(world.issue_squad_carpet_bomb(squad, Vec3::ZERO, Some(Vec3::ZERO), 0.0, 1,));
        }

        for _ in 0..80 {
            first.update_entities_with_database_and_gameplay(STEP, &database, &gameplay);
            second.update_entities_with_database_and_gameplay(STEP, &database, &gameplay);
        }
        assert_eq!(first.checksum(), second.checksum());
        assert_eq!(
            first.get_squad(squad_id).unwrap().carpet_bomb_phase(),
            SquadCarpetBombPhase::Returning
        );
        assert!(
            first
                .get_unit(unit_id)
                .unwrap()
                .is_move_air_returning_to_base()
        );
    }

    fn carpet_world(
        move_air: bool,
    ) -> (
        World,
        Database,
        GameplayCatalog,
        EntityId,
        EntityId,
        EntityId,
    ) {
        let mut database = Database::new();
        database.objects.extend([
            ProtoObject {
                name: "bomber".to_owned(),
                tactics: Some("bomber.tactics".to_owned()),
                movement_type: move_air.then(|| "Air".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "victim".to_owned(),
                hitpoints: Some(100.0),
                ..ProtoObject::default()
            },
        ]);
        let tactics = TacticData {
            weapons: vec![Weapon {
                name: "Bomb".to_owned(),
                max_range: Some(20.0),
                ..Weapon::default()
            }],
            actions: vec![Action {
                name: "BombGround".to_owned(),
                action_type: Some("RangedAttack".to_owned()),
                weapon: Some("Bomb".to_owned()),
                ..Action::default()
            }],
            ..TacticData::default()
        };
        let gameplay = GameplayCatalog::from_test_profiles(
            &database,
            [("bomber".to_owned(), tactics)],
            [("bomber".to_owned(), bomb_profile())],
        );
        let mut world = World::new();
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let unit_id = world.create_unit_at(1, Vec3::ZERO);
        world.get_unit_mut(unit_id).unwrap().proto_object_name = "bomber".to_owned();
        if move_air {
            world
                .get_unit_mut(unit_id)
                .unwrap()
                .configure_flight_controller(FlightControllerKind::MoveAir, 0.0);
        }
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        let victim_id = world.create_unit_at(2, Vec3::ZERO);
        let victim = world.get_unit_mut(victim_id).unwrap();
        victim.proto_object_name = "victim".to_owned();
        victim.hitpoints = 100.0;
        victim.max_hitpoints = 100.0;
        if move_air {
            world.update_entities(STEP);
            world.update_entities(STEP);
        }
        (world, database, gameplay, squad_id, unit_id, victim_id)
    }

    fn bomb_profile() -> AttackProfile {
        AttackProfile {
            action_name: "BombGround".to_owned(),
            animation_type: "Attack".to_owned(),
            weapon_name: "Bomb".to_owned(),
            weapon_type: None,
            projectile: None,
            impact_effect: None,
            area_damage: Some(AreaDamageProfile {
                radius: 4.0,
                primary_target_factor: 0.0,
                distance_factor: 0.0,
                damage_factor: 0.0,
                linear_damage: false,
                ignores_y_axis: false,
                friendly_fire: false,
            }),
            pull: None,
            hardpoint: None,
            orientation: crate::gameplay::AttackOrientationProfile::default(),
            charged_animation: None,
            friendly_fire: false,
            targets_foot_of_unit: false,
            projectile_reactions: ProjectileReactionFlags::default(),
            max_range: 20.0,
            max_velocity_lead: 0.0,
            accuracy: AttackAccuracyProfile::default(),
            damage_per_attack: 10.0,
            ammunition: AttackAmmunition::None,
            animations: vec![AttackAnimation {
                asset_path: "bomb.uax".to_owned(),
                weight: 1,
                duration: 0.1,
                attack_positions: vec![0.0],
                events: Vec::new(),
                hardpoint_track: None,
            }],
            pre_attack_cooldown: [0.0, 0.0],
            post_attack_cooldown: [0.0, 0.0],
            reload_duration: 0.0,
            visual_ammo: 0,
            uses_height_bonus_damage: false,
        }
    }
}
