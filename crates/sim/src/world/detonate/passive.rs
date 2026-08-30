//! Active unit Detonate triggers shared by suicide, bomb, and physics paths.

use super::{DetonateExplosion, World};
use crate::entities::UnitDetonatePhase;
use crate::entities::units::{UnitDetonateTriggerConfig, UnitDetonation};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{AbilityRecoveryStart, DetonateActionProfile, GameplayCatalog};
use crate::physics::UnitCollisionContact;
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::world::combat::AttackDamage;
use glam::Vec3;
use num_traits::ToPrimitive;

impl World {
    pub(super) fn activate_persistent_unit_detonations(&mut self, gameplay: &GameplayCatalog) {
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            let profile = self.units.get(unit_id).and_then(|unit| {
                (unit.is_alive()
                    && !unit.is_detonate_armed()
                    && unit.detonate_phase() == UnitDetonatePhase::Inactive)
                    .then(|| gameplay.first_persistent_detonate_action(&unit.proto_object_name))
                    .flatten()
            });
            let Some(profile) = profile else {
                continue;
            };
            if self.persistent_detonate_action_enabled(unit_id, &profile) {
                let _started =
                    self.begin_profile_detonate_action(unit_id, &profile, false, false, None);
            }
        }
    }

    fn persistent_detonate_action_enabled(
        &self,
        unit_id: EntityId,
        profile: &DetonateActionProfile,
    ) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let authored_enabled = !profile.starts_disabled();
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    )
                });
        unit.actions
            .is_enabled(profile.action_name(), !player_enabled)
    }

    /// Create an action that evaluates the triggers authored on `action_name`.
    ///
    /// Bomb and physical-replacement systems use this entry point after they
    /// create their owner. It intentionally does not make the action immediate.
    pub fn activate_unit_detonate_action(
        &mut self,
        unit_id: EntityId,
        action_name: &str,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let Some(profile) = gameplay.detonate_action(&unit.proto_object_name, action_name) else {
            return false;
        };
        self.begin_profile_detonate_action(unit_id, &profile, false, false, None)
    }

    pub(super) fn begin_profile_detonate_action(
        &mut self,
        unit_id: EntityId,
        profile: &DetonateActionProfile,
        immediate: bool,
        death_override: bool,
        instigator_player_id: Option<PlayerId>,
    ) -> bool {
        if self
            .units
            .get(unit_id)
            .is_none_or(|unit| unit.detonate_phase() != UnitDetonatePhase::Inactive)
        {
            return false;
        }
        let config =
            self.detonate_trigger_config(profile, immediate, death_override, instigator_player_id);
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        if unit
            .detonate_action_name()
            .is_none_or(|name| !name.eq_ignore_ascii_case(profile.action_name()))
        {
            unit.arm_detonate(profile.action_name(), 1.0);
        }
        unit.begin_detonate_action(config)
    }

    pub(in crate::world) fn force_active_unit_detonation(
        &mut self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(detonation) = self
            .units
            .get_mut(unit_id)
            .and_then(crate::entities::Unit::force_detonate_action)
        else {
            return false;
        };
        let Some(explosion) = self.detonate_explosion(unit_id, &detonation, gameplay) else {
            return false;
        };
        self.resolve_detonate_explosion(&explosion, gameplay);
        true
    }

    fn detonate_trigger_config(
        &mut self,
        profile: &DetonateActionProfile,
        immediate: bool,
        death_override: bool,
        instigator_player_id: Option<PlayerId>,
    ) -> UnitDetonateTriggerConfig {
        UnitDetonateTriggerConfig {
            immediate,
            on_death: death_override || profile.has_death_trigger(),
            countdown_ms: profile
                .duration()
                .map(|duration| self.sample_detonate_countdown(duration)),
            proximity_radius: profile
                .has_proximity_trigger()
                .then_some(profile.proximity_radius()),
            physics_threshold: profile.physics_trigger_threshold(),
            instigator_player_id,
        }
    }

    fn sample_detonate_countdown(
        &mut self,
        duration: crate::gameplay::DetonateDurationProfile,
    ) -> u32 {
        let mut seconds = duration.seconds();
        if duration.spread() > 0.0 {
            seconds += self
                .sim_rng
                .range_float(-duration.spread(), duration.spread());
        }
        (finite_nonnegative(seconds) * 1_000.0)
            .to_u32()
            .unwrap_or(u32::MAX)
    }

    pub(super) fn advance_active_unit_detonations(
        &mut self,
        elapsed_seconds: f32,
        gameplay: &GameplayCatalog,
    ) {
        let Some(elapsed_ms) = elapsed_milliseconds(elapsed_seconds) else {
            return;
        };
        let unit_ids = self.active_detonate_unit_ids();
        let explosions = unit_ids
            .into_iter()
            .filter_map(|unit_id| self.advance_unit_detonate(unit_id, elapsed_ms, gameplay))
            .collect::<Vec<_>>();
        for explosion in explosions {
            self.resolve_detonate_explosion(&explosion, gameplay);
        }
    }

    fn active_detonate_unit_ids(&self) -> Vec<EntityId> {
        self.units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (unit.detonate_phase() != UnitDetonatePhase::Inactive).then_some(unit_id)
            })
            .collect()
    }

    fn advance_unit_detonate(
        &mut self,
        unit_id: EntityId,
        elapsed_ms: u32,
        gameplay: &GameplayCatalog,
    ) -> Option<DetonateExplosion> {
        let enemy_in_proximity = self
            .units
            .get(unit_id)
            .and_then(crate::entities::Unit::detonate_proximity_radius)
            .is_some_and(|radius| self.enemy_in_detonate_proximity(unit_id, radius, gameplay));
        let detonation = self
            .units
            .get_mut(unit_id)?
            .advance_detonate_action(elapsed_ms, enemy_in_proximity)?;
        self.detonate_explosion(unit_id, &detonation, gameplay)
    }

    fn detonate_explosion(
        &self,
        unit_id: EntityId,
        detonation: &UnitDetonation,
        gameplay: &GameplayCatalog,
    ) -> Option<DetonateExplosion> {
        let unit = self.units.get(unit_id)?;
        let profile = gameplay.detonate_action(&unit.proto_object_name, &detonation.action_name)?;
        Some(DetonateExplosion {
            unit_id,
            squad_id: unit.squad_id,
            source_player_id: unit.base.player_id,
            instigator_player_id: detonation
                .instigator_player_id
                .unwrap_or(unit.base.player_id),
            position: unit.base.position,
            proto_object_name: unit.proto_object_name.clone(),
            profile,
            damage_multiplier: unit.effective_damage_multiplier(),
        })
    }

    fn resolve_detonate_explosion(
        &mut self,
        explosion: &DetonateExplosion,
        gameplay: &GameplayCatalog,
    ) {
        let authored_damage = self.get_player(explosion.source_player_id).map_or(
            explosion.profile.damage_per_second(),
            |player| {
                player.technologies.weapon_damage(
                    &explosion.proto_object_name,
                    explosion.profile.weapon_name(),
                    explosion.profile.damage_per_second(),
                )
            },
        );
        let _dealt = self.apply_attack_damage(
            &AttackDamage {
                attacker_id: explosion.unit_id,
                attacker_player_id: explosion.instigator_player_id,
                primary_target_id: None,
                ground_zero: explosion.position,
                direction: Vec3::ZERO,
                damage: finite_nonnegative(authored_damage * explosion.damage_multiplier),
                weapon_type: explosion.profile.weapon_type().map(str::to_owned),
                area_damage: explosion.profile.area_damage(),
            },
            Some(gameplay),
        );
        if let Some(squad_id) = explosion.squad_id {
            self.start_detonate_recovery(squad_id, &explosion.proto_object_name, gameplay);
        }
        if self.finish_physics_replacement_detonation(
            explosion.unit_id,
            explosion.profile.detonate_throw(),
        ) {
            return;
        }
        let _killed = self.kill_unit(explosion.unit_id, false);
    }

    fn start_detonate_recovery(
        &mut self,
        squad_id: EntityId,
        proto_object_name: &str,
        gameplay: &GameplayCatalog,
    ) {
        let Some((player_id, order, already_started)) = self.squads.get(squad_id).map(|squad| {
            (
                squad.base.player_id,
                squad.detonate.order(),
                squad.detonate.recovery_started(),
            )
        }) else {
            return;
        };
        if already_started {
            return;
        }
        let recovery = order
            .and_then(|order| order.requested_ability_id)
            .and_then(|requested| gameplay.resolve_order_ability(proto_object_name, requested))
            .map(|ability| self.detonate_ability_recovery(player_id, ability));
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.detonate.mark_recovery_started();
            if let Some((kind, time, ability_id)) = recovery {
                squad.finish_ability_execution(kind, time, ability_id);
            }
        }
    }

    fn detonate_ability_recovery(
        &self,
        player_id: PlayerId,
        ability: &crate::gameplay::AbilityGameplay,
    ) -> (Option<crate::entities::RecoveryType>, f32, Option<u8>) {
        let time = self
            .get_player(player_id)
            .map_or(ability.recovery_time(), |player| {
                player
                    .technologies
                    .ability_recovery_time(ability.name(), ability.recovery_time())
            });
        let kind = (ability.recovery_start() == Some(AbilityRecoveryStart::Attack))
            .then(|| ability.recovery_type())
            .flatten();
        (kind, time, Some(ability.database_id()))
    }

    pub(in crate::world) fn resolve_dead_unit_detonations(&mut self, gameplay: &GameplayCatalog) {
        loop {
            let dead_ids = self
                .units
                .iter()
                .filter_map(|(unit_id, unit)| {
                    (unit.hitpoints <= 0.0
                        && !unit.is_static_death_replacement()
                        && unit.detonate_phase() != UnitDetonatePhase::Inactive)
                        .then_some(unit_id)
                })
                .collect::<Vec<_>>();
            let explosions = dead_ids
                .into_iter()
                .filter_map(|unit_id| self.dead_unit_detonation(unit_id, gameplay))
                .collect::<Vec<_>>();
            if explosions.is_empty() {
                break;
            }
            for explosion in explosions {
                self.resolve_detonate_explosion(&explosion, gameplay);
            }
        }
    }

    fn dead_unit_detonation(
        &mut self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<DetonateExplosion> {
        let detonation = self.units.get_mut(unit_id)?.notify_detonate_death()?;
        self.detonate_explosion(unit_id, &detonation, gameplay)
    }

    pub(in crate::world) fn activate_physics_detonation_contacts(
        &mut self,
        contacts: &[UnitCollisionContact],
    ) {
        for contact in contacts {
            for unit_id in [contact.first, contact.second] {
                if let Some(unit) = self.units.get_mut(unit_id) {
                    unit.notify_detonate_physics_collision(contact.projected_velocity);
                }
            }
        }
        for (_, unit) in self.units.iter_mut() {
            let impact_speed = unit.physics.as_ref().map_or(
                0.0,
                crate::physics::PhysicsBody::ground_impact_speed_this_step,
            );
            unit.notify_detonate_physics_collision(impact_speed);
        }
    }

    fn enemy_in_detonate_proximity(
        &self,
        source_id: EntityId,
        proximity_radius: f32,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let Some(source) = self.units.get(source_id) else {
            return false;
        };
        let source_radius = source.obstruction_radius();
        let source_tied_to_ground = !source.flying
            && source
                .physics
                .as_ref()
                .is_none_or(crate::physics::PhysicsBody::is_grounded);
        self.units.iter().any(|(target_id, target)| {
            target_id != source_id
                && target.is_alive()
                && !target.flying
                && !target.is_garrisoned()
                && target.base.player_id != GAIA_PLAYER
                && self.players_are_enemies(target.base.player_id, source.base.player_id)
                && !gameplay.object_is_neutral(&target.proto_object_name)
                && source.base.position.distance(target.base.position)
                    <= proximity_radius + source_radius + target.obstruction_radius()
                && (source_tied_to_ground
                    || source.base.position.y - target.base.position.y <= proximity_radius)
        })
    }
}

