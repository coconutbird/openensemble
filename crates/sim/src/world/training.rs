//! Retail-style unit/squad training validation, payment, and cancellation.

use super::World;
use crate::entities::units::TriggerCommandStateRef;
use crate::entities::{TrainingKind, TrainingProgress, TrainingTask};
use crate::entity_id::EntityId;
use crate::player::{MAX_RESOURCES, PlayerId, PlayerTechState, PopulationCost, Resources};
use crate::scenario::population::{
    object_population_costs, object_population_type_id, squad_population_costs,
};
use pipeline::database::hw1::objects::{ProtoObject, TrainLimit, TrainLimitType};
use pipeline::database::hw1::{Database, Squad as ProtoSquad};

/// Maximum work items accepted from one training command.
pub const MAX_TRAIN_BATCH: u32 = 256;

/// Number of requested training items accepted by authoritative validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainingQueueResult {
    pub accepted: u32,
    pub requested: u32,
}

/// Rejection produced while validating authoritative building training.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrainingError {
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    #[error("building {0:?} is not present in the world")]
    BuildingNotFound(EntityId),
    #[error("building {building_id:?} is not owned by player {player_id}")]
    BuildingNotOwned {
        building_id: EntityId,
        player_id: PlayerId,
    },
    #[error("{kind:?} runtime prototype ID {prototype_id} was not found")]
    PrototypeNotFound {
        kind: TrainingKind,
        prototype_id: i32,
    },
    #[error("object training prototype '{0}' is not a standalone unit or building")]
    PrototypeNotSpawnable(String),
    #[error("building {building_id:?} does not offer {kind:?} '{prototype}'")]
    CommandUnavailable {
        building_id: EntityId,
        kind: TrainingKind,
        prototype: String,
    },
    #[error("training '{0}' has costs but the database has no resource table")]
    MissingResourceTable(String),
    #[error("training '{prototype}' references unknown resource '{resource}'")]
    UnknownResource { prototype: String, resource: String },
    #[error("training '{prototype}' has invalid cost for resource '{resource}'")]
    InvalidCost { prototype: String, resource: String },
    #[error("training '{0}' has invalid build points")]
    InvalidBuildPoints(String),
    #[error("training '{0}' has population but the database has no population table")]
    MissingPopulationTable(String),
    #[error("'{prototype}' references unknown population type '{population}'")]
    UnknownPopulation {
        prototype: String,
        population: String,
    },
    #[error("'{0}' uses InstantTrainWithRecharge, which is not modeled yet")]
    InstantRechargeUnsupported(String),
    #[error("'{0}' uses dynamic cost escalation, which is not modeled yet")]
    CostEscalationUnsupported(String),
    #[error("training count {count} exceeds the per-command limit of {MAX_TRAIN_BATCH}")]
    BatchTooLarge { count: u32 },
}

#[derive(Debug, Clone)]
struct TrainingDefinition {
    kind: TrainingKind,
    prototype_name: String,
    total_points: f32,
    cost: Resources,
    population_costs: Vec<PopulationCost>,
}

#[derive(Debug, Clone, Copy)]
struct TrainLimitRule {
    count: u32,
    bucket: Option<u8>,
}

#[derive(Debug, Clone, Copy)]
struct TrainingQueueRequest<'database> {
    player_id: PlayerId,
    building_id: EntityId,
    database: &'database Database,
    kind: TrainingKind,
    prototype_id: i32,
    count: u32,
    no_cost: bool,
    trigger_state: Option<TriggerCommandStateRef>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TriggerTrainingRequest<'database> {
    pub(crate) player_id: PlayerId,
    pub(crate) building_id: EntityId,
    pub(crate) database: &'database Database,
    pub(crate) prototype_id: i32,
    pub(crate) count: u32,
    pub(crate) no_cost: bool,
    pub(crate) trigger_state: Option<TriggerCommandStateRef>,
}

impl World {
    /// Validate, pay, reserve population, and enqueue as many items as possible.
    ///
    /// Retail accepts a prefix when resources, population, or a train limit
    /// prevents the entire requested count from being queued.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed data or an unavailable building command.
    pub fn queue_training(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        database: &Database,
        kind: TrainingKind,
        prototype_id: i32,
        count: u32,
    ) -> Result<TrainingQueueResult, TrainingError> {
        self.queue_training_internal(TrainingQueueRequest {
            player_id,
            building_id,
            database,
            kind,
            prototype_id,
            count,
            no_cost: false,
            trigger_state: None,
        })
    }

