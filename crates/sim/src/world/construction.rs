//! Retail-style building construction, placement, payment, and cancellation.

mod on_built;
mod placement;

use super::{ObjectCostError, World};
use crate::entities::units::{ProductionTask, TriggerCommandStateRef};
use crate::entities::{ConstructionKind, ConstructionProgress, ConstructionTask};
use crate::entity_id::EntityId;
use crate::player::{PlayerId, PlayerTechState, PopulationCost, Resources};
use crate::scenario::create_unbuilt_building_from_prototype;
use crate::scenario::population::{object_population_costs, object_population_type_id};
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::{ProtoObject, TrainLimitType};

use placement::{direct_build_transform, find_build_other_socket};

impl World {
    pub(crate) fn set_construction_damage_multiplier(&mut self, multiplier: Option<f32>) {
        self.construction_damage_multiplier = multiplier
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(1.0);
    }
}

/// Result of accepting one `BuildOther` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructionQueueResult {
    /// `BuildOther` creates its target only after reaching the shared worker.
    pub target_building_id: Option<EntityId>,
}

/// Rejection produced while validating authoritative construction.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConstructionError {
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    #[error("builder {0:?} is not present in the world")]
    BuilderNotFound(EntityId),
    #[error("builder {builder_id:?} is not owned by player {player_id}")]
    BuilderNotOwned {
        builder_id: EntityId,
        player_id: PlayerId,
    },
    #[error("builder {0:?} has not completed construction")]
    BuilderUnbuilt(EntityId),
    #[error("runtime building prototype ID {0} was not found")]
    PrototypeNotFound(i32),
    #[error("construction target '{0}' is not a building")]
    PrototypeNotBuilding(String),
    #[error("builder {builder_id:?} does not offer {kind:?} '{prototype}'")]
    CommandUnavailable {
        builder_id: EntityId,
        kind: ConstructionKind,
        prototype: String,
    },
    #[error(transparent)]
    ObjectCost(#[from] ObjectCostError),
    #[error("construction '{0}' has invalid build points")]
    InvalidBuildPoints(String),
    #[error("construction '{0}' has population but the database has no population table")]
    MissingPopulationTable(String),
    #[error("'{prototype}' references unknown population type '{population}'")]
    UnknownPopulation {
        prototype: String,
        population: String,
    },
    #[error("player {player_id} cannot afford '{prototype}'")]
    InsufficientResources {
        player_id: PlayerId,
        prototype: String,
    },
    #[error("player {player_id} has no population room for '{prototype}'")]
    PopulationFull {
        player_id: PlayerId,
        prototype: String,
    },
    #[error("the train limit for '{prototype}' on builder {builder_id:?} is reached")]
    TrainLimitReached {
        builder_id: EntityId,
        prototype: String,
    },
    #[error("builder {0:?} already has queued or active BuildOther work")]
    BuildOtherAlreadyQueued(EntityId),
    #[error("builder {builder_id:?} has no available socket for '{prototype}'")]
    SocketUnavailable {
        builder_id: EntityId,
        prototype: String,
    },
    #[error("construction position and facing must be finite")]
    InvalidTransform,
    #[error("new building {0:?} could not start its construction worker")]
    WorkerUnavailable(EntityId),
}

#[derive(Debug, Clone)]
struct ConstructionDefinition {
    prototype_id: i32,
    forbid_id: i32,
    prototype_name: String,
    total_points: f32,
    cost: Resources,
    population_costs: Vec<PopulationCost>,
    manual: bool,
}

#[derive(Debug, Clone, Copy)]
struct TrainLimitRule {
    count: u32,
    bucket: Option<u8>,
}

#[derive(Debug, Clone, Copy)]
struct BuildOtherRequest<'database> {
    player_id: PlayerId,
    builder_id: EntityId,
    database: &'database Database,
    prototype_id: i32,
    no_cost: bool,
    trigger_state: Option<TriggerCommandStateRef>,
}

