//! Source-backed base protection from completed child buildings.

use super::World;
use crate::entities::BaseId;
use crate::entity::Entity;
use crate::entity_id::EntityId;

impl World {
    pub(in crate::world) fn recompute_unit_base_child_damage(&mut self, unit_id: EntityId) {
        if let Some(base_id) = self.get_unit(unit_id).and_then(|unit| unit.base_id) {
            self.recompute_base_child_damage(base_id);
        }
    }

    pub(in crate::world) fn recompute_base_child_damage(&mut self, base_id: BaseId) {
        let Some((anchor_id, building_ids)) = self.bases.get(&base_id).map(|base| {
            (
                base.anchor_building_id,
                base.buildings().collect::<Vec<_>>(),
            )
        }) else {
            return;
        };
        let recipient_id = self.base_child_damage_recipient(anchor_id, &building_ids);
        let count = recipient_id.map_or(0, |recipient_id| {
            building_ids
                .iter()
                .filter(|&&building_id| building_id != recipient_id)
                .filter(|&&building_id| {
                    self.units.get(building_id).is_some_and(|building| {
                        building.is_alive()
                            && building.built
                            && building.contributes_child_damage_protection()
                    })
                })
                .count()
        });
        for &building_id in &building_ids {
            if let Some(building) = self.units.get_mut(building_id) {
                building.set_child_object_damage_taken_multiplier(1.0);
            }
        }
        let Some(recipient_id) = recipient_id else {
            return;
        };
        let scalar = self
            .units
            .get(recipient_id)
            .map_or(0.0, crate::entities::Unit::child_damage_base_scalar);
        let count = u16::try_from(count).unwrap_or(u16::MAX);
        let multiplier = if count == 0 {
            1.0
        } else {
            1.0 / ((f32::from(count) + 1.0) * scalar)
        };
        if let Some(recipient) = self.units.get_mut(recipient_id) {
            recipient.set_child_object_damage_taken_multiplier(multiplier);
        }
    }

    fn base_child_damage_recipient(
        &self,
        anchor_id: EntityId,
        building_ids: &[EntityId],
    ) -> Option<EntityId> {
        let eligible = |building_id| {
            self.units.get(building_id).is_some_and(|building| {
                building.is_alive() && building.built && building.child_damage_base_scalar() > 0.0
            })
        };
        eligible(anchor_id)
            .then_some(anchor_id)
            .or_else(|| building_ids.iter().copied().find(|&id| eligible(id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{configure_unit_from_proto, population};
    use pipeline::database::hw1::{Database, ProtoObject};

    #[test]
    fn completed_child_buildings_protect_the_base_until_they_die() {
        let database = protection_database();
        let mut world = World::new();
        world.init_players(2);
        let anchor_id = configured_building(&mut world, &database, 0);
        population::apply_object_population(&mut world, anchor_id, &database, &database.objects[0]);
        let base_id = world.register_base(anchor_id).unwrap();

        let first_id = configured_building(&mut world, &database, 1);
        world.get_unit_mut(first_id).unwrap().built = false;
        assert!(world.add_building_to_base(base_id, first_id));
        assert_multiplier(&world, anchor_id, 1.0);

        assert!(population::complete_object_population(&mut world, first_id));
        world.activate_unit_on_built(first_id, &database, &database.objects[1]);
        assert_multiplier(&world, anchor_id, 0.5);

        let second_id = configured_building(&mut world, &database, 1);
        assert!(world.add_building_to_base(base_id, second_id));
        assert_multiplier(&world, anchor_id, 1.0 / 3.0);

        assert!(world.kill_unit(second_id, false));
        assert_multiplier(&world, anchor_id, 0.5);
        assert!(world.kill_unit(first_id, true));
        assert_multiplier(&world, anchor_id, 1.0);
    }

    #[test]
    fn combat_uses_the_composed_base_protection_multiplier() {
        let database = protection_database();
        let mut world = World::new();
        world.init_players(2);
        let anchor_id = configured_building(&mut world, &database, 0);
        population::apply_object_population(&mut world, anchor_id, &database, &database.objects[0]);
        let base_id = world.register_base(anchor_id).unwrap();
        let child_id = configured_building(&mut world, &database, 1);
        assert!(world.add_building_to_base(base_id, child_id));

        let dealt = world.apply_weapon_damage(2, anchor_id, 20.0, None, None);

        assert!((dealt - 20.0).abs() < f32::EPSILON);
        assert!((world.get_unit(anchor_id).unwrap().hitpoints - 90.0).abs() < f32::EPSILON);
        let _lethal = world.apply_weapon_damage(2, child_id, 1_000.0, None, None);
        assert_multiplier(&world, anchor_id, 1.0);
    }

    fn configured_building(world: &mut World, database: &Database, index: usize) -> EntityId {
        let id = world.create_building(1);
        let prototype = &database.objects[index];
        configure_unit_from_proto(world, id, &prototype.name, index, prototype);
        id
    }

    fn assert_multiplier(world: &World, unit_id: EntityId, expected: f32) {
        let actual = world
            .get_unit(unit_id)
            .unwrap()
            .child_object_damage_taken_multiplier();
        assert!((actual - expected).abs() < 1.0e-6, "{actual} != {expected}");
    }

    fn protection_database() -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "base".to_owned(),
                    object_class: Some("Building".to_owned()),
                    hitpoints: Some(100.0),
                    child_object_damage_taken_scalar: Some(1.0),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "child".to_owned(),
                    object_class: Some("Building".to_owned()),
                    flags: vec!["ChildForDamageTakenScalar".to_owned()],
                    ..ProtoObject::default()
                },
            ],
            ..Database::default()
        }
    }
}