    pub(crate) fn queue_trigger_training(
        &mut self,
        request: TriggerTrainingRequest<'_>,
    ) -> Result<TrainingQueueResult, TrainingError> {
        let TriggerTrainingRequest {
            player_id,
            building_id,
            database,
            prototype_id,
            count,
            no_cost,
            trigger_state,
        } = request;
        self.queue_training_internal(TrainingQueueRequest {
            player_id,
            building_id,
            database,
            kind: TrainingKind::Squad,
            prototype_id,
            count,
            no_cost,
            trigger_state,
        })
    }

    fn queue_training_internal(
        &mut self,
        request: TrainingQueueRequest<'_>,
    ) -> Result<TrainingQueueResult, TrainingError> {
        let TrainingQueueRequest {
            player_id,
            building_id,
            database,
            kind,
            prototype_id,
            count,
            no_cost,
            trigger_state,
        } = request;
        if count > MAX_TRAIN_BATCH {
            return Err(TrainingError::BatchTooLarge { count });
        }
        let definition = training_definition(
            database,
            kind,
            prototype_id,
            self.get_player(player_id)
                .map(|player| &player.technologies),
        )?;
        let limit =
            self.validate_training_command(player_id, building_id, database, &definition)?;
        let mut accepted = 0;
        while accepted < count {
            if limit.is_some_and(|rule| {
                self.training_limit_count(building_id, &definition, rule) >= rule.count
            }) {
                break;
            }
            let Some(player) = self.get_player(player_id) else {
                return Err(TrainingError::PlayerNotFound(player_id));
            };
            if (!no_cost && !player.resources.can_afford(&definition.cost))
                || !player.can_reserve_population(&definition.population_costs)
            {
                break;
            }
            if let Some(player) = self.get_player_mut(player_id) {
                if !no_cost {
                    player.resources.pay(&definition.cost);
                }
                let reserved = player.reserve_population(&definition.population_costs);
                debug_assert!(reserved, "immutable population check just succeeded");
            }
            let task = TrainingTask {
                player_id,
                kind,
                prototype_id,
                prototype_name: definition.prototype_name.clone(),
                current_points: 0.0,
                total_points: definition.total_points,
                cost: if no_cost {
                    Resources::new()
                } else {
                    definition.cost
                },
                population_costs: definition.population_costs.clone(),
                train_limit_bucket: limit.and_then(|rule| rule.bucket),
                trigger_state,
            };
            let Some(building) = self.get_building_mut(building_id) else {
                self.refund_training_task(&task);
                return Err(TrainingError::BuildingNotFound(building_id));
            };
            building.production.enqueue_training(task);
            accepted += 1;
        }
        Ok(TrainingQueueResult {
            accepted,
            requested: count,
        })
    }

    /// Cancel matching queued items from the tail, then the current item.
    ///
    /// Returns the number of items canceled and fully refunded.
    ///
    /// # Errors
    ///
    /// Returns an error when the source building or runtime prototype is invalid.
    pub fn cancel_training(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        database: &Database,
        kind: TrainingKind,
        prototype_id: i32,
        count: u32,
    ) -> Result<u32, TrainingError> {
        let definition = training_definition(
            database,
            kind,
            prototype_id,
            self.get_player(player_id)
                .map(|player| &player.technologies),
        )?;
        let _limit =
            self.validate_training_command(player_id, building_id, database, &definition)?;
        let canceled = self
            .get_building_mut(building_id)
            .ok_or(TrainingError::BuildingNotFound(building_id))?
            .production
            .cancel_training(player_id, kind, prototype_id, count);
        let canceled_count = u32::try_from(canceled.len()).unwrap_or(u32::MAX);
        for task in &canceled {
            self.refund_training_task(task);
            if let Some(trigger_state) = task.trigger_state {
                self.notify_building_command_task(trigger_state, None);
            }
        }
        Ok(canceled_count)
    }

    /// Read authoritative progress for one item on one building.
    ///
    /// # Errors
    ///
    /// Returns an error when the player, building, or prototype is invalid.
    pub fn training_progress(
        &self,
        player_id: PlayerId,
        building_id: EntityId,
        database: &Database,
        kind: TrainingKind,
        prototype_id: i32,
    ) -> Result<Option<TrainingProgress>, TrainingError> {
        let definition = training_definition(
            database,
            kind,
            prototype_id,
            self.get_player(player_id)
                .map(|player| &player.technologies),
        )?;
        let _limit =
            self.validate_training_command(player_id, building_id, database, &definition)?;
        Ok(self
            .get_building(building_id)
            .and_then(|building| {
                building
                    .production
                    .training_task(player_id, kind, prototype_id)
            })
            .map(|(task, queued)| TrainingProgress {
                building_id,
                kind,
                prototype_id,
                current_points: task.current_points,
                total_points: task.total_points,
                queued,
            }))
    }

