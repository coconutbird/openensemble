//! Immutable ability definitions joined to per-object command mappings.

use crate::entities::{RecoveryType, SquadMode};
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
    ability_type: Option<String>,
    target_type: Option<String>,
    objects: Vec<String>,
    ammunition_cost: f32,
    squad_mode: Option<SquadMode>,
    keep_squad_mode: bool,
    recovery_start: Option<AbilityRecoveryStart>,
    recovery_type: Option<RecoveryType>,
    recovery_time: f32,
    duration: f32,
    damage_taken_modifier: f32,
    dodge_modifier: f32,
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

    /// Return the authored ability category, such as `Work`.
    #[must_use]
    pub fn ability_type(&self) -> Option<&str> {
        self.ability_type.as_deref()
    }

    /// Return the authored target category, such as `Location`.
    #[must_use]
    pub fn target_type(&self) -> Option<&str> {
        self.target_type.as_deref()
    }

    /// Return the ability's authored proto-object list in database order.
    #[must_use]
    pub fn objects(&self) -> &[String] {
        &self.objects
    }

    /// Return the finite, nonnegative ammunition charged per use.
    #[must_use]
    pub const fn ammunition_cost(&self) -> f32 {
        self.ammunition_cost
    }

    /// Return the behavior mode entered while this ability executes.
    #[must_use]
    pub const fn squad_mode(&self) -> Option<SquadMode> {
        self.squad_mode
    }

    /// Return whether the authored mode remains after ability completion.
    #[must_use]
    pub const fn keeps_squad_mode(&self) -> bool {
        self.keep_squad_mode
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

    /// Return the authored active lifetime in seconds.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.duration
    }

    /// Return the incoming-damage scalar applied while the ability is active.
    ///
    /// Zero means that the ability does not adjust the live unit scalar.
    #[must_use]
    pub const fn damage_taken_modifier(&self) -> f32 {
        self.damage_taken_modifier
    }

    /// Return the dodge scalar applied while the ability is active.
    ///
    /// Zero means that the ability does not adjust the live unit scalar.
    #[must_use]
    pub const fn dodge_modifier(&self) -> f32 {
        self.dodge_modifier
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
                ability_type: ability.ability_type.clone(),
                target_type: ability.target_type.clone(),
                objects: ability.objects.clone(),
                ammunition_cost: ability
                    .ammo_cost
                    .filter(|cost| cost.is_finite() && *cost >= 0.0)
                    .unwrap_or_default(),
                squad_mode: ability
                    .squad_mode
                    .as_deref()
                    .and_then(SquadMode::from_authored),
                keep_squad_mode: ability.keep_squad_mode.unwrap_or(false),
                recovery_start: parse_recovery_start(ability.recover_start.as_deref()),
                recovery_type: ability
                    .recover_type
                    .as_deref()
                    .and_then(RecoveryType::from_authored),
                recovery_time: ability
                    .recover_time
                    .filter(|time| time.is_finite() && *time >= 0.0)
                    .unwrap_or_default(),
                duration: ability
                    .duration
                    .filter(|time| time.is_finite() && *time >= 0.0)
                    .unwrap_or_default(),
                damage_taken_modifier: finite_modifier(ability.damage_taken_modifier),
                dodge_modifier: finite_modifier(ability.dodge_modifier),
            })
        })
        .collect()
}

fn finite_modifier(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
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