fn elapsed_milliseconds(elapsed_seconds: f32) -> Option<u32> {
    if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
        return None;
    }
    (elapsed_seconds * 1_000.0).round_ties_even().to_u32()
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::UnitState;
    use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
    use pipeline::database::hw1::tactics::{
        Action, ActionDuration, PhysicsDetonation, TacticData, Weapon,
    };
    use pipeline::database::hw1::{Database, ProtoObject};

    #[test]
    fn countdown_waits_one_action_update_and_uses_strict_milliseconds() {
        let action = detonate_action();
        let action = Action {
            duration: Some(ActionDuration {
                seconds: 0.1,
                ..ActionDuration::default()
            }),
            ..action
        };
        let gameplay = fixture(action, 50.0, 10.0, 0.0);
        let mut world = configured_world();
        let source = configured_unit(&mut world, 1, Vec3::ZERO, "bomb");
        let target = configured_unit(&mut world, 2, Vec3::X * 2.0, "target");
        assert!(world.activate_unit_detonate_action(source, "Detonate", &gameplay));

        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world
                .get_unit(source)
                .unwrap()
                .detonate_countdown_remaining_ms(),
            Some(100)
        );
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world
                .get_unit(source)
                .unwrap()
                .detonate_countdown_remaining_ms(),
            Some(50)
        );
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert_eq!(
            world
                .get_unit(source)
                .unwrap()
                .detonate_countdown_remaining_ms(),
            Some(0)
        );
        assert_eq!(
            world.get_unit(target).unwrap().hitpoints.to_bits(),
            100.0_f32.to_bits()
        );
        world.update_entities_with_gameplay(0.05, &gameplay);

        assert!(world.get_unit(source).is_none());
        assert_eq!(
            world.get_unit(target).unwrap().hitpoints.to_bits(),
            50.0_f32.to_bits()
        );
    }

    #[test]
    fn proximity_rejects_flying_and_neutral_targets_before_enemy() {
        let action = Action {
            detonate_when_in_range: Some(true),
            ..detonate_action()
        };
        let gameplay = fixture(action, 1.0, 1.0, 2.0);
        let mut world = configured_world();
        let source = configured_unit(&mut world, 1, Vec3::ZERO, "bomb");
        let target = configured_unit(&mut world, 2, Vec3::X * 3.0, "target");
        world.get_unit_mut(target).unwrap().flying = true;
        assert!(world.activate_unit_detonate_action(source, "Detonate", &gameplay));

        world.update_entities_with_gameplay(0.05, &gameplay);
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.get_unit(source).is_some());
        let target_unit = world.get_unit_mut(target).unwrap();
        target_unit.flying = false;
        target_unit.proto_object_name = "neutral_target".to_owned();
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.get_unit(source).is_some());
        world.get_unit_mut(target).unwrap().proto_object_name = "target".to_owned();
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.get_unit(source).is_none());
    }

    #[test]
    fn death_notifications_cascade_with_instigator_team_attribution() {
        let action = Action {
            detonate_on_death: Some(true),
            ..detonate_action()
        };
        let gameplay = fixture(action, 1_000.0, 4.0, 0.0);
        let mut world = configured_world();
        let first = configured_unit(&mut world, GAIA_PLAYER, Vec3::ZERO, "bomb");
        let second = configured_unit(&mut world, GAIA_PLAYER, Vec3::X * 3.0, "bomb");
        let instigator_ally = configured_unit(&mut world, 2, Vec3::X * 5.0, "target");
        let profile = gameplay.detonate_action("bomb", "Detonate").unwrap();
        for unit_id in [first, second] {
            assert!(world.begin_profile_detonate_action(unit_id, &profile, false, false, Some(2),));
        }
        assert!(world.kill_unit(first, false));
        world.update_entities_with_gameplay(0.05, &gameplay);

        assert!(world.get_unit(first).is_none());
        assert!(
            world.get_unit(second).is_none(),
            "second state: {:?}",
            world.get_unit(second).map(|unit| (
                unit.hitpoints,
                unit.detonate_phase(),
                unit.detonate_action_name().map(str::to_owned)
            ))
        );
        assert_eq!(
            world.get_unit(instigator_ally).unwrap().hitpoints.to_bits(),
            100.0_f32.to_bits()
        );
    }

    #[test]
    fn body_and_ground_contacts_activate_physics_detonation() {
        let action = Action {
            detonate_from_physics: Some(PhysicsDetonation {
                threshold: Some(2.0),
            }),
            ..detonate_action()
        };
        let gameplay = fixture(action, 0.0, 0.0, 0.0);
        let mut world = configured_world();
        let body_source = configured_unit(&mut world, 1, Vec3::ZERO, "bomb");
        configure_dynamic_body(&mut world, body_source, 0.0);
        let obstruction = configured_unit(&mut world, 2, Vec3::X * 1.5, "target");
        world.get_unit_mut(obstruction).unwrap().physics = Some(PhysicsBody::static_obstruction(
            BoxCollider::new(Vec3::splat(1.0), Vec3::ZERO),
        ));
        {
            let source = world.get_unit_mut(body_source).unwrap();
            source.state = UnitState::Moving;
            source.move_target = Some(Vec3::X * 100.0);
            source.base.velocity = Vec3::X * 5.0;
        }
        assert!(world.activate_unit_detonate_action(body_source, "Detonate", &gameplay));
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(
            world
                .get_unit(body_source)
                .unwrap()
                .detonate_physics_trigger_activated()
        );
        world.get_unit_mut(body_source).unwrap().stop();
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(
            world.get_unit(body_source).is_none(),
            "body source state: {:?}",
            world.get_unit(body_source).map(|unit| (
                unit.base.velocity,
                unit.detonate_phase(),
                unit.detonate_physics_trigger_activated()
            ))
        );

        let ground_source = configured_unit(&mut world, 1, Vec3::Y * 0.1, "bomb");
        configure_dynamic_body(&mut world, ground_source, 0.0);
        {
            let source = world.get_unit_mut(ground_source).unwrap();
            assert!(source.apply_impulse(Vec3::Y));
            source.base.velocity = Vec3::NEG_Y * 5.0;
        }
        assert!(world.activate_unit_detonate_action(ground_source, "Detonate", &gameplay));
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(
            world
                .get_unit(ground_source)
                .unwrap()
                .detonate_physics_trigger_activated()
        );
        world.update_entities_with_gameplay(0.05, &gameplay);
        assert!(world.get_unit(ground_source).is_none());
    }

    #[test]
    fn countdown_spread_consumes_the_synchronized_random_stream() {
        let action = Action {
            duration: Some(ActionDuration {
                seconds: 1.0,
                spread: Some(0.25),
            }),
            ..detonate_action()
        };
        let gameplay = fixture(action, 0.0, 0.0, 0.0);
        let mut first = configured_world_with_seed(73);
        let mut second = configured_world_with_seed(73);
        let first_id = configured_unit(&mut first, 1, Vec3::ZERO, "bomb");
        let second_id = configured_unit(&mut second, 1, Vec3::ZERO, "bomb");
        assert!(first.activate_unit_detonate_action(first_id, "Detonate", &gameplay));
        assert!(second.activate_unit_detonate_action(second_id, "Detonate", &gameplay));

        let first_remaining = first
            .get_unit(first_id)
            .unwrap()
            .detonate_countdown_remaining_ms()
            .unwrap();
        assert!((750..=1_250).contains(&first_remaining));
        assert_eq!(
            first_remaining,
            second
                .get_unit(second_id)
                .unwrap()
                .detonate_countdown_remaining_ms()
                .unwrap()
        );
        assert_eq!(first.checksum_with_rng(), second.checksum_with_rng());
    }

    fn configured_world() -> World {
        configured_world_with_seed(1)
    }

    fn configured_world_with_seed(seed: u64) -> World {
        let mut world = World::with_seed(seed);
        world.init_players(2);
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        world
    }

    fn configured_unit(
        world: &mut World,
        player_id: PlayerId,
        position: Vec3,
        proto_object_name: &str,
    ) -> EntityId {
        let unit_id = world.create_unit_at(player_id, position);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = proto_object_name.to_owned();
        unit.obstruction_half_extents = Vec3::splat(0.5);
        unit.set_max_hitpoints(100.0);
        unit_id
    }

    fn configure_dynamic_body(world: &mut World, unit_id: EntityId, ground_height: f32) {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.physics = Some(PhysicsBody::ground_vehicle(
            PhysicsMaterial {
                mass: 1.0,
                friction: 0.0,
                restitution: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
            },
            BoxCollider::new(Vec3::splat(1.0), Vec3::ZERO),
            ground_height,
            10.0,
            100.0,
            360.0,
        ));
    }

    fn fixture(action: Action, damage: f32, aoe_radius: f32, max_range: f32) -> GameplayCatalog {
        let mut database = Database::new();
        database.objects.extend([
            ProtoObject {
                name: "bomb".to_owned(),
                tactics: Some("bomb.tactics".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "target".to_owned(),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "neutral_target".to_owned(),
                flags: vec!["Neutral".to_owned()],
                ..ProtoObject::default()
            },
        ]);
        let tactics = TacticData {
            actions: vec![action],
            weapons: vec![Weapon {
                name: "Bomb".to_owned(),
                damage_per_second: Some(damage),
                aoe_radius: Some(aoe_radius),
                max_range: Some(max_range),
                ..Weapon::default()
            }],
            ..TacticData::default()
        };
        GameplayCatalog::from_tactics(&database, [("bomb".to_owned(), tactics)])
    }

    fn detonate_action() -> Action {
        Action {
            name: "Detonate".to_owned(),
            action_type: Some("Detonate".to_owned()),
            weapon: Some("Bomb".to_owned()),
            ..Action::default()
        }
    }
}
