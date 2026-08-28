//! Immutable ability definitions joined to per-object command mappings.

use crate::entities::RecoveryType;
use pipeline::database::hw1::{Ability, Database};
use std::collections::BTreeMap;

/// Authored point at which an ability begins its recovery channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityRecoveryStart {
    /// Recovery begins when movement completes.
    Move,
    /// Recovery begins when the ability attack completes.
    Attack,
}

/// Immutable runtime subset of one database ability.
#[derive(Debug, Clone)]
pub struct AbilityGameplay {
    database_id: u8,
    name: String,
    recovery_start: Option<AbilityRecoveryStart>,
    recovery_type: Option<RecoveryType>,
    recovery_time: f32,
}

impl AbilityGameplay {
    /// Return the ability's wire/database index.
    #[must_use]
    pub const fn database_id(&self) -> u8 {
        self.database_id
    }

    /// Return the authored ability name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the authored recovery-start event.
    #[must_use]
    pub const fn recovery_start(&self) -> Option<AbilityRecoveryStart> {
        self.recovery_start
    }

    /// Return the recovery channel shared with other squad actions.
    #[must_use]
    pub const fn recovery_type(&self) -> Option<RecoveryType> {
        self.recovery_type
    }

    /// Return the unmodified database recovery duration in seconds.
    #[must_use]
    pub const fn recovery_time(&self) -> f32 {
        self.recovery_time
    }
}

pub(super) fn collect_abilities(database: &Database) -> Vec<AbilityGameplay> {
    database
        .abilities
        .iter()
        .enumerate()
        .filter_map(|(index, ability)| {
            Some(AbilityGameplay {
                database_id: u8::try_from(index).ok()?,
                name: ability.name.clone(),
                recovery_start: parse_recovery_start(ability.recover_start.as_deref()),
                recovery_type: ability
                    .recover_type
                    .as_deref()
                    .and_then(RecoveryType::from_authored),
                recovery_time: ability
                    .recover_time
                    .filter(|time| time.is_finite() && *time >= 0.0)
                    .unwrap_or_default(),
            })
        })
        .collect()
}

pub(super) fn command_ability_id(database: &Database) -> Option<u8> {
    ability_index(database, "Command")
}

pub(super) fn collect_object_ability_commands(database: &Database) -> BTreeMap<String, u8> {
    database
        .objects
        .iter()
        .filter_map(|object| {
            let ability = object.ability_command.as_deref()?;
            Some((
                object.name.to_ascii_lowercase(),
                ability_index(database, ability)?,
            ))
        })
        .collect()
}

pub(crate) fn resolve_database_ability<'database>(
    database: &'database Database,
    proto_object_name: &str,
    requested_id: u8,
) -> Option<(u8, &'database Ability)> {
    let actual_id = if ability_index(database, "Command") == Some(requested_id) {
        let command_name = database
            .objects
            .iter()
            .find(|object| object.name.eq_ignore_ascii_case(proto_object_name))?
            .ability_command
            .as_deref()?;
        ability_index(database, command_name)?
    } else {
        requested_id
    };
    Some((actual_id, database.abilities.get(usize::from(actual_id))?))
}

fn ability_index(database: &Database, name: &str) -> Option<u8> {
    database
        .abilities
        .iter()
        .position(|ability| ability.name.eq_ignore_ascii_case(name))
        .and_then(|index| u8::try_from(index).ok())
}

fn parse_recovery_start(value: Option<&str>) -> Option<AbilityRecoveryStart> {
    let value = value?;
    if value.eq_ignore_ascii_case("Move") {
        Some(AbilityRecoveryStart::Move)
    } else if value.eq_ignore_ascii_case("Attack") {
        Some(AbilityRecoveryStart::Attack)
    } else {
        None
    }
}
