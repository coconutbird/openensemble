//! `PhysicsDetonateOnDeath` replacement creation and terminal physics state.

use super::World;
use crate::entity_id::EntityId;
use crate::gameplay::{DetonateThrowProfile, GameplayCatalog};
use crate::player::PlayerId;
use glam::Vec3;

impl World {
    pub(in crate::world) fn create_physics_detonate_replacement(
        &mut self,
        dead_unit_id: EntityId,
        instigator_player_id: PlayerId,
        gameplay: &GameplayCatalog,
    ) -> Option<EntityId> {
        let source = self.units.get(dead_unit_id)?;
        if source.is_physics_replacement() {
            return None;
        }
        let proto_object_name = source.proto_object_name.clone();
        let template = std::sync::Arc::clone(gameplay.physics_replacement(&proto_object_name)?);
        let detonate = gameplay.first_detonate_action(&proto_object_name)?;
        let replacement_id = self.spawn_physics_replacement(dead_unit_id, &template)?;
        if !self.begin_profile_detonate_action(
            replacement_id,
            &detonate,
            false,
            false,
            Some(instigator_player_id),
        ) {
            let _removed = self.remove_unit(replacement_id);
            return None;
        }
        Some(replacement_id)
    }

    pub(super) fn finish_physics_replacement_detonation(
        &mut self,
        unit_id: EntityId,
        tuning: DetonateThrowProfile,
    ) -> bool {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        if !unit.mark_physics_replacement_detonated() {
            return false;
        }
        let current_velocity = unit.base.velocity;
        let impulse = self.sample_detonate_throw(current_velocity, tuning);
        let _applied = self.apply_unit_impulse(unit_id, impulse);
        true
    }

    fn sample_detonate_throw(
        &mut self,
        current_velocity: Vec3,
        tuning: DetonateThrowProfile,
    ) -> Vec3 {
        let horizontal = tuning.horizontal_max();
        let vertical = tuning.vertical_max();
        let planar_velocity = Vec3::new(current_velocity.x, 0.0, current_velocity.z);
        if planar_velocity.length() > 0.1 {
            let direction = planar_velocity.normalize();
            let x = direction.x * self.sim_rng.range_float(0.5 * horizontal, horizontal);
            let z = direction.z * self.sim_rng.range_float(0.5 * horizontal, horizontal);
            let y = self.sim_rng.range_float(0.5 * vertical, vertical);
            Vec3::new(x, y, z)
        } else {
            Vec3::new(
                self.sim_rng.range_float(-horizontal, horizontal),
                self.sim_rng.range_float(0.5 * vertical, vertical),
                self.sim_rng.range_float(-horizontal, horizontal),
            )
        }
    }

    /// Removes each detonated replacement at the second cleanup that finds its body asleep. A
    /// unit without a body counts as asleep.
    pub(in crate::world) fn cleanup_physics_detonate_replacements(&mut self) {
        let detonated = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                unit.physics_replacement_is_detonated().then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in detonated {
            let resting = self.unit_body_is_active(unit_id) != Some(true);
            let removable = self
                .units
                .get_mut(unit_id)
                .is_some_and(|unit| unit.advance_physics_replacement_cleanup(resting));
            if removable {
                let _removed = self.remove_unit(unit_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_support::{TERRAIN_HEIGHT, box_template, world_with_flat_terrain};
    use crate::player::GAIA_PLAYER;
    use pipeline::database::hw1::tactics::{Action, ActionDuration, TacticData, Weapon};
    use pipeline::database::hw1::{Database, ProtoObject};

    #[test]
    fn lethal_damage_replaces_counts_down_and_protects_instigator_team() {
        let gameplay = fixture();
        let mut world = world_with_flat_terrain();
        world.get_player_mut(1).unwrap().team_id = 1;
        world.get_player_mut(2).unwrap().team_id = 2;
        world.configure_standard_team_relations();
        let ground = Vec3::new(50.0, TERRAIN_HEIGHT, 50.0);
        let barrel = configured_unit(&mut world, GAIA_PLAYER, ground, "barrel", 10.0);
        let ally = configured_unit(&mut world, 2, ground + Vec3::X * 2.0, "target", 100.0);

        let dealt = world.apply_weapon_damage(2, barrel, 10.0, None, Some(&gameplay));
        assert_eq!(dealt.to_bits(), 10.0_f32.to_bits());
        assert!(world.get_unit(barrel).is_none());
        let replacement_id = world
            .units
            .iter()
            .find_map(|(unit_id, unit)| unit.is_physics_replacement().then_some(unit_id))
            .expect("physics replacement");
        let replacement = world.get_unit(replacement_id).unwrap();
        assert!(world.unit_body(replacement_id).is_some());
        assert_eq!(replacement.hitpoints.to_bits(), 10.0_f32.to_bits());
        assert_eq!(
            replacement.detonate_phase(),
            crate::UnitDetonatePhase::Pending
        );

        for _ in 0..4 {
            world.update_entities_with_gameplay(0.05, &gameplay);
        }
        assert!(
            world
                .get_unit(replacement_id)
                .is_some_and(crate::entities::Unit::physics_replacement_is_detonated)
        );
        assert_eq!(
            world.get_unit(ally).unwrap().hitpoints.to_bits(),
            100.0_f32.to_bits()
        );
        // The replacement goes once its body rests.
        let removed_after = (1..=200).find(|_| {
            world.update_entities_with_gameplay(0.05, &gameplay);
            world.get_unit(replacement_id).is_none()
        });
        assert!(
            removed_after.is_some(),
            "the replacement body never came to rest"
        );
    }

    fn configured_unit(
        world: &mut World,
        player_id: PlayerId,
        position: Vec3,
        proto_object_name: &str,
        hitpoints: f32,
    ) -> EntityId {
        let unit_id = world.create_unit_at(player_id, position);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = proto_object_name.to_owned();
        unit.obstruction_half_extents = Vec3::splat(0.5);
        unit.set_max_hitpoints(hitpoints);
        unit_id
    }

    fn fixture() -> GameplayCatalog {
        let mut database = Database::new();
        database.objects.extend([
            ProtoObject {
                name: "barrel".to_owned(),
                tactics: Some("barrel.tactics".to_owned()),
                flags: vec!["PhysicsDetonateOnDeath".to_owned()],
                physics_replacement_info: Some("barrel_wreck".to_owned()),
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
                damage_per_second: Some(100.0),
                aoe_radius: Some(4.0),
                ..Weapon::default()
            }],
            actions: vec![Action {
                name: "DetonateDeath".to_owned(),
                action_type: Some("Detonate".to_owned()),
                weapon: Some("Bomb".to_owned()),
                duration: Some(ActionDuration {
                    seconds: 0.1,
                    ..ActionDuration::default()
                }),
                ..Action::default()
            }],
            ..TacticData::default()
        };
        let mut gameplay =
            GameplayCatalog::from_tactics(&database, [("barrel".to_owned(), tactics)]);
        gameplay.insert_test_physics_replacement(
            "barrel",
            box_template("barrel_wreck", Vec3::splat(0.5), 1.0, Vec3::Y * 0.5),
        );
        gameplay
    }
}
