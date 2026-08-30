//! Shared deterministic building worker for research, training, and construction.

mod air_traffic_control;
mod birth;
mod fly_in;
mod placement;

use super::World;
use super::construction::construction_points;
use super::research::{research_points, technology_by_id};
use super::training::training_points;
use crate::entities::units::ProductionTask;
use crate::entities::{ConstructionKind, TrainingKind, TrainingTask, Unit};
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::scenario::{create_squad_from_prototype, create_unit_squad_from_prototype};
use pipeline::database::hw1::Database;

/// Work completed by one authoritative production update.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProductionUpdate {
    pub completed_research: usize,
    pub completed_training: usize,
    pub completed_construction: usize,
}

#[derive(Debug)]
enum ProductionTick {
    None,
    PromotedResearch {
        player_id: crate::player::PlayerId,
        technology_id: i32,
    },
    PromotedBuildOther,
    ResearchProgress {
        player_id: crate::player::PlayerId,
        technology_id: i32,
        points: f32,
    },
    Complete(ProductionTask),
    Invalid(ProductionTask),
}

impl World {
    /// Advance each building's one shared production worker.
    #[must_use]
    pub fn update_production(&mut self, dt: f32, database: &Database) -> ProductionUpdate {
        self.update_production_internal(dt, database, None)
    }

    /// Advance production with scenario-layered animation timing available.
    #[must_use]
    pub fn update_production_with_gameplay(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: &GameplayCatalog,
    ) -> ProductionUpdate {
        self.update_production_internal(dt, database, Some(gameplay))
    }