impl World {
    /// Validate, pay for, and immediately create one unfinished building.
    ///
    /// `Build` is issued to the builder, but the resulting target owns the
    /// construction worker just as it does in retail.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid player, builder, prototype, command,
    /// transform, cost, population requirement, train limit, or worker state.
    pub fn start_build(
        &mut self,
        player_id: PlayerId,
        builder_id: EntityId,
        database: &Database,
        prototype_id: i32,
        position: Vec3,
        socket_id: EntityId,
    ) -> Result<EntityId, ConstructionError> {
        let mut definition = construction_definition(
            database,
            prototype_id,
            self.get_player(player_id)
                .map(|player| &player.technologies),
        )?;
        definition.cost = self.object_cost(database, player_id, prototype_id)?;
        let limit = self.validate_construction_command(
            player_id,
            builder_id,
            database,
            &definition,
            ConstructionKind::Build,
        )?;
        self.validate_construction_capacity(player_id, builder_id, &definition, limit, false)?;
        let choice =
            direct_build_transform(self, database, position, socket_id, definition.prototype_id)?;
        self.pay_construction(player_id, &definition, false, false)?;

        let owner_id = self
            .get_unit(builder_id)
            .map(|builder| builder.base.player_id)
            .ok_or(ConstructionError::BuilderNotFound(builder_id))?;
        let Some(building_id) = create_unbuilt_building_from_prototype(
            self,
            owner_id,
            choice.position,
            choice.forward,
            &definition.prototype_name,
            database,
        ) else {
            self.refund_construction_payment(player_id, &definition.cost, &[], false);
            return Err(ConstructionError::PrototypeNotBuilding(
                definition.prototype_name,
            ));
        };
        let task = direct_construction_task(
            owner_id,
            player_id,
            &definition,
            Resources::new(),
            Vec::new(),
            building_id,
            limit.and_then(|rule| rule.bucket),
        );
        let started = self.get_building_mut(building_id).is_some_and(|building| {
            building.built_by = Some(builder_id);
            building.build_socket_id = choice.entity_id;
            building.build_socket_index = choice.index;
            building.production.start_construction(ConstructionTask {
                cost: definition.cost,
                ..task
            })
        });
        if !started {
            let _removed = self.remove_unit(building_id);
            self.refund_construction_payment(player_id, &definition.cost, &[], false);
            return Err(ConstructionError::WorkerUnavailable(building_id));
        }
        if let Some(socket_id) = choice.entity_id
            && !self.connect_socket_plug(socket_id, building_id)
        {
            let _removed = self.remove_unit(building_id);
            self.refund_construction_payment(player_id, &definition.cost, &[], false);
            return Err(ConstructionError::SocketUnavailable {
                builder_id: socket_id,
                prototype: definition.prototype_name,
            });
        }
        self.attach_constructed_building_to_base(
            builder_id,
            building_id,
            database,
            &definition.prototype_name,
        );
        Ok(building_id)
    }

    /// Cancel a direct `Build` task owned by its unfinished target.
    ///
    /// # Errors
    ///
    /// Returns an error when the player or target building is absent.
    pub fn cancel_build(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        prototype_id: i32,
    ) -> Result<bool, ConstructionError> {
        if self.get_player(player_id).is_none() {
            return Err(ConstructionError::PlayerNotFound(player_id));
        }
        let Some(building) = self.get_building_mut(building_id) else {
            return Err(ConstructionError::BuilderNotFound(building_id));
        };
        let Some(task) = building.production.cancel_construction(
            player_id,
            ConstructionKind::Build,
            prototype_id,
        ) else {
            return Ok(false);
        };
        self.refund_construction_payment(task.purchasing_player_id, &task.cost, &[], false);
        if let Some(trigger_state) = task.trigger_state {
            self.notify_building_command_task(trigger_state, None);
        }
        let _removed = self.remove_unit(building_id);
        Ok(true)
    }

    /// Validate, pay, reserve population, and enqueue one `BuildOther` item.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid construction data, unavailable commands or
    /// sockets, insufficient resources or population, or duplicate work.
    pub fn queue_build_other(
        &mut self,
        player_id: PlayerId,
        builder_id: EntityId,
        database: &Database,
        prototype_id: i32,
    ) -> Result<ConstructionQueueResult, ConstructionError> {
        self.queue_build_other_internal(BuildOtherRequest {
            player_id,
            builder_id,
            database,
            prototype_id,
            no_cost: false,
            trigger_state: None,
        })
    }

