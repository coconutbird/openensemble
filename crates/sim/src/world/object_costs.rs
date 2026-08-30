//! Authoritative retail object-cost calculation, including dynamic escalation.

use super::World;
use crate::entities::TrainingKind;
use crate::entity::Entity;
use crate::player::{MAX_RESOURCES, PlayerId, Resources};
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::objects::ProtoObject;

/// Rejection produced while resolving one player's current object cost.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjectCostError {
    #[error("player {0} is not present in the world")]
    PlayerNotFound(PlayerId),
    #[error("object runtime prototype ID {0} was not found")]
    PrototypeNotFound(i32),
    #[error("object '{0}' has costs but the database has no resource table")]
    MissingResourceTable(String),
    #[error("object '{prototype}' references unknown resource '{resource}'")]
    UnknownResource { prototype: String, resource: String },
    #[error("object '{prototype}' has invalid cost for resource '{resource}'")]
    InvalidCost { prototype: String, resource: String },
    #[error("object '{0}' has invalid cost escalation")]
    InvalidEscalation(String),
}

impl World {
    /// Return the next retail price for an object using live and future counts.
    ///
    /// This is the authoritative quote consumed by construction, standalone
    /// unit training, and presentation clients. It includes scenario-layered
    /// escalation data and the configured co-op partner for buildings.
    ///
    /// # Errors
    ///
    /// Returns an error when the player or prototype is absent, or when the
    /// scenario-layered cost/resource data is malformed.
    pub fn object_cost(
        &self,
        database: &Database,
        player_id: PlayerId,
        prototype_id: i32,
    ) -> Result<Resources, ObjectCostError> {
        let prototype = runtime_object(database, prototype_id)
            .ok_or(ObjectCostError::PrototypeNotFound(prototype_id))?;
        self.object_cost_for_prototype(database, player_id, prototype, 0)
    }

    pub(super) fn object_cost_for_prototype(
        &self,
        database: &Database,
        player_id: PlayerId,
        prototype: &ProtoObject,
        count_adjustment: i32,
    ) -> Result<Resources, ObjectCostError> {
        let _player = self
            .get_player(player_id)
            .ok_or(ObjectCostError::PlayerNotFound(player_id))?;
        let base = base_object_cost(database, prototype)?;
        escalate_cost(self, player_id, prototype, base, count_adjustment)
    }
}

fn base_object_cost(
    database: &Database,
    prototype: &ProtoObject,
) -> Result<Resources, ObjectCostError> {
    if prototype.costs.is_empty() {
        return Ok(Resources::new());
    }
    let resources = database
        .game_data
        .as_ref()
        .and_then(|game_data| game_data.resources.as_ref())
        .ok_or_else(|| ObjectCostError::MissingResourceTable(prototype.name.clone()))?;
    let mut cost = Resources::new();
    for entry in &prototype.costs {
        if !entry.amount.is_finite() || entry.amount < 0.0 {
            return Err(ObjectCostError::InvalidCost {
                prototype: prototype.name.clone(),
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
            .ok_or_else(|| ObjectCostError::UnknownResource {
                prototype: prototype.name.clone(),
                resource: entry.resource_type.clone(),
            })?;
        cost.add(resource_id, entry.amount);
    }
    Ok(cost)
}

fn escalate_cost(
    world: &World,
    player_id: PlayerId,
    prototype: &ProtoObject,
    mut cost: Resources,
    count_adjustment: i32,
) -> Result<Resources, ObjectCostError> {
    let linear = has_flag(prototype, "LinearCostEscalation");
    let escalation = prototype
        .cost_escalation
        .unwrap_or(if linear { 0.0 } else { 1.0 });
    if !escalation.is_finite() || escalation < 0.0 {
        return Err(ObjectCostError::InvalidEscalation(prototype.name.clone()));
    }
    let unchanged = if linear {
        escalation.abs().to_bits() == 0.0_f32.to_bits()
    } else {
        escalation.to_bits() == 1.0_f32.to_bits()
    };
    if unchanged {
        return Ok(cost);
    }

    let existing = escalation_count(world, player_id, prototype, count_adjustment);
    if existing == 0 {
        return Ok(cost);
    }
    if linear {
        let addition = escalation * existing.to_f32().unwrap_or(f32::MAX);
        for amount in &mut cost.amounts {
            if *amount != 0.0 {
                *amount += addition;
            }
        }
    } else {
        for _ in 0..existing {
            for amount in &mut cost.amounts {
                *amount *= escalation;
            }
        }
        for amount in &mut cost.amounts {
            if *amount != 0.0 {
                *amount = (*amount * 0.1).round() * 10.0;
            }
        }
    }
    Ok(cost)
}

fn escalation_count(
    world: &World,
    player_id: PlayerId,
    prototype: &ProtoObject,
    count_adjustment: i32,
) -> u32 {
    let mut count = i64::from(count_adjustment);
    let mut players = vec![player_id];
    if world.is_coop()
        && prototype
            .object_class
            .as_deref()
            .is_some_and(|class| class.eq_ignore_ascii_case("Building"))
        && let Some(partner_id) = world
            .get_player(player_id)
            .and_then(crate::player::Player::coop_player_id)
        && partner_id != player_id
        && world.get_player(partner_id).is_some()
    {
        players.push(partner_id);
    }
    let names = if prototype.cost_escalation_object.is_empty() {
        std::slice::from_ref(&prototype.name)
    } else {
        prototype.cost_escalation_object.as_slice()
    };
    for counted_player in players {
        for name in names {
            count =
                count.saturating_add(i64::from(player_object_count(world, counted_player, name)));
        }
    }
    u32::try_from(count.max(0)).unwrap_or(u32::MAX)
}

fn player_object_count(world: &World, player_id: PlayerId, prototype_name: &str) -> u32 {
    let live = world.units.iter().filter(|(_, unit)| {
        unit.base.player_id == player_id
            && unit.is_alive()
            && unit.proto_object_name.eq_ignore_ascii_case(prototype_name)
    });
    let future = world.units.iter().filter_map(|(_, producer)| {
        producer
            .is_alive()
            .then_some(&producer.production)
            .map(|production| {
                let training = production.training_tasks().filter(|task| {
                    task.player_id() == player_id
                        && task.kind() == TrainingKind::Unit
                        && task.prototype_name().eq_ignore_ascii_case(prototype_name)
                });
                let construction = production.construction_tasks().filter(|task| {
                    task.purchasing_player_id() == player_id
                        && task.target_building_id().is_none()
                        && task.prototype_name().eq_ignore_ascii_case(prototype_name)
                });
                training.count() + construction.count()
            })
    });
    u32::try_from(live.count().saturating_add(future.sum::<usize>())).unwrap_or(u32::MAX)
}

fn runtime_object(database: &Database, prototype_id: i32) -> Option<&ProtoObject> {
    usize::try_from(prototype_id)
        .ok()
        .and_then(|index| database.objects.get(index))
}

fn has_flag(prototype: &ProtoObject, expected: &str) -> bool {
    prototype
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(expected))
}