    fn update_production_internal(
        &mut self,
        dt: f32,
        database: &Database,
        gameplay: Option<&GameplayCatalog>,
    ) -> ProductionUpdate {
        if !dt.is_finite() || dt <= 0.0 {
            return ProductionUpdate::default();
        }
        if let Some(gameplay) = gameplay {
            self.update_air_traffic_controls(database, gameplay);
        }
        let building_ids = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (unit.is_building() && unit.production.needs_update()).then_some(id)
            })
            .collect::<Vec<_>>();
        let mut completed = Vec::new();
        let mut invalid = Vec::new();
        for building_id in building_ids {
            if let Some(building) = self.get_building_mut(building_id) {
                building.production.advance_training_recharges(dt);
            }
            self.update_trained_squad_birth(building_id, dt, database, gameplay);
            match self.tick_building_production(building_id, dt, database) {
                ProductionTick::PromotedResearch {
                    player_id,
                    technology_id,
                } => self.set_research_points(player_id, building_id, technology_id, 0.0),
                ProductionTick::ResearchProgress {
                    player_id,
                    technology_id,
                    points,
                } => self.set_research_points(player_id, building_id, technology_id, points),
                ProductionTick::PromotedBuildOther => {
                    if !self.start_promoted_build_other(building_id, database)
                        && let Some(task) = self
                            .get_building_mut(building_id)
                            .and_then(|building| building.production.current_item.take())
                    {
                        invalid.push((building_id, task));
                    }
                }
                ProductionTick::Complete(task) => completed.push((building_id, task)),
                ProductionTick::Invalid(task) => invalid.push((building_id, task)),
                ProductionTick::None => {}
            }
        }
        for (building_id, task) in invalid {
            let trigger_state = task.trigger_state();
            self.cancel_invalid_production(building_id, &task);
            if let Some(trigger_state) = trigger_state {
                self.notify_building_command_task(trigger_state, None);
            }
        }
        let mut update = ProductionUpdate::default();
        for (building_id, task) in completed {
            let trigger_state = task.trigger_state();
            let mut trained_squad = None;
            match task {
                ProductionTask::Research(task) => {
                    self.complete_research(building_id, &task, database);
                    update.completed_research += 1;
                }
                ProductionTask::Training(task) => {
                    let trained_entity = self.complete_training(building_id, &task, database);
                    if trained_entity.is_some() {
                        update.completed_training += 1;
                    }
                    if task.kind == TrainingKind::Squad {
                        trained_squad = trained_entity;
                    }
                }
                ProductionTask::Construction(task) => match task.kind {
                    ConstructionKind::Build => {
                        if self.complete_direct_construction(building_id, database) {
                            update.completed_construction += 1;
                        }
                    }
                    ConstructionKind::BuildOther => self.complete_build_other(&task),
                },
            }
            if let Some(trigger_state) = trigger_state {
                self.notify_building_command_task(trigger_state, trained_squad);
            }
        }
        update
    }

    /// Backward-compatible research update; training shares the same worker.
    #[must_use]
    pub fn update_research(&mut self, dt: f32, database: &Database) -> usize {
        self.update_production(dt, database).completed_research
    }

    pub(crate) fn refund_production_for_removed_unit(&mut self, unit: &Unit) {
        for task in unit.production.tasks() {
            let trigger_state = task.trigger_state();
            match task {
                ProductionTask::Research(task) => {
                    self.finish_research_assignment(
                        task.player_id,
                        unit.base.id,
                        task.technology_id,
                    );
                    self.refund_cost(task.player_id, &task.cost);
                }
                ProductionTask::Training(task) => self.refund_training_task(task),
                ProductionTask::Construction(task) => match task.kind {
                    ConstructionKind::Build => {}
                    ConstructionKind::BuildOther => {
                        self.refund_construction_task(task);
                        if let Some(target_id) = task.target_building_id {
                            let _removed = self.remove_unit(target_id);
                        }
                    }
                },
            }
            if let Some(trigger_state) = trigger_state {
                self.notify_building_command_task(trigger_state, None);
            }
        }
    }

    fn tick_building_production(
        &mut self,
        building_id: EntityId,
        dt: f32,
        database: &Database,
    ) -> ProductionTick {
        let promoted = self
            .get_building_mut(building_id)
            .is_some_and(|building| building.production.promote_next());
        if promoted {
            let Some(building) = self.get_building(building_id) else {
                return ProductionTick::None;
            };
            return match building
                .production
                .current_item
                .as_ref()
                .expect("promotion installs a current item")
            {
                ProductionTask::Research(task) => ProductionTick::PromotedResearch {
                    player_id: task.player_id,
                    technology_id: task.technology_id,
                },
                ProductionTask::Training(_) => ProductionTick::None,
                ProductionTask::Construction(task) => match task.kind {
                    ConstructionKind::Build => ProductionTick::None,
                    ConstructionKind::BuildOther => ProductionTick::PromotedBuildOther,
                },
            };
        }
        let build_other_done = self
            .get_building(building_id)
            .and_then(|building| building.production.current_construction())
            .filter(|task| task.kind == ConstructionKind::BuildOther)
            .and_then(|task| task.target_building_id)
            .is_some_and(|target_id| {
                self.get_building(target_id)
                    .is_none_or(|target| target.built || !target.base.alive)
            });
        let total_points = self
            .get_building(building_id)
            .and_then(|building| building.production.current_item.as_ref())
            .and_then(|task| match task {
                ProductionTask::Research(task) => technology_by_id(database, task.technology_id)
                    .and_then(|technology| research_points(technology).ok()),
                ProductionTask::Training(task) => training_points(
                    database,
                    task.kind,
                    task.prototype_id,
                    self.get_player(task.player_id)
                        .map(|player| &player.technologies),
                ),
                ProductionTask::Construction(task) => construction_points(
                    database,
                    task.prototype_id,
                    self.get_player(task.player_id)
                        .map(|player| &player.technologies),
                ),
            });
        let Some(building) = self.get_building_mut(building_id) else {
            return ProductionTick::None;
        };
        let work = dt * building.work_rate_scalar;
        let Some(task) = building.production.current_item.as_mut() else {
            return ProductionTick::None;
        };
        let Some(total_points) = total_points else {
            return ProductionTick::Invalid(
                building
                    .production
                    .current_item
                    .take()
                    .expect("invalid production item exists"),
            );
        };
        let (current_points, research) = advance_production_task(task, total_points, work);
        let is_build_other = matches!(
            task,
            ProductionTask::Construction(task) if task.kind == ConstructionKind::BuildOther
        );
        if (is_build_other && build_other_done)
            || (!is_build_other && current_points >= total_points)
        {
            return ProductionTick::Complete(
                building
                    .production
                    .current_item
                    .take()
                    .expect("completed production item exists"),
            );
        }
        research.map_or(ProductionTick::None, |(player_id, technology_id)| {
            ProductionTick::ResearchProgress {
                player_id,
                technology_id,
                points: current_points,
            }
        })
    }

    fn cancel_invalid_production(&mut self, building_id: EntityId, task: &ProductionTask) {
        match task {
            ProductionTask::Research(task) => {
                self.cancel_invalid_research_task(building_id, task);
            }
            ProductionTask::Training(task) => self.refund_training_task(task),
            ProductionTask::Construction(task) => {
                self.cancel_invalid_construction(building_id, task);
            }
        }
    }

    pub(super) fn complete_training(
        &mut self,
        building_id: EntityId,
        task: &TrainingTask,
        database: &Database,
    ) -> Option<EntityId> {
        self.complete_training_with_sound(building_id, task, database, true)
    }

    pub(super) fn complete_training_with_sound(
        &mut self,
        building_id: EntityId,
        task: &TrainingTask,
        database: &Database,
        play_sound: bool,
    ) -> Option<EntityId> {
        let Some((birth_controller_id, position, forward)) =
            self.training_birth_origin(building_id)
        else {
            self.refund_training_task(task);
            return None;
        };
        let (squad_id, entity_id) = match task.kind {
            TrainingKind::Squad => {
                let squad_id = create_squad_from_prototype(
                    self,
                    task.player_id,
                    position,
                    forward,
                    &task.prototype_name,
                    database,
                );
                (squad_id, squad_id)
            }
            TrainingKind::Unit => {
                let Some((squad_id, unit_id)) = create_unit_squad_from_prototype(
                    self,
                    task.player_id,
                    position,
                    forward,
                    &task.prototype_name,
                    database,
                ) else {
                    self.refund_training_task(task);
                    return None;
                };
                (squad_id, unit_id)
            }
        };
        match task.kind {
            TrainingKind::Squad => {
                if let Some(squad) = self.get_squad_mut(entity_id) {
                    squad.trained_by = Some(building_id);
                    squad.train_limit_bucket = task.train_limit_bucket;
                }
            }
            TrainingKind::Unit => {
                if let Some(unit) = self.get_unit_mut(entity_id) {
                    unit.trained_by = Some(building_id);
                    unit.train_limit_bucket = task.train_limit_bucket;
                }
            }
        }
        if !self.contain_and_queue_trained_squad(birth_controller_id, squad_id, play_sound) {
            let unit_ids = self
                .get_squad(squad_id)
                .map_or_else(Vec::new, |squad| squad.unit_ids.clone());
            for unit_id in unit_ids {
                let _removed = self.remove_unit(unit_id);
            }
            if self.get_squad(squad_id).is_some() {
                let _removed = self.remove_squad(squad_id);
            }
            self.refund_training_task(task);
            return None;
        }
        if let Some(player) = self.get_player_mut(task.player_id) {
            player.release_reserved_population(&task.population_costs);
        }
        Some(entity_id)
    }
}