    pub(crate) fn queue_trigger_build_other(
        &mut self,
        player_id: PlayerId,
        builder_id: EntityId,
        database: &Database,
        prototype_id: i32,
        no_cost: bool,
        trigger_state: Option<TriggerCommandStateRef>,
    ) -> Result<ConstructionQueueResult, ConstructionError> {
        self.queue_build_other_internal(BuildOtherRequest {
            player_id,
            builder_id,
            database,
            prototype_id,
            no_cost,
            trigger_state,
        })
    }

    fn queue_build_other_internal(
        &mut self,
        request: BuildOtherRequest<'_>,
    ) -> Result<ConstructionQueueResult, ConstructionError> {
        let BuildOtherRequest {
            player_id,
            builder_id,
            database,
            prototype_id,
            no_cost,
            trigger_state,
        } = request;
        let mut definition = construction_definition(
            database,
            prototype_id,
            self.get_player(player_id)
                .map(|player| &player.technologies),
        )?;
        if !no_cost {
            definition.cost = self.object_cost(database, player_id, prototype_id)?;
        }
        let limit = self.validate_construction_command(
            player_id,
            builder_id,
            database,
            &definition,
            ConstructionKind::BuildOther,
        )?;
        if self.get_unit(builder_id).is_some_and(|builder| {
            builder
                .production
                .construction_tasks()
                .any(|task| task.kind == ConstructionKind::BuildOther)
        }) {
            return Err(ConstructionError::BuildOtherAlreadyQueued(builder_id));
        }
        self.validate_construction_capacity(player_id, builder_id, &definition, limit, no_cost)?;
        find_build_other_socket(
            self,
            builder_id,
            database,
            definition.prototype_id,
            &definition.prototype_name,
        )?;
        self.pay_construction(player_id, &definition, true, no_cost)?;

        let task = ConstructionTask {
            player_id,
            purchasing_player_id: player_id,
            kind: ConstructionKind::BuildOther,
            prototype_id,
            prototype_name: definition.prototype_name,
            current_points: 0.0,
            total_points: definition.total_points,
            cost: if no_cost {
                Resources::new()
            } else {
                definition.cost
            },
            population_costs: definition.population_costs,
            target_building_id: None,
            manual: definition.manual,
            train_limit_bucket: limit.and_then(|rule| rule.bucket),
            trigger_state,
        };
        let Some(builder) = self.get_building_mut(builder_id) else {
            self.refund_construction_task(&task);
            return Err(ConstructionError::BuilderNotFound(builder_id));
        };
        builder.production.enqueue_construction(task);
        Ok(ConstructionQueueResult {
            target_building_id: None,
        })
    }

    /// Cancel the newest queued matching `BuildOther`, then the current item.
    ///
    /// # Errors
    ///
    /// Returns an error when the player or builder is absent.
    pub fn cancel_build_other(
        &mut self,
        player_id: PlayerId,
        builder_id: EntityId,
        prototype_id: i32,
    ) -> Result<bool, ConstructionError> {
        if self.get_player(player_id).is_none() {
            return Err(ConstructionError::PlayerNotFound(player_id));
        }
        let Some(builder) = self.get_building_mut(builder_id) else {
            return Err(ConstructionError::BuilderNotFound(builder_id));
        };
        let Some(task) = builder.production.cancel_construction(
            player_id,
            ConstructionKind::BuildOther,
            prototype_id,
        ) else {
            return Ok(false);
        };
        self.refund_construction_task(&task);
        if let Some(trigger_state) = task.trigger_state {
            self.notify_building_command_task(trigger_state, None);
        }
        if let Some(target_id) = task.target_building_id {
            let _removed = self.remove_unit(target_id);
        }
        Ok(true)
    }

    /// Query one direct or socket-construction task for UI presentation.
    ///
    /// # Errors
    ///
    /// Returns an error when the player or worker building is absent.
    pub fn construction_progress(
        &self,
        player_id: PlayerId,
        worker_id: EntityId,
        kind: ConstructionKind,
        prototype_id: i32,
    ) -> Result<Option<ConstructionProgress>, ConstructionError> {
        if self.get_player(player_id).is_none() {
            return Err(ConstructionError::PlayerNotFound(player_id));
        }
        let worker = self
            .get_building(worker_id)
            .ok_or(ConstructionError::BuilderNotFound(worker_id))?;
        Ok(worker
            .production
            .construction_task(player_id, kind, prototype_id)
            .map(|(task, queued)| self.construction_progress_snapshot(worker_id, task, queued)))
    }

