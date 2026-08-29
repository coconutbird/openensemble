//! Shared deterministic building worker for research, training, and construction.

use super::World;
use super::construction::construction_points;
use super::research::{research_points, technology_by_id};
use super::training::training_points;
use crate::entities::units::ProductionTask;
use crate::entities::{ConstructionKind, TrainingKind, TrainingTask, Unit};
use crate::entity_id::EntityId;
use crate::scenario::{create_object_from_prototype, create_squad_from_prototype};
use glam::Vec3;
use pipeline::database::hw1::Database;

const TRAINING_SPAWN_CLEARANCE: f32 = 2.0;

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
        if !dt.is_finite() || dt <= 0.0 {
            return ProductionUpdate::default();
        }
        let building_ids = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (unit.is_building() && !unit.production.is_idle()).then_some(id)
            })
            .collect::<Vec<_>>();
        let mut completed = Vec::new();
        let mut invalid = Vec::new();
        for building_id in building_ids {
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
                        if self.complete_direct_construction(building_id) {
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

    fn complete_training(
        &mut self,
        building_id: EntityId,
        task: &TrainingTask,
        database: &Database,
    ) -> Option<EntityId> {
        let Some((position, forward)) = training_spawn_transform(self, building_id) else {
            self.refund_training_task(task);
            return None;
        };
        let entity_id = match task.kind {
            TrainingKind::Squad => Some(create_squad_from_prototype(
                self,
                task.player_id,
                position,
                forward,
                &task.prototype_name,
                database,
            )),
            TrainingKind::Unit => create_object_from_prototype(
                self,
                task.player_id,
                position,
                forward,
                &task.prototype_name,
                database,
            ),
        };
        let Some(entity_id) = entity_id else {
            self.refund_training_task(task);
            return None;
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
        self.issue_training_rally_order(task.player_id, building_id, entity_id);
        if let Some(player) = self.get_player_mut(task.player_id) {
            player.release_reserved_population(&task.population_costs);
        }
        Some(entity_id)
    }

    fn issue_training_rally_order(
        &mut self,
        player_id: crate::player::PlayerId,
        building_id: EntityId,
        trained_entity_id: EntityId,
    ) {
        let Some(rally_point) = self.training_rally_point(building_id, player_id) else {
            return;
        };
        let Some(building_position) = self
            .get_building(building_id)
            .map(|building| building.base.position)
        else {
            return;
        };
        let mut destination = self.resolve_rally_point(rally_point);
        if rally_point.target_entity_id().is_none() {
            let direction = destination - building_position;
            if direction.length() > 4.0 {
                destination -= direction.normalize() * 4.0;
            }
        }
        let _issued = self.issue_move_order(player_id, trained_entity_id, destination);
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

fn training_spawn_transform(world: &World, building_id: EntityId) -> Option<(Vec3, Vec3)> {
    let building = world.get_building(building_id)?;
    let forward =
        Vec3::new(building.base.forward.x, 0.0, building.base.forward.z).normalize_or(Vec3::Z);
    let radius = building
        .obstruction_half_extents
        .x
        .abs()
        .max(building.obstruction_half_extents.z.abs());
    let position = building.base.position + forward * (radius + TRAINING_SPAWN_CLEARANCE);
    Some((position, forward))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{ProtoDataModification, ProtoDataRelativity, ProtoDataType, Resources};
    use pipeline::database::hw1::ProtoObject;

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
    fn completed_training_prefers_local_rally_and_stops_near_direct_points() {
        let database = training_database();
        let mut world = World::new();
        world.init_players(1);
        let building_id = world.create_building_at(1, Vec3::ZERO);
        assert!(world.set_player_rally_point(1, Vec3::new(0.0, 0.0, 30.0), None));
        assert!(world.set_unit_rally_point(building_id, 1, Vec3::new(0.0, 0.0, 20.0), None));

        let trained = world
            .complete_training(building_id, &training_task(), &database)
            .unwrap();

        assert_eq!(
            world.get_unit(trained).unwrap().move_target,
            Some(Vec3::new(0.0, 0.0, 16.0))
        );
    }

    #[test]
    fn completed_training_preserves_entity_rally_targets_without_offset() {
        let database = training_database();
        let mut world = World::new();
        world.init_players(1);
        let building_id = world.create_building_at(1, Vec3::ZERO);
        let target = world.create_unit_at(1, Vec3::new(30.0, 0.0, 10.0));
        assert!(world.set_player_rally_point(1, Vec3::ZERO, Some(target)));

        let trained = world
            .complete_training(building_id, &training_task(), &database)
            .unwrap();

        assert_eq!(
            world.get_unit(trained).unwrap().move_target,
            Some(Vec3::new(30.0, 0.0, 10.0))
        );
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
}
