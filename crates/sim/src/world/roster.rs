//! Authoritative retail-style live and queued player roster counts.

use super::World;
use crate::entities::{BuildingProduction, TrainingKind};
use crate::entity::Entity;
use crate::player::PlayerId;

impl World {
    /// Count a player's live units, including buildings, with an optional object-type filter.
    #[must_use]
    pub fn player_unit_count(&self, player_id: PlayerId, object_type: Option<&str>) -> u32 {
        saturating_count(self.units.iter().filter(|(_, unit)| {
            unit.base.player_id == player_id
                && unit.is_alive()
                && object_type.is_none_or(|expected| unit.is_object_type(expected))
        }))
    }

    /// Count a player's queued `TrainUnit` work with an optional object-type filter.
    #[must_use]
    pub fn player_future_unit_count(&self, player_id: PlayerId, object_type: Option<&str>) -> u32 {
        saturating_count(self.units.iter().flat_map(|(_, producer)| {
            producer
                .is_alive()
                .then_some(&producer.production)
                .into_iter()
                .flat_map(BuildingProduction::training_tasks)
                .filter(move |task| {
                    task.player_id() == player_id
                        && task.kind() == TrainingKind::Unit
                        && object_type.is_none_or(|expected| {
                            self.prototype_name_is_object_type(task.prototype_name(), expected)
                        })
                })
        }))
    }

    /// Count a player's live squads with an optional proto-squad filter.
    #[must_use]
    pub fn player_squad_count(&self, player_id: PlayerId, prototype_id: Option<i32>) -> u32 {
        saturating_count(self.squads.iter().filter(|(_, squad)| {
            squad.base.player_id == player_id
                && squad.is_alive()
                && prototype_id.is_none_or(|expected| squad.proto_squad_id == expected)
        }))
    }

    /// Count a player's queued `TrainSquad` work with an optional proto-squad filter.
    #[must_use]
    pub fn player_future_squad_count(&self, player_id: PlayerId, prototype_id: Option<i32>) -> u32 {
        saturating_count(self.units.iter().flat_map(|(_, producer)| {
            producer
                .is_alive()
                .then_some(&producer.production)
                .into_iter()
                .flat_map(BuildingProduction::training_tasks)
                .filter(move |task| {
                    task.player_id() == player_id
                        && task.kind() == TrainingKind::Squad
                        && prototype_id.is_none_or(|expected| {
                            self.queued_squad_matches_prototype(
                                expected,
                                task.prototype_id(),
                                task.prototype_name(),
                            )
                        })
                })
        }))
    }
}

fn saturating_count<T>(values: impl Iterator<Item = T>) -> u32 {
    u32::try_from(values.count()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{TrainingTask, UnitState};
    use crate::player::Resources;
    use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

    #[test]
    fn live_counts_include_buildings_and_apply_retail_type_filters() {
        let mut world = roster_world();
        let marine = world.create_unit(1);
        configure_unit(&mut world, marine, 17, "unsc_inf_marine_01", "Infantry");
        let building = world.create_building(1);
        configure_unit(
            &mut world,
            building,
            18,
            "unsc_bldg_barracks_01",
            "Building",
        );
        let dead = world.create_unit(1);
        configure_unit(&mut world, dead, 17, "unsc_inf_marine_01", "Infantry");
        world.get_unit_mut(dead).unwrap().state = UnitState::Dead;
        let other_player = world.create_unit(2);
        configure_unit(
            &mut world,
            other_player,
            17,
            "unsc_inf_marine_01",
            "Infantry",
        );

        assert_eq!(world.player_unit_count(1, None), 2);
        assert_eq!(world.player_unit_count(1, Some("Infantry")), 1);
        assert_eq!(world.player_unit_count(1, Some("UNSC_BLDG_BARRACKS_01")), 1);
        assert_eq!(world.player_unit_count(1, Some("Vehicle")), 0);

        let marine_squad = world.create_squad(1);
        world.get_squad_mut(marine_squad).unwrap().proto_squad_id = 70;
        let other_squad = world.create_squad(1);
        world.get_squad_mut(other_squad).unwrap().proto_squad_id = 71;
        let enemy_squad = world.create_squad(2);
        world.get_squad_mut(enemy_squad).unwrap().proto_squad_id = 70;

        assert_eq!(world.player_squad_count(1, None), 2);
        assert_eq!(world.player_squad_count(1, Some(70)), 1);
        assert_eq!(world.player_squad_count(1, Some(72)), 0);
    }

    #[test]
    fn future_counts_keep_unit_and_squad_training_separate() {
        let mut world = roster_world();
        let producer = world.create_building(1);
        enqueue(
            &mut world,
            producer,
            1,
            TrainingKind::Unit,
            17,
            "unsc_inf_marine_01",
        );
        enqueue(
            &mut world,
            producer,
            1,
            TrainingKind::Squad,
            0,
            "unsc_marine_squad",
        );
        enqueue(
            &mut world,
            producer,
            2,
            TrainingKind::Unit,
            17,
            "unsc_inf_marine_01",
        );

        assert_eq!(world.player_future_unit_count(1, None), 1);
        assert_eq!(world.player_future_unit_count(1, Some("Infantry")), 1);
        assert_eq!(world.player_future_unit_count(1, Some("Vehicle")), 0);
        assert_eq!(world.player_future_squad_count(1, None), 1);
        assert_eq!(world.player_future_squad_count(1, Some(70)), 1);
        assert_eq!(world.player_future_squad_count(1, Some(71)), 0);

        world.get_unit_mut(producer).unwrap().state = UnitState::Dead;
        assert_eq!(world.player_future_unit_count(1, None), 0);
        assert_eq!(world.player_future_squad_count(1, None), 0);
    }

    fn roster_world() -> World {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "unsc_inf_marine_01".to_owned(),
            dbid: Some(17),
            object_types: vec!["Infantry".to_owned(), "UNSC".to_owned()],
            ..ProtoObject::default()
        });
        database.squads.push(ProtoSquad {
            name: "unsc_marine_squad".to_owned(),
            dbid: Some(70),
            ..ProtoSquad::default()
        });
        database.squads.push(ProtoSquad {
            name: "unsc_warthog_squad".to_owned(),
            dbid: Some(71),
            ..ProtoSquad::default()
        });
        database.objects.push(ProtoObject {
            name: "unsc_bldg_barracks_01".to_owned(),
            dbid: Some(18),
            object_types: vec!["Building".to_owned(), "UNSC".to_owned()],
            ..ProtoObject::default()
        });
        let mut world = World::new();
        world.configure_prototype_catalogs(&database);
        world.init_players(2);
        world
    }

    fn configure_unit(
        world: &mut World,
        unit_id: crate::entity_id::EntityId,
        prototype_id: i32,
        prototype_name: &str,
        object_type: &str,
    ) {
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_id = prototype_id;
        unit.proto_object_name = prototype_name.to_owned();
        unit.object_types = vec![object_type.to_owned(), "UNSC".to_owned()];
    }

    fn enqueue(
        world: &mut World,
        producer_id: crate::entity_id::EntityId,
        player_id: PlayerId,
        kind: TrainingKind,
        prototype_id: i32,
        prototype_name: &str,
    ) {
        world
            .get_unit_mut(producer_id)
            .unwrap()
            .production
            .enqueue_training(TrainingTask {
                player_id,
                kind,
                prototype_id,
                prototype_name: prototype_name.to_owned(),
                current_points: 0.0,
                total_points: 1.0,
                cost: Resources::default(),
                population_costs: Vec::new(),
                train_limit_bucket: None,
                trigger_state: None,
            });
    }
}