    /// Add external/manual construction work to an unfinished building.
    pub fn add_build_points(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        points: f32,
    ) -> bool {
        if !points.is_finite() || points <= 0.0 {
            return false;
        }
        let Some(building) = self.get_building_mut(building_id) else {
            return false;
        };
        let Some(ProductionTask::Construction(task)) = building.production.current_item.as_mut()
        else {
            return false;
        };
        if task.kind != ConstructionKind::Build || task.purchasing_player_id != player_id {
            return false;
        }
        task.current_points = (task.current_points + points).min(task.total_points);
        true
    }

    pub(super) fn start_promoted_build_other(
        &mut self,
        builder_id: EntityId,
        database: &Database,
    ) -> bool {
        let Some(task) = self
            .get_building(builder_id)
            .and_then(|builder| builder.production.current_construction())
            .filter(|task| {
                task.kind == ConstructionKind::BuildOther && task.target_building_id.is_none()
            })
            .cloned()
        else {
            return false;
        };
        let definition = ConstructionDefinition {
            prototype_id: task.prototype_id,
            forbid_id: object_by_runtime_id(database, task.prototype_id)
                .and_then(|prototype| prototype.dbid)
                .unwrap_or(task.prototype_id),
            prototype_name: task.prototype_name.clone(),
            total_points: task.total_points,
            cost: task.cost,
            population_costs: task.population_costs.clone(),
            manual: task.manual,
        };
        let Ok(choice) = find_build_other_socket(
            self,
            builder_id,
            database,
            definition.prototype_id,
            &definition.prototype_name,
        ) else {
            return false;
        };
        let Some(building_id) = create_unbuilt_building_from_prototype(
            self,
            task.player_id,
            choice.position,
            choice.forward,
            &task.prototype_name,
            database,
        ) else {
            return false;
        };
        let child_task = direct_construction_task(
            task.player_id,
            task.purchasing_player_id,
            &definition,
            Resources::new(),
            Vec::new(),
            building_id,
            task.train_limit_bucket,
        );
        let child_started = self.get_building_mut(building_id).is_some_and(|building| {
            building.built_by = Some(builder_id);
            building.build_socket_id = choice.entity_id;
            building.build_socket_index = choice.index;
            building.train_limit_bucket = task.train_limit_bucket;
            building.production.start_construction(child_task)
        });
        if !child_started {
            let _removed = self.remove_unit(building_id);
            return false;
        }
        if let Some(socket_id) = choice.entity_id
            && !self.connect_socket_plug(socket_id, building_id)
        {
            let _removed = self.remove_unit(building_id);
            return false;
        }
        self.attach_constructed_building_to_base(
            builder_id,
            building_id,
            database,
            &definition.prototype_name,
        );
        let _parking_lot = crate::scenario::parking_lots::materialize_auto_parking_lot(
            self,
            builder_id,
            building_id,
            database,
        );
        let Some(ProductionTask::Construction(source_task)) = self
            .get_building_mut(builder_id)
            .and_then(|builder| builder.production.current_item.as_mut())
        else {
            let _removed = self.remove_unit(building_id);
            return false;
        };
        source_task.target_building_id = Some(building_id);
        true
    }

    pub(super) fn complete_direct_construction(
        &mut self,
        building_id: EntityId,
        database: &Database,
    ) -> bool {
        if !crate::scenario::population::complete_object_population(self, building_id) {
            return false;
        }
        let Some(prototype_name) = self
            .get_building(building_id)
            .map(|building| building.proto_object_name.clone())
        else {
            return true;
        };
        if let Some(prototype) = object_by_name(database, &prototype_name) {
            self.activate_unit_on_built(building_id, database, prototype);
        }
        true
    }

    pub(super) fn complete_build_other(&mut self, task: &ConstructionTask) {
        if let Some(player) = self.get_player_mut(task.purchasing_player_id) {
            player.release_reserved_population(&task.population_costs);
        }
    }

