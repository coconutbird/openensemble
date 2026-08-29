//! Retail-style technology eligibility, payment, and building research queues.

use super::{TechnologyError, World};
use crate::entities::units::TriggerCommandStateRef;
use crate::entities::{ResearchProgress, ResearchTask};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{MAX_RESOURCES, Player, PlayerId, Resources, TechStatus};
use pipeline::database::hw1::techs::{PrereqsWrapper, TechCost, TypeCountEntry};
use pipeline::database::hw1::{Database, Tech};

/// Result of accepting a positive retail research command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchQueueResult {
    Queued,
    CompletedInstantly,
}

/// Rejection produced while validating authoritative research work.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResearchError {
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    #[error("building {0:?} is not present in the world")]
    BuildingNotFound(EntityId),
    #[error("building {building_id:?} is not owned by player {player_id}")]
    BuildingNotOwned {
        building_id: EntityId,
        player_id: PlayerId,
    },
    #[error("technology runtime ID {0} was not found")]
    TechnologyNotFound(i32),
    #[error("building {building_id:?} does not offer technology '{technology}'")]
    CommandUnavailable {
        building_id: EntityId,
        technology: String,
    },
    #[error("technology '{technology}' has retail status {status:?}, not Available")]
    TechnologyUnavailable {
        technology: String,
        status: TechStatus,
    },
    #[error("technology '{0}' uses per-unit unique effects that are not modeled yet")]
    UniqueTechnologyUnsupported(String),
    #[error("shadow technology '{0}' must be activated by the tech tree, not researched manually")]
    ShadowTechnologyUnsupported(String),
    #[error("technology '{technology}' references unknown resource '{resource}'")]
    UnknownResource {
        technology: String,
        resource: String,
    },
    #[error("technology '{technology}' has invalid cost for resource '{resource}'")]
    InvalidCost {
        technology: String,
        resource: String,
    },
    #[error("technology '{0}' has costs but the database has no resource table")]
    MissingResourceTable(String),
    #[error("technology '{0}' has invalid research points")]
    InvalidResearchPoints(String),
    #[error("player {player_id} cannot afford technology '{technology}'")]
    InsufficientResources {
        player_id: PlayerId,
        technology: String,
    },
    #[error(transparent)]
    Activation(#[from] TechnologyError),
}

#[derive(Debug, Clone, Copy)]
struct ResearchQueueRequest<'database> {
    player_id: PlayerId,
    building_id: EntityId,
    database: &'database Database,
    technology_id: i32,
    no_cost: bool,
    trigger_state: Option<TriggerCommandStateRef>,
}

impl World {
    /// Derive one player's current retail status for a non-unique technology.
    ///
    /// # Errors
    ///
    /// Returns an error when the player or runtime technology ID is invalid.
    pub fn technology_status(
        &self,
        player_id: PlayerId,
        database: &Database,
        technology_id: i32,
    ) -> Result<TechStatus, ResearchError> {
        let player = self
            .get_player(player_id)
            .ok_or(ResearchError::PlayerNotFound(player_id))?;
        let technology = technology_by_id(database, technology_id)
            .ok_or(ResearchError::TechnologyNotFound(technology_id))?;
        Ok(self.derive_technology_status(player, database, technology_id, technology))
    }

    /// Read authoritative progress for a queued or active research item.
    ///
    /// # Errors
    ///
    /// Returns an error when the player or runtime technology ID is invalid.
    pub fn research_progress(
        &self,
        player_id: PlayerId,
        database: &Database,
        technology_id: i32,
    ) -> Result<Option<ResearchProgress>, ResearchError> {
        let player = self
            .get_player(player_id)
            .ok_or(ResearchError::PlayerNotFound(player_id))?;
        let _technology = technology_by_id(database, technology_id)
            .ok_or(ResearchError::TechnologyNotFound(technology_id))?;
        let Some(building_id) = player.research.research_building(technology_id) else {
            return Ok(None);
        };
        let progress = self
            .get_building(building_id)
            .and_then(|building| building.production.research_task(player_id, technology_id))
            .map(|(task, queued)| ResearchProgress {
                building_id,
                current_points: task.current_points,
                total_points: task.total_points,
                queued,
            });
        Ok(progress)
    }

    /// Validate, pay for, and enqueue exactly one technology item.
    ///
    /// Retail ignores a positive count greater than one for research, so this
    /// API likewise creates one queue entry per accepted command.
    ///
    /// # Errors
    ///
    /// Returns a detailed rejection without mutating resources or queues.
    pub fn queue_research(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        database: &Database,
        technology_id: i32,
    ) -> Result<ResearchQueueResult, ResearchError> {
        self.queue_research_internal(ResearchQueueRequest {
            player_id,
            building_id,
            database,
            technology_id,
            no_cost: false,
            trigger_state: None,
        })
    }