    pub(super) fn refund_training_task(&mut self, task: &TrainingTask) {
        if let Some(player) = self.get_player_mut(task.player_id) {
            player.resources.refund(&task.cost);
            player.release_reserved_population(&task.population_costs);
        }
    }

    fn validate_training_command(
        &self,
        player_id: PlayerId,
        building_id: EntityId,
        database: &Database,
        definition: &TrainingDefinition,
    ) -> Result<Option<TrainLimitRule>, TrainingError> {
        let player = self
            .get_player(player_id)
            .ok_or(TrainingError::PlayerNotFound(player_id))?;
        let building = self
            .get_building(building_id)
            .ok_or(TrainingError::BuildingNotFound(building_id))?;
        if building.base.player_id != player_id {
            return Err(TrainingError::BuildingNotOwned {
                building_id,
                player_id,
            });
        }
        if !building.is_operational() {
            return Err(TrainingError::CommandUnavailable {
                building_id,
                kind: definition.kind,
                prototype: definition.prototype_name.clone(),
            });
        }
        let Some(building_prototype) = database.objects.iter().find(|prototype| {
            prototype
                .name
                .eq_ignore_ascii_case(&building.proto_object_name)
        }) else {
            return Err(TrainingError::CommandUnavailable {
                building_id,
                kind: definition.kind,
                prototype: definition.prototype_name.clone(),
            });
        };
        let authored = building_prototype.commands.iter().any(|command| {
            command
                .target
                .eq_ignore_ascii_case(&definition.prototype_name)
                && command_kind_matches(
                    database,
                    definition.kind,
                    &definition.prototype_name,
                    command.command_type.as_deref(),
                )
        });
        let enabled = player.technologies.command_enabled(
            &building.proto_object_name,
            definition.kind.command_name(),
            &definition.prototype_name,
            authored,
        );
        if !enabled {
            return Err(TrainingError::CommandUnavailable {
                building_id,
                kind: definition.kind,
                prototype: definition.prototype_name.clone(),
            });
        }
        Ok(find_train_limit(database, building_prototype, definition))
    }

    fn training_limit_count(
        &self,
        building_id: EntityId,
        definition: &TrainingDefinition,
        rule: TrainLimitRule,
    ) -> u32 {
        let matches = |kind, prototype_name: &str, bucket| {
            if let Some(expected_bucket) = rule.bucket {
                bucket == Some(expected_bucket)
            } else {
                kind == definition.kind
                    && prototype_name.eq_ignore_ascii_case(&definition.prototype_name)
            }
        };
        let queued = self.get_building(building_id).map_or(0, |building| {
            building
                .production
                .training_tasks()
                .filter(|task| matches(task.kind, &task.prototype_name, task.train_limit_bucket))
                .count()
        });
        let live_squads = self
            .squads
            .iter()
            .filter(|(_, squad)| {
                squad.trained_by == Some(building_id)
                    && matches(
                        TrainingKind::Squad,
                        &squad.proto_squad_name,
                        squad.train_limit_bucket,
                    )
            })
            .count();
        let live_units = self
            .units
            .iter()
            .filter(|(_, unit)| {
                unit.trained_by == Some(building_id)
                    && matches(
                        TrainingKind::Unit,
                        &unit.proto_object_name,
                        unit.train_limit_bucket,
                    )
            })
            .count();
        u32::try_from(queued + live_squads + live_units).unwrap_or(u32::MAX)
    }
}

/// Resolve a squad's retail runtime table index by name.
#[must_use]
pub fn squad_runtime_id(database: &Database, name: &str) -> Option<i32> {
    database
        .squads
        .iter()
        .position(|prototype| prototype.name.eq_ignore_ascii_case(name.trim()))
        .and_then(|index| i32::try_from(index).ok())
}

/// Resolve an object's retail runtime table index by name.
#[must_use]
pub fn object_runtime_id(database: &Database, name: &str) -> Option<i32> {
    database
        .objects
        .iter()
        .position(|prototype| prototype.name.eq_ignore_ascii_case(name.trim()))
        .and_then(|index| i32::try_from(index).ok())
}