    pub(super) fn cancel_invalid_construction(
        &mut self,
        worker_id: EntityId,
        task: &ConstructionTask,
    ) {
        match task.kind {
            ConstructionKind::Build => {
                let _removed = self.remove_unit(worker_id);
            }
            ConstructionKind::BuildOther => {
                self.refund_construction_task(task);
                if let Some(target_id) = task.target_building_id {
                    let _removed = self.remove_unit(target_id);
                }
            }
        }
    }

    pub(super) fn refund_construction_task(&mut self, task: &ConstructionTask) {
        self.refund_construction_payment(
            task.purchasing_player_id,
            &task.cost,
            &task.population_costs,
            task.kind == ConstructionKind::BuildOther,
        );
    }

    fn validate_construction_command(
        &self,
        player_id: PlayerId,
        builder_id: EntityId,
        database: &Database,
        definition: &ConstructionDefinition,
        kind: ConstructionKind,
    ) -> Result<Option<TrainLimitRule>, ConstructionError> {
        let player = self
            .get_player(player_id)
            .ok_or(ConstructionError::PlayerNotFound(player_id))?;
        let builder = self
            .get_unit(builder_id)
            .ok_or(ConstructionError::BuilderNotFound(builder_id))?;
        if builder.base.player_id != player_id {
            return Err(ConstructionError::BuilderNotOwned {
                builder_id,
                player_id,
            });
        }
        if !builder.is_operational() {
            return Err(ConstructionError::BuilderUnbuilt(builder_id));
        }
        if kind == ConstructionKind::BuildOther && !builder.is_building() {
            return Err(command_unavailable(builder_id, kind, definition));
        }
        if player.is_object_forbidden(database, definition.forbid_id) {
            return Err(command_unavailable(builder_id, kind, definition));
        }
        let Some(builder_prototype) = object_by_name(database, &builder.proto_object_name) else {
            return Err(command_unavailable(builder_id, kind, definition));
        };
        let authored = builder_prototype.commands.iter().any(|command| {
            command
                .target
                .trim()
                .eq_ignore_ascii_case(&definition.prototype_name)
                && command.command_type.as_deref().is_some_and(|command_type| {
                    command_type
                        .trim()
                        .eq_ignore_ascii_case(kind.command_name())
                })
        });
        let enabled = player.technologies.command_enabled(
            &builder.proto_object_name,
            kind.command_name(),
            &definition.prototype_name,
            authored,
        );
        if !enabled {
            return Err(command_unavailable(builder_id, kind, definition));
        }
        Ok(find_train_limit(builder_prototype, definition))
    }

    fn validate_construction_capacity(
        &self,
        player_id: PlayerId,
        builder_id: EntityId,
        definition: &ConstructionDefinition,
        limit: Option<TrainLimitRule>,
        no_cost: bool,
    ) -> Result<(), ConstructionError> {
        if limit.is_some_and(|rule| {
            self.construction_limit_count(builder_id, definition, rule) >= rule.count
        }) {
            return Err(ConstructionError::TrainLimitReached {
                builder_id,
                prototype: definition.prototype_name.clone(),
            });
        }
        let player = self
            .get_player(player_id)
            .ok_or(ConstructionError::PlayerNotFound(player_id))?;
        if !no_cost && !player.resources.can_afford(&definition.cost) {
            return Err(ConstructionError::InsufficientResources {
                player_id,
                prototype: definition.prototype_name.clone(),
            });
        }
        if !player.can_reserve_population(&definition.population_costs) {
            return Err(ConstructionError::PopulationFull {
                player_id,
                prototype: definition.prototype_name.clone(),
            });
        }
        Ok(())
    }