    pub(crate) fn queue_trigger_research(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        database: &Database,
        technology_id: i32,
        no_cost: bool,
        trigger_state: Option<TriggerCommandStateRef>,
    ) -> Result<ResearchQueueResult, ResearchError> {
        self.queue_research_internal(ResearchQueueRequest {
            player_id,
            building_id,
            database,
            technology_id,
            no_cost,
            trigger_state,
        })
    }

    fn queue_research_internal(
        &mut self,
        request: ResearchQueueRequest<'_>,
    ) -> Result<ResearchQueueResult, ResearchError> {
        let ResearchQueueRequest {
            player_id,
            building_id,
            database,
            technology_id,
            no_cost,
            trigger_state,
        } = request;
        let technology = technology_by_id(database, technology_id)
            .ok_or(ResearchError::TechnologyNotFound(technology_id))?;
        self.validate_research_command(player_id, building_id, database, technology)?;
        Self::validate_research_flags(technology)?;
        let status = self.technology_status(player_id, database, technology_id)?;
        if status != TechStatus::Available {
            return Err(ResearchError::TechnologyUnavailable {
                technology: technology.name.clone(),
                status,
            });
        }
        let total_points = research_points(technology)?;
        let cost = technology_cost(database, technology)?;
        self.pay_and_mark_research(
            player_id,
            building_id,
            technology_id,
            technology,
            &cost,
            no_cost,
        )?;
        let charged_cost = if no_cost { Resources::new() } else { cost };

        if has_flag(technology, "Instant") {
            let activation = self.activate_technology(player_id, database, &technology.name);
            self.finish_research_assignment(player_id, building_id, technology_id);
            if let Err(error) = activation {
                self.refund_cost(player_id, &charged_cost);
                return Err(error.into());
            }
            return Ok(ResearchQueueResult::CompletedInstantly);
        }

        let task = ResearchTask {
            player_id,
            technology_id,
            technology_name: technology.name.clone(),
            current_points: 0.0,
            total_points,
            cost: charged_cost,
            trigger_state,
        };
        let Some(building) = self.get_building_mut(building_id) else {
            self.finish_research_assignment(player_id, building_id, technology_id);
            self.refund_cost(player_id, &task.cost);
            return Err(ResearchError::BuildingNotFound(building_id));
        };
        building.production.enqueue_research(task);
        Ok(ResearchQueueResult::Queued)
    }