pub(super) fn training_points(
    database: &Database,
    kind: TrainingKind,
    prototype_id: i32,
    technologies: Option<&PlayerTechState>,
) -> Option<f32> {
    let points = match kind {
        TrainingKind::Unit => {
            let prototype = object_by_runtime_id(database, prototype_id)?;
            let base = prototype.build_points?;
            technologies.map_or(base, |state| state.build_points(&prototype.name, base))
        }
        TrainingKind::Squad => squad_by_runtime_id(database, prototype_id)?.build_points?,
    };
    (points.is_finite() && points >= 0.0).then_some(points)
}

fn training_definition(
    database: &Database,
    kind: TrainingKind,
    prototype_id: i32,
    technologies: Option<&PlayerTechState>,
) -> Result<TrainingDefinition, TrainingError> {
    match kind {
        TrainingKind::Unit => unit_training_definition(database, prototype_id, technologies),
        TrainingKind::Squad => squad_training_definition(database, prototype_id),
    }
}

fn unit_training_definition(
    database: &Database,
    prototype_id: i32,
    technologies: Option<&PlayerTechState>,
) -> Result<TrainingDefinition, TrainingError> {
    let prototype =
        object_by_runtime_id(database, prototype_id).ok_or(TrainingError::PrototypeNotFound {
            kind: TrainingKind::Unit,
            prototype_id,
        })?;
    if has_flag(&prototype.flags, "InstantTrainWithRecharge") {
        return Err(TrainingError::InstantRechargeUnsupported(
            prototype.name.clone(),
        ));
    }
    if !object_is_spawnable(prototype) {
        return Err(TrainingError::PrototypeNotSpawnable(prototype.name.clone()));
    }
    if prototype.cost_escalation.is_some() || !prototype.cost_escalation_object.is_empty() {
        return Err(TrainingError::CostEscalationUnsupported(
            prototype.name.clone(),
        ));
    }
    validate_object_population(database, prototype)?;
    Ok(TrainingDefinition {
        kind: TrainingKind::Unit,
        prototype_name: prototype.name.clone(),
        total_points: valid_unit_build_points(prototype, technologies)?,
        cost: training_cost(
            database,
            &prototype.name,
            prototype
                .costs
                .iter()
                .map(|cost| (cost.resource_type.as_str(), cost.amount)),
        )?,
        population_costs: object_population_costs(database, prototype),
    })
}

fn squad_training_definition(
    database: &Database,
    prototype_id: i32,
) -> Result<TrainingDefinition, TrainingError> {
    let prototype =
        squad_by_runtime_id(database, prototype_id).ok_or(TrainingError::PrototypeNotFound {
            kind: TrainingKind::Squad,
            prototype_id,
        })?;
    if has_flag(&prototype.flags, "InstantTrainWithRecharge") {
        return Err(TrainingError::InstantRechargeUnsupported(
            prototype.name.clone(),
        ));
    }
    for member in prototype
        .units
        .as_ref()
        .map_or(&[][..], |units| units.entries.as_slice())
    {
        if let Some(object) = database
            .objects
            .iter()
            .find(|object| object.name.eq_ignore_ascii_case(member.proto_object.trim()))
        {
            validate_object_population(database, object)?;
        }
    }
    Ok(TrainingDefinition {
        kind: TrainingKind::Squad,
        prototype_name: prototype.name.clone(),
        total_points: valid_build_points(&prototype.name, prototype.build_points)?,
        cost: training_cost(
            database,
            &prototype.name,
            prototype
                .costs
                .iter()
                .map(|cost| (cost.resource_type.as_str(), cost.amount)),
        )?,
        population_costs: squad_population_costs(database, prototype),
    })
}

fn valid_build_points(name: &str, points: Option<f32>) -> Result<f32, TrainingError> {
    let points = points.unwrap_or(0.0);
    if points.is_finite() && points >= 0.0 {
        Ok(points)
    } else {
        Err(TrainingError::InvalidBuildPoints(name.to_owned()))
    }
}

fn valid_unit_build_points(
    prototype: &ProtoObject,
    technologies: Option<&PlayerTechState>,
) -> Result<f32, TrainingError> {
    let base = prototype.build_points.unwrap_or_default();
    let points = technologies.map_or(base, |state| state.build_points(&prototype.name, base));
    valid_build_points(&prototype.name, Some(points))
}