    fn construction_limit_count(
        &self,
        builder_id: EntityId,
        definition: &ConstructionDefinition,
        rule: TrainLimitRule,
    ) -> u32 {
        let matches = |prototype_name: &str, bucket| {
            if let Some(expected_bucket) = rule.bucket {
                bucket == Some(expected_bucket)
            } else {
                prototype_name.eq_ignore_ascii_case(&definition.prototype_name)
            }
        };
        let queued = self.get_unit(builder_id).map_or(0, |builder| {
            builder
                .production
                .construction_tasks()
                .filter(|task| {
                    task.target_building_id.is_none()
                        && matches(&task.prototype_name, task.train_limit_bucket)
                })
                .count()
        });
        let live = self
            .units
            .iter()
            .filter(|(_, unit)| {
                unit.built_by == Some(builder_id)
                    && matches(&unit.proto_object_name, unit.train_limit_bucket)
            })
            .count();
        u32::try_from(queued + live).unwrap_or(u32::MAX)
    }

    fn pay_construction(
        &mut self,
        player_id: PlayerId,
        definition: &ConstructionDefinition,
        reserve_population: bool,
        no_cost: bool,
    ) -> Result<(), ConstructionError> {
        let player = self
            .get_player_mut(player_id)
            .ok_or(ConstructionError::PlayerNotFound(player_id))?;
        if !no_cost && !player.resources.can_afford(&definition.cost) {
            return Err(ConstructionError::InsufficientResources {
                player_id,
                prototype: definition.prototype_name.clone(),
            });
        }
        if reserve_population && !player.can_reserve_population(&definition.population_costs) {
            return Err(ConstructionError::PopulationFull {
                player_id,
                prototype: definition.prototype_name.clone(),
            });
        }
        if !no_cost {
            player.resources.pay(&definition.cost);
        }
        if reserve_population {
            let reserved = player.reserve_population(&definition.population_costs);
            debug_assert!(reserved, "mutable population check just succeeded");
        }
        Ok(())
    }

    fn refund_construction_payment(
        &mut self,
        player_id: PlayerId,
        cost: &Resources,
        population_costs: &[PopulationCost],
        release_population: bool,
    ) {
        if let Some(player) = self.get_player_mut(player_id) {
            player.resources.refund(cost);
            if release_population {
                player.release_reserved_population(population_costs);
            }
        }
    }

    fn construction_progress_snapshot(
        &self,
        worker_id: EntityId,
        task: &ConstructionTask,
        queued: bool,
    ) -> ConstructionProgress {
        let progress = task
            .target_building_id
            .and_then(|target_id| self.get_building(target_id))
            .and_then(|target| target.production.current_construction())
            .filter(|target| target.kind == ConstructionKind::Build);
        ConstructionProgress {
            worker_id,
            building_id: task.target_building_id,
            kind: task.kind,
            prototype_id: task.prototype_id,
            current_points: progress.map_or(task.current_points, |target| target.current_points),
            total_points: progress.map_or(task.total_points, |target| target.total_points),
            queued,
        }
    }

    fn attach_constructed_building_to_base(
        &mut self,
        builder_id: EntityId,
        building_id: EntityId,
        database: &Database,
        prototype_name: &str,
    ) {
        let base_id = self
            .get_unit(builder_id)
            .and_then(|builder| builder.base_id);
        if let Some(base_id) = base_id
            && self
                .get_building(building_id)
                .is_some_and(|building| building.base_id.is_none())
        {
            let _attached = self.add_building_to_base(base_id, building_id);
        } else if object_by_name(database, prototype_name)
            .is_some_and(|prototype| has_flag(&prototype.flags, "KBCreatesBase"))
        {
            let _base_id = self.register_base(building_id);
        }
    }
}

pub(super) fn construction_points(
    database: &Database,
    prototype_id: i32,
    technologies: Option<&PlayerTechState>,
) -> Option<f32> {
    let prototype = object_by_runtime_id(database, prototype_id)?;
    let base = prototype.build_points.unwrap_or_default();
    let points = technologies.map_or(base, |state| state.build_points(&prototype.name, base));
    (points.is_finite() && points >= 0.0).then_some(points)
}

fn construction_definition(
    database: &Database,
    prototype_id: i32,
    technologies: Option<&PlayerTechState>,
) -> Result<ConstructionDefinition, ConstructionError> {
    let prototype = object_by_runtime_id(database, prototype_id)
        .ok_or(ConstructionError::PrototypeNotFound(prototype_id))?;
    if !is_building_proto(prototype) {
        return Err(ConstructionError::PrototypeNotBuilding(
            prototype.name.clone(),
        ));
    }
    validate_object_population(database, prototype)?;
    Ok(ConstructionDefinition {
        prototype_id,
        forbid_id: prototype.dbid.unwrap_or(prototype_id),
        prototype_name: prototype.name.clone(),
        total_points: valid_build_points(prototype, technologies)?,
        cost: Resources::new(),
        population_costs: object_population_costs(database, prototype),
        manual: has_flag(&prototype.flags, "ManualBuild"),
    })
}