    /// Cancel and fully refund an outstanding technology item.
    ///
    /// The command may be issued to any owned building that offers the tech;
    /// the player-global assignment identifies the building doing the work.
    ///
    /// # Errors
    ///
    /// Returns an error when the command source or technology is invalid.
    pub fn cancel_research(
        &mut self,
        player_id: PlayerId,
        command_building_id: EntityId,
        database: &Database,
        technology_id: i32,
    ) -> Result<bool, ResearchError> {
        let technology = technology_by_id(database, technology_id)
            .ok_or(ResearchError::TechnologyNotFound(technology_id))?;
        self.validate_research_command(player_id, command_building_id, database, technology)?;
        let Some(research_building_id) = self
            .get_player(player_id)
            .and_then(|player| player.research.research_building(technology_id))
        else {
            return Ok(false);
        };
        let task = self
            .get_building_mut(research_building_id)
            .and_then(|building| {
                building
                    .production
                    .cancel_research(player_id, technology_id)
            });
        self.finish_research_assignment(player_id, research_building_id, technology_id);
        if let Some(task) = task {
            self.refund_cost(player_id, &task.cost);
            if let Some(trigger_state) = task.trigger_state {
                self.notify_building_command_task(trigger_state, None);
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn derive_technology_status(
        &self,
        player: &Player,
        database: &Database,
        technology_id: i32,
        technology: &Tech,
    ) -> TechStatus {
        if player.research.is_researching(technology_id) {
            return TechStatus::Researching;
        }
        if player.technologies.is_active(&technology.name) && !has_flag(technology, "Perpetual") {
            return TechStatus::Active;
        }
        if authored_unobtainable(technology) {
            return TechStatus::Unobtainable;
        }
        if player.is_technology_forbidden(database, technology_id) || technology.alpha == Some(1) {
            return TechStatus::Obtainable;
        }
        if prerequisites_met(self, player, database, technology) {
            TechStatus::Available
        } else {
            TechStatus::Obtainable
        }
    }

    fn validate_research_command(
        &self,
        player_id: PlayerId,
        building_id: EntityId,
        database: &Database,
        technology: &Tech,
    ) -> Result<(), ResearchError> {
        let player = self
            .get_player(player_id)
            .ok_or(ResearchError::PlayerNotFound(player_id))?;
        let building = self
            .get_building(building_id)
            .ok_or(ResearchError::BuildingNotFound(building_id))?;
        if building.base.player_id != player_id {
            return Err(ResearchError::BuildingNotOwned {
                building_id,
                player_id,
            });
        }
        if !building.is_operational() {
            return Err(ResearchError::CommandUnavailable {
                building_id,
                technology: technology.name.clone(),
            });
        }
        if !building_offers_research(database, building, technology, player) {
            return Err(ResearchError::CommandUnavailable {
                building_id,
                technology: technology.name.clone(),
            });
        }
        Ok(())
    }

    fn validate_research_flags(technology: &Tech) -> Result<(), ResearchError> {
        if has_flag(technology, "UniqueProtoUnitInstance") {
            return Err(ResearchError::UniqueTechnologyUnsupported(
                technology.name.clone(),
            ));
        }
        if has_flag(technology, "Shadow") {
            return Err(ResearchError::ShadowTechnologyUnsupported(
                technology.name.clone(),
            ));
        }
        Ok(())
    }

    fn pay_and_mark_research(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        technology_id: i32,
        technology: &Tech,
        cost: &Resources,
        no_cost: bool,
    ) -> Result<(), ResearchError> {
        let player = self
            .get_player_mut(player_id)
            .ok_or(ResearchError::PlayerNotFound(player_id))?;
        if !no_cost && !player.resources.can_afford(cost) {
            return Err(ResearchError::InsufficientResources {
                player_id,
                technology: technology.name.clone(),
            });
        }
        if !no_cost {
            player.resources.pay(cost);
        }
        let inserted = player.research.start(technology_id, building_id);
        debug_assert!(inserted, "status validation rejected duplicate research");
        Ok(())
    }

    pub(super) fn set_research_points(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        technology_id: i32,
        points: f32,
    ) {
        if let Some(player) = self.get_player_mut(player_id) {
            player
                .research
                .set_points(technology_id, building_id, points);
        }
    }

    pub(super) fn complete_research(
        &mut self,
        building_id: EntityId,
        task: &ResearchTask,
        database: &Database,
    ) {
        self.finish_research_assignment(task.player_id, building_id, task.technology_id);
        let activated = self
            .activate_technology(task.player_id, database, &task.technology_name)
            .unwrap_or(false);
        if !activated {
            self.refund_cost(task.player_id, &task.cost);
        }
    }

    pub(super) fn cancel_invalid_research_task(
        &mut self,
        building_id: EntityId,
        task: &ResearchTask,
    ) {
        self.finish_research_assignment(task.player_id, building_id, task.technology_id);
        self.refund_cost(task.player_id, &task.cost);
    }

    pub(super) fn finish_research_assignment(
        &mut self,
        player_id: PlayerId,
        building_id: EntityId,
        technology_id: i32,
    ) {
        if let Some(player) = self.get_player_mut(player_id) {
            player.research.stop(technology_id, building_id);
        }
    }

    pub(super) fn refund_cost(&mut self, player_id: PlayerId, cost: &Resources) {
        if let Some(player) = self.get_player_mut(player_id) {
            player.resources.refund(cost);
        }
    }
}

/// Resolve a technology's retail runtime table index by name.
#[must_use]
pub fn technology_prototype_id(database: &Database, name: &str) -> Option<i32> {
    database
        .techs
        .iter()
        .position(|technology| technology.name.eq_ignore_ascii_case(name.trim()))
        .and_then(|index| i32::try_from(index).ok())
}

pub(super) fn technology_by_id(database: &Database, technology_id: i32) -> Option<&Tech> {
    usize::try_from(technology_id)
        .ok()
        .and_then(|index| database.techs.get(index))
}

pub(super) fn research_points(technology: &Tech) -> Result<f32, ResearchError> {
    let points = technology.research_points.unwrap_or(0.0);
    if points.is_finite() && points >= 0.0 {
        Ok(points)
    } else {
        Err(ResearchError::InvalidResearchPoints(
            technology.name.clone(),
        ))
    }
}

fn technology_cost(database: &Database, technology: &Tech) -> Result<Resources, ResearchError> {
    if technology.costs.is_empty() {
        return Ok(Resources::new());
    }
    let resources = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .ok_or_else(|| ResearchError::MissingResourceTable(technology.name.clone()))?;
    let mut cost = Resources::new();
    for entry in &technology.costs {
        add_cost_entry(&mut cost, resources, technology, entry)?;
    }
    Ok(cost)
}

fn add_cost_entry(
    cost: &mut Resources,
    resources: &pipeline::database::hw1::gamedata::ResourcesWrapper,
    technology: &Tech,
    entry: &TechCost,
) -> Result<(), ResearchError> {
    if !entry.amount.is_finite() || entry.amount < 0.0 {
        return Err(ResearchError::InvalidCost {
            technology: technology.name.clone(),
            resource: entry.resource_type.clone(),
        });
    }
    let resource_id = resources
        .entries
        .iter()
        .position(|resource| {
            resource
                .name
                .eq_ignore_ascii_case(entry.resource_type.trim())
        })
        .filter(|index| *index < MAX_RESOURCES)
        .ok_or_else(|| ResearchError::UnknownResource {
            technology: technology.name.clone(),
            resource: entry.resource_type.clone(),
        })?;
    cost.add(resource_id, entry.amount);
    Ok(())
}

fn authored_unobtainable(technology: &Tech) -> bool {
    technology
        .status
        .as_deref()
        .is_some_and(|status| status.trim().eq_ignore_ascii_case("Unobtainable"))
        || has_flag(technology, "Unobtainable")
}

fn has_flag(technology: &Tech, expected: &str) -> bool {
    technology
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

fn prerequisites_met(
    world: &World,
    player: &Player,
    database: &Database,
    technology: &Tech,
) -> bool {
    let use_or = has_flag(technology, "OrPrereqs");
    match (&technology.prereqs, &technology.or_prereqs) {
        (None, None) => !use_or,
        (Some(primary), None) => prerequisite_group_met(world, player, database, primary, use_or),
        (None, Some(alternate)) => prerequisite_group_met(world, player, database, alternate, true),
        (Some(primary), Some(alternate)) => {
            prerequisite_group_met(world, player, database, primary, use_or)
                || prerequisite_group_met(world, player, database, alternate, true)
        }
    }
}

fn prerequisite_group_met(
    world: &World,
    player: &Player,
    database: &Database,
    prerequisites: &PrereqsWrapper,
    use_or: bool,
) -> bool {
    let tech_results = prerequisites.entries.iter().map(|entry| {
        let name = entry
            .text
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&entry.tech);
        entry.status.trim().eq_ignore_ascii_case("Active") && player.technologies.is_active(name)
    });
    let count_results = prerequisites
        .type_counts
        .iter()
        .map(|entry| type_count_met(world, player.id, database, entry));
    if use_or {
        tech_results.chain(count_results).any(|met| met)
    } else {
        tech_results.chain(count_results).all(|met| met)
    }
}

fn type_count_met(
    world: &World,
    player_id: PlayerId,
    database: &Database,
    prerequisite: &TypeCountEntry,
) -> bool {
    let unit_name = prerequisite.unit.trim();
    if !database
        .objects
        .iter()
        .any(|object| object.name.eq_ignore_ascii_case(unit_name))
    {
        return false;
    }
    let actual = world
        .units
        .iter()
        .filter(|(_, unit)| {
            unit.base.player_id == player_id
                && unit.is_alive()
                && unit.proto_object_name.eq_ignore_ascii_case(unit_name)
        })
        .count();
    let actual = i32::try_from(actual).unwrap_or(i32::MAX);
    let expected = prerequisite.count.unwrap_or(0);
    match prerequisite.operator.as_deref().map(str::trim) {
        Some(operator) if operator.eq_ignore_ascii_case("gt") => actual > expected,
        Some(operator) if operator.eq_ignore_ascii_case("lt") => actual < expected,
        _ => actual == expected,
    }
}

fn building_offers_research(
    database: &Database,
    building: &crate::entities::Unit,
    technology: &Tech,
    player: &Player,
) -> bool {
    let authored = database
        .objects
        .iter()
        .find(|proto| proto.name.eq_ignore_ascii_case(&building.proto_object_name))
        .is_some_and(|proto| {
            proto.commands.iter().any(|command| {
                command.target.trim().eq_ignore_ascii_case(&technology.name)
                    && command.command_type.as_deref().is_none_or(|command_type| {
                        command_type.trim().is_empty()
                            || command_type.trim().eq_ignore_ascii_case("Research")
                    })
            })
        });
    player.technologies.command_enabled(
        &building.proto_object_name,
        "Research",
        &technology.name,
        authored,
    )
}