fn training_cost<'a>(
    database: &Database,
    prototype: &str,
    entries: impl Iterator<Item = (&'a str, f32)>,
) -> Result<Resources, TrainingError> {
    let entries = entries.collect::<Vec<_>>();
    if entries.is_empty() {
        return Ok(Resources::new());
    }
    let resources = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .ok_or_else(|| TrainingError::MissingResourceTable(prototype.to_owned()))?;
    let mut cost = Resources::new();
    for (resource, amount) in entries {
        if !amount.is_finite() || amount < 0.0 {
            return Err(TrainingError::InvalidCost {
                prototype: prototype.to_owned(),
                resource: resource.to_owned(),
            });
        }
        let resource_id = resources
            .entries
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(resource.trim()))
            .filter(|index| *index < MAX_RESOURCES)
            .ok_or_else(|| TrainingError::UnknownResource {
                prototype: prototype.to_owned(),
                resource: resource.to_owned(),
            })?;
        cost.add(resource_id, amount);
    }
    Ok(cost)
}

fn validate_object_population(
    database: &Database,
    prototype: &ProtoObject,
) -> Result<(), TrainingError> {
    for amount in &prototype.population {
        if !amount.amount.is_finite() || amount.amount < 0.0 {
            return Err(TrainingError::UnknownPopulation {
                prototype: prototype.name.clone(),
                population: amount
                    .population_type
                    .clone()
                    .unwrap_or_else(|| "inferred default".to_owned()),
            });
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
            return Err(TrainingError::MissingPopulationTable(
                prototype.name.clone(),
            ));
        }
        if object_population_type_id(database, prototype, amount).is_none() {
            return Err(TrainingError::UnknownPopulation {
                prototype: prototype.name.clone(),
                population: amount
                    .population_type
                    .clone()
                    .unwrap_or_else(|| "inferred default".to_owned()),
            });
        }
    }
    Ok(())
}

fn find_train_limit(
    database: &Database,
    building: &ProtoObject,
    definition: &TrainingDefinition,
) -> Option<TrainLimitRule> {
    building
        .train_limits
        .iter()
        .find(|limit| train_limit_matches(database, limit, definition))
        .map(|limit| TrainLimitRule {
            count: u32::from(limit.count.unwrap_or(0)),
            bucket: limit.bucket,
        })
}

fn train_limit_matches(
    database: &Database,
    limit: &TrainLimit,
    definition: &TrainingDefinition,
) -> bool {
    limit
        .target
        .trim()
        .eq_ignore_ascii_case(&definition.prototype_name)
        && match limit.limit_type {
            Some(TrainLimitType::Unit) => definition.kind == TrainingKind::Unit,
            Some(TrainLimitType::Squad) => definition.kind == TrainingKind::Squad,
            None => {
                command_kind_matches(database, definition.kind, &definition.prototype_name, None)
            }
        }
}

fn command_kind_matches(
    database: &Database,
    kind: TrainingKind,
    target: &str,
    authored: Option<&str>,
) -> bool {
    if let Some(authored) = authored.map(str::trim).filter(|value| !value.is_empty()) {
        return authored.eq_ignore_ascii_case(kind.command_name());
    }
    let is_squad = database
        .squads
        .iter()
        .any(|prototype| prototype.name.eq_ignore_ascii_case(target));
    match kind {
        TrainingKind::Squad => is_squad,
        TrainingKind::Unit => {
            !is_squad
                && database
                    .objects
                    .iter()
                    .any(|prototype| prototype.name.eq_ignore_ascii_case(target))
        }
    }
}

fn object_by_runtime_id(database: &Database, prototype_id: i32) -> Option<&ProtoObject> {
    usize::try_from(prototype_id)
        .ok()
        .and_then(|index| database.objects.get(index))
}

fn squad_by_runtime_id(database: &Database, prototype_id: i32) -> Option<&ProtoSquad> {
    usize::try_from(prototype_id)
        .ok()
        .and_then(|index| database.squads.get(index))
}

fn has_flag(flags: &[String], expected: &str) -> bool {
    flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

fn object_is_spawnable(prototype: &ProtoObject) -> bool {
    prototype.object_class.as_deref().is_some_and(|class| {
        class.eq_ignore_ascii_case("Unit") || class.eq_ignore_ascii_case("Building")
    }) || prototype
        .select_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Building"))
        || has_flag(&prototype.flags, "KBCreatesBase")
}