fn direct_construction_task(
    owner_id: PlayerId,
    purchasing_player_id: PlayerId,
    definition: &ConstructionDefinition,
    cost: Resources,
    population_costs: Vec<PopulationCost>,
    building_id: EntityId,
    train_limit_bucket: Option<u8>,
) -> ConstructionTask {
    ConstructionTask {
        player_id: owner_id,
        purchasing_player_id,
        kind: ConstructionKind::Build,
        prototype_id: definition.prototype_id,
        prototype_name: definition.prototype_name.clone(),
        current_points: 0.0,
        total_points: definition.total_points,
        cost,
        population_costs,
        target_building_id: Some(building_id),
        manual: definition.manual,
        train_limit_bucket,
        trigger_state: None,
    }
}

fn validate_object_population(
    database: &Database,
    prototype: &ProtoObject,
) -> Result<(), ConstructionError> {
    for amount in &prototype.population {
        if !amount.amount.is_finite() || amount.amount < 0.0 {
            return Err(unknown_population(
                prototype,
                amount.population_type.as_deref(),
            ));
        }
        if amount.amount == 0.0 {
            continue;
        }
        if database
            .game_data
            .as_ref()
            .and_then(|game_data| game_data.pops.as_ref())
            .is_none()
        {
            return Err(ConstructionError::MissingPopulationTable(
                prototype.name.clone(),
            ));
        }
        if object_population_type_id(database, prototype, amount).is_none() {
            return Err(unknown_population(
                prototype,
                amount.population_type.as_deref(),
            ));
        }
    }
    Ok(())
}

fn unknown_population(prototype: &ProtoObject, population: Option<&str>) -> ConstructionError {
    ConstructionError::UnknownPopulation {
        prototype: prototype.name.clone(),
        population: population.unwrap_or("inferred default").to_owned(),
    }
}

fn valid_build_points(
    prototype: &ProtoObject,
    technologies: Option<&PlayerTechState>,
) -> Result<f32, ConstructionError> {
    let base = prototype.build_points.unwrap_or_default();
    let points = technologies.map_or(base, |state| state.build_points(&prototype.name, base));
    if points.is_finite() && points >= 0.0 {
        Ok(points)
    } else {
        Err(ConstructionError::InvalidBuildPoints(
            prototype.name.clone(),
        ))
    }
}

fn find_train_limit(
    builder: &ProtoObject,
    definition: &ConstructionDefinition,
) -> Option<TrainLimitRule> {
    builder
        .train_limits
        .iter()
        .find(|limit| {
            limit
                .target
                .trim()
                .eq_ignore_ascii_case(&definition.prototype_name)
                && !matches!(limit.limit_type, Some(TrainLimitType::Squad))
        })
        .map(|limit| TrainLimitRule {
            count: u32::from(limit.count.unwrap_or(0)),
            bucket: limit.bucket,
        })
}

fn command_unavailable(
    builder_id: EntityId,
    kind: ConstructionKind,
    definition: &ConstructionDefinition,
) -> ConstructionError {
    ConstructionError::CommandUnavailable {
        builder_id,
        kind,
        prototype: definition.prototype_name.clone(),
    }
}

fn object_by_runtime_id(database: &Database, prototype_id: i32) -> Option<&ProtoObject> {
    usize::try_from(prototype_id)
        .ok()
        .and_then(|index| database.objects.get(index))
}

fn object_by_name<'a>(database: &'a Database, name: &str) -> Option<&'a ProtoObject> {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name.trim()))
}

fn is_building_proto(prototype: &ProtoObject) -> bool {
    prototype
        .object_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Building"))
        || prototype
            .select_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("Building"))
        || has_flag(&prototype.flags, "KBCreatesBase")
}

fn has_flag(flags: &[String], expected: &str) -> bool {
    flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}