fn advance_production_task(
    task: &mut ProductionTask,
    total_points: f32,
    work: f32,
) -> (f32, Option<(crate::player::PlayerId, i32)>) {
    match task {
        ProductionTask::Research(task) => {
            task.total_points = total_points;
            task.current_points = (task.current_points + work).min(total_points);
            (
                task.current_points,
                Some((task.player_id, task.technology_id)),
            )
        }
        ProductionTask::Training(task) => {
            task.total_points = total_points;
            task.current_points = (task.current_points + work).min(total_points);
            (task.current_points, None)
        }
        ProductionTask::Construction(task) => {
            task.total_points = total_points;
            if task.kind == ConstructionKind::BuildOther || !task.manual {
                task.current_points = (task.current_points + work).min(total_points);
            }
            (task.current_points, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{ProtoDataModification, ProtoDataRelativity, ProtoDataType, Resources};
    use glam::Vec3;
    use pipeline::database::hw1::ProtoObject;
    use pipeline::database::hw1::Squad as ProtoSquad;
    use pipeline::database::hw1::civs::Civ;
    use pipeline::database::hw1::objects::TrainerType;
    use pipeline::database::hw1::squads::{Birth, UnitEntry, UnitsWrapper};

    #[test]
    fn building_work_rate_scalar_advances_the_shared_worker() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "test_unit".to_owned(),
            dbid: Some(0),
            build_points: Some(10.0),
            ..ProtoObject::default()
        });
        let mut world = World::new();
        let building_id = world.create_building(1);
        let building = world.get_building_mut(building_id).unwrap();
        building.work_rate_scalar = 2.0;
        building.production.enqueue_training(TrainingTask {
            player_id: 1,
            kind: TrainingKind::Unit,
            prototype_id: 0,
            prototype_name: "test_unit".to_owned(),
            current_points: 0.0,
            total_points: 10.0,
            cost: Resources::new(),
            population_costs: Vec::new(),
            train_limit_bucket: None,
            trigger_state: None,
        });

        let _promoted = world.update_production(1.0, &database);
        let _advanced = world.update_production(1.0, &database);
        let progress = world
            .get_building(building_id)
            .unwrap()
            .production
            .current_training()
            .unwrap();
        assert!((progress.current_points() - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn trigger_modified_unit_build_points_drive_the_shared_worker() {
        let database = Database {
            objects: vec![ProtoObject {
                name: "test_unit".to_owned(),
                build_points: Some(10.0),
                ..ProtoObject::default()
            }],
            ..Database::default()
        };
        let mut world = World::new();
        world.init_players(1);
        world
            .get_player_mut(1)
            .unwrap()
            .technologies
            .modify_proto_data(
                "test_unit",
                &ProtoDataModification {
                    data_type: ProtoDataType::BuildPoints,
                    amount: 2.0,
                    relativity: ProtoDataRelativity::Percent,
                    all_actions: false,
                    name: None,
                    invert: false,
                    command_type: None,
                    command_data: None,
                },
            );
        let building_id = world.create_building(1);
        world
            .get_building_mut(building_id)
            .unwrap()
            .production
            .enqueue_training(TrainingTask {
                player_id: 1,
                kind: TrainingKind::Unit,
                prototype_id: 0,
                prototype_name: "test_unit".to_owned(),
                current_points: 0.0,
                total_points: 10.0,
                cost: Resources::new(),
                population_costs: Vec::new(),
                train_limit_bucket: None,
                trigger_state: None,
            });

        let _promoted = world.update_production(1.0, &database);
        let _advanced = world.update_production(1.0, &database);
        let progress = world
            .get_building(building_id)
            .unwrap()
            .production
            .current_training()
            .unwrap();
        assert!((progress.total_points() - 20.0).abs() <= f32::EPSILON);
        assert!((progress.current_points() - 1.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn trained_unit_waits_for_birth_then_uses_local_plotted_rally() {
        let database = training_database();
        let mut world = World::new();
        world.init_players(1);
        let building_id = world.create_building_at(1, Vec3::ZERO);
        assert!(world.set_player_rally_point(1, Vec3::new(0.0, 0.0, 30.0), None));
        assert!(world.set_unit_rally_point(building_id, 1, Vec3::new(0.0, 0.0, 20.0), None));

        let trained = world
            .complete_training(building_id, &training_task(), &database)
            .unwrap();
        let squad_id = world.get_unit(trained).unwrap().squad_id.unwrap();
        assert!(world.get_unit(trained).unwrap().is_garrisoned());
        assert_eq!(world.get_squad(squad_id).unwrap().move_target, None);

        let _birth = world.update_production(0.05, &database);

        assert_eq!(
            world.get_squad(squad_id).unwrap().move_target,
            Some(Vec3::new(0.0, 0.0, 15.5))
        );
        assert!(!world.get_unit(trained).unwrap().is_garrisoned());
    }

    #[test]
    fn trained_unit_birth_preserves_entity_rally_targets_without_offset() {
        let database = training_database();
        let mut world = World::new();
        world.init_players(1);
        let building_id = world.create_building_at(1, Vec3::ZERO);
        let target = world.create_unit_at(1, Vec3::new(30.0, 0.0, 10.0));
        assert!(world.set_player_rally_point(1, Vec3::ZERO, Some(target)));

        let trained = world
            .complete_training(building_id, &training_task(), &database)
            .unwrap();
        let squad_id = world.get_unit(trained).unwrap().squad_id.unwrap();
        let _birth = world.update_production(0.05, &database);

        assert_eq!(
            world.get_squad(squad_id).unwrap().move_target,
            Some(Vec3::new(30.0, 0.0, 10.0))
        );
    }

    #[test]
    fn associated_parking_lot_owns_birth_and_uses_producer_rally() {
        let database = training_database();
        let mut world = World::new();
        world.init_players(1);
        let producer = world.create_building_at(1, Vec3::ZERO);
        let parking = world.create_building_at(1, Vec3::new(50.0, 0.0, 0.0));
        assert!(world.associate_parking_lot(producer, parking));
        assert!(world.set_unit_rally_point(producer, 1, Vec3::new(70.0, 0.0, 0.0), None));

        let trained = world
            .complete_training(producer, &training_task(), &database)
            .unwrap();
        let squad_id = world.get_unit(trained).unwrap().squad_id.unwrap();

        assert_eq!(
            world.get_unit(trained).unwrap().garrison.container_id(),
            Some(parking)
        );
        assert_eq!(
            world
                .get_building(producer)
                .unwrap()
                .production
                .trained_squad_births()
                .count(),
            0
        );
        assert_eq!(
            world
                .get_building(parking)
                .unwrap()
                .production
                .trained_squad_births()
                .count(),
            1
        );

        let _birth = world.update_production(0.05, &database);

        assert!(!world.get_unit(trained).unwrap().is_garrisoned());
        assert!(world.get_squad(squad_id).unwrap().base.position.x >= 50.0);
        assert!(world.get_squad(squad_id).unwrap().move_target.unwrap().x > 60.0);
    }

    #[test]
    fn trained_birth_animations_are_authoritative_and_set_queue_delay() {
        let database = animated_training_database();
        let mut gameplay = GameplayCatalog::default();
        gameplay.insert_test_scripted_animation_clip(
            "member",
            "Birth0",
            "art/member_birth.uax",
            400,
        );
        gameplay.insert_test_scripted_animation_clip(
            "trainer",
            "Train",
            "art/trainer_train.uax",
            1_250,
        );
        let mut world = World::new();
        world.init_players(1);
        let trainer = world.create_building_at(1, Vec3::ZERO);
        world.get_building_mut(trainer).unwrap().proto_object_name = "trainer".to_owned();

        let squad_id = world
            .complete_training(trainer, &squad_training_task(), &database)
            .unwrap();
        let member = world.get_squad(squad_id).unwrap().unit_ids[0];
        let _birth = world.update_production_with_gameplay(0.05, &database, &gameplay);

        let member_animation = world.entity_scripted_animation(member).unwrap();
        assert_eq!(member_animation.animation_type(), "Birth0");
        assert_eq!(member_animation.asset_path(), Some("art/member_birth.uax"));
        let trainer_animation = world.entity_scripted_animation(trainer).unwrap();
        assert_eq!(trainer_animation.animation_type(), "Train");
        assert_eq!(trainer_animation.duration_ms(), 1_250);
        let birth_time = world
            .get_building(trainer)
            .unwrap()
            .production
            .trained_squad_birth_time();
        assert!((birth_time - 1.25).abs() <= f32::EPSILON);
    }

    #[test]
    fn flying_fly_in_birth_starts_above_trainer_then_rallies() {
        let database = fly_in_training_database(true);
        let mut world = World::new();
        world.init_players(1);
        let trainer = world.create_building_at(1, Vec3::ZERO);
        world.get_building_mut(trainer).unwrap().proto_object_name = "trainer".to_owned();
        assert!(world.set_unit_rally_point(trainer, 1, Vec3::Z * 30.0, None));

        let squad_id = world
            .complete_training(trainer, &fly_in_training_task(), &database)
            .unwrap();
        let leader_id = world.get_squad(squad_id).unwrap().unit_ids[0];
        let _birth = world.update_production(0.05, &database);

        assert!(
            world
                .get_squad(squad_id)
                .unwrap()
                .trained_air_birth()
                .is_some()
        );
        assert!((world.get_unit(leader_id).unwrap().base.position.y - 100.0).abs() <= f32::EPSILON);
        assert_eq!(world.get_squad(squad_id).unwrap().move_target, None);

        for _ in 0..40 {
            world.update_entities(0.05);
        }
        assert!(
            world
                .get_squad(squad_id)
                .unwrap()
                .trained_air_birth()
                .is_none()
        );
        assert!(world.get_unit(leader_id).unwrap().base.position.y.abs() <= f32::EPSILON);
        assert!(world.get_squad(squad_id).unwrap().move_target.is_some());
    }

    #[test]
    fn ground_fly_in_birth_uses_civilization_transport() {
        let database = fly_in_training_database(false);
        let mut world = World::new();
        world.init_players(1);
        world.get_player_mut(1).unwrap().civ_id = 0;
        let trainer = world.create_building_at(1, Vec3::ZERO);
        world.get_building_mut(trainer).unwrap().proto_object_name = "trainer".to_owned();
        assert!(world.set_unit_rally_point(trainer, 1, Vec3::Z * 30.0, None));

        let passenger_id = world
            .complete_training(trainer, &fly_in_training_task(), &database)
            .unwrap();
        let passenger_unit = world.get_squad(passenger_id).unwrap().unit_ids[0];
        let _birth = world.update_production(0.05, &database);
        let carrier_id = world
            .squads
            .iter()
            .find_map(|(id, squad)| squad.transport_fly_in().is_some().then_some(id))
            .expect("civilization transport fly-in");

        assert!(world.get_unit(passenger_unit).unwrap().is_garrisoned());
        assert_ne!(carrier_id, passenger_id);

        for _ in 0..40 {
            world.update_entities(0.05);
        }
        assert!(!world.get_unit(passenger_unit).unwrap().is_garrisoned());
        assert!(world.get_squad(carrier_id).is_none());
        let passenger = world.get_squad(passenger_id).unwrap();
        assert!(passenger.move_target.is_some() || passenger.base.position.z > 0.0);
    }

    fn training_database() -> Database {
        Database {
            objects: vec![ProtoObject {
                name: "test_unit".to_owned(),
                object_class: Some("Unit".to_owned()),
                ..ProtoObject::default()
            }],
            ..Database::default()
        }
    }

    fn animated_training_database() -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "trainer".to_owned(),
                    object_class: Some("Building".to_owned()),
                    flags: vec!["Hook".to_owned(), "BirthOnTop".to_owned()],
                    trainer_type: Some(TrainerType {
                        trainer_type: 0,
                        apply_formation: Some(false),
                    }),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "member".to_owned(),
                    object_class: Some("Unit".to_owned()),
                    ..ProtoObject::default()
                },
            ],
            squads: vec![ProtoSquad {
                name: "trained_squad".to_owned(),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "member".to_owned(),
                        count: 1,
                        ..UnitEntry::default()
                    }],
                }),
                birth: Some(Birth {
                    animation_0: Some("Birth0".to_owned()),
                    trainer_animation: Some("Train".to_owned()),
                    ..Birth::default()
                }),
                ..ProtoSquad::default()
            }],
            ..Database::default()
        }
    }

    fn fly_in_training_database(flying: bool) -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "trainer".to_owned(),
                    object_class: Some("Building".to_owned()),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "member".to_owned(),
                    object_class: Some("Unit".to_owned()),
                    movement_type: flying.then(|| "Air".to_owned()),
                    velocity: Some(50.0),
                    object_types: vec!["Infantry".to_owned()],
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "transport".to_owned(),
                    object_class: Some("Unit".to_owned()),
                    movement_type: Some("Air".to_owned()),
                    velocity: Some(100.0),
                    contain: vec!["Infantry".to_owned()],
                    max_contained: Some(8),
                    ..ProtoObject::default()
                },
            ],
            squads: vec![ProtoSquad {
                name: "fly_in_squad".to_owned(),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "member".to_owned(),
                        count: 1,
                        ..UnitEntry::default()
                    }],
                }),
                birth: Some(Birth {
                    birth_type: Some("FlyIn".to_owned()),
                    ..Birth::default()
                }),
                ..ProtoSquad::default()
            }],
            civs: vec![Civ {
                name: "test_civ".to_owned(),
                transport: Some("transport".to_owned()),
                ..Civ::default()
            }],
            ..Database::default()
        }
    }

    fn training_task() -> TrainingTask {
        TrainingTask {
            player_id: 1,
            kind: TrainingKind::Unit,
            prototype_id: 0,
            prototype_name: "test_unit".to_owned(),
            current_points: 0.0,
            total_points: 0.0,
            cost: Resources::new(),
            population_costs: Vec::new(),
            train_limit_bucket: None,
            trigger_state: None,
        }
    }

    fn squad_training_task() -> TrainingTask {
        TrainingTask {
            player_id: 1,
            kind: TrainingKind::Squad,
            prototype_id: 0,
            prototype_name: "trained_squad".to_owned(),
            current_points: 0.0,
            total_points: 0.0,
            cost: Resources::new(),
            population_costs: Vec::new(),
            train_limit_bucket: None,
            trigger_state: None,
        }
    }

    fn fly_in_training_task() -> TrainingTask {
        TrainingTask {
            prototype_name: "fly_in_squad".to_owned(),
            ..squad_training_task()
        }
    }
}
