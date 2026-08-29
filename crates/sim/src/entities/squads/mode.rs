//! Retail squad-mode identities used by commands and tactic rules.

/// A squad's current behavior mode.
///
/// Discriminants match `BSquadAI` and the one-byte `BWorkCommand` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SquadMode {
    /// Ordinary movement and combat.
    #[default]
    Normal = 0,
    /// Hold position while continuing to engage.
    StandGround = 1,
    /// Deployed stationary weapon mode.
    Lockdown = 2,
    /// Sniper weapon mode.
    Sniper = 3,
    /// Vehicle hit-and-run mode.
    HitAndRun = 4,
    /// Suppress automatic combat opportunities.
    Passive = 5,
    /// Infantry cover mode.
    Cover = 6,
    /// Generic active-ability mode.
    Ability = 7,
    /// Carrying a scenario object.
    CarryingObject = 8,
    /// Leader-power mode.
    Power = 9,
    /// Scenario 07 Scarab scan mode.
    ScarabScan = 10,
    /// Scenario 07 Scarab target mode.
    ScarabTarget = 11,
    /// Scenario 07 Scarab kill mode.
    ScarabKill = 12,
}

impl SquadMode {
    /// Decode a database squad-mode spelling without case sensitivity.
    #[must_use]
    pub fn from_authored(value: &str) -> Option<Self> {
        (0..=12)
            .filter_map(Self::from_i32)
            .find(|mode| mode.as_str().eq_ignore_ascii_case(value.trim()))
    }

    /// Decode the numeric mode carried by a work command.
    #[must_use]
    pub const fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Normal),
            1 => Some(Self::StandGround),
            2 => Some(Self::Lockdown),
            3 => Some(Self::Sniper),
            4 => Some(Self::HitAndRun),
            5 => Some(Self::Passive),
            6 => Some(Self::Cover),
            7 => Some(Self::Ability),
            8 => Some(Self::CarryingObject),
            9 => Some(Self::Power),
            10 => Some(Self::ScarabScan),
            11 => Some(Self::ScarabTarget),
            12 => Some(Self::ScarabKill),
            _ => None,
        }
    }

    /// Return the database spelling consumed by tactic target rules.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::StandGround => "StandGround",
            Self::Lockdown => "Lockdown",
            Self::Sniper => "Sniper",
            Self::HitAndRun => "HitAndRun",
            Self::Passive => "Passive",
            Self::Cover => "Cover",
            Self::Ability => "Ability",
            Self::CarryingObject => "CarryingObject",
            Self::Power => "Power",
            Self::ScarabScan => "ScarabScan",
            Self::ScarabTarget => "ScarabTarget",
            Self::ScarabKill => "ScarabKill",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_values_round_trip_to_retail_names() {
        assert_eq!(
            SquadMode::from_i32(0).map(SquadMode::as_str),
            Some("Normal")
        );
        assert_eq!(SquadMode::from_i32(6).map(SquadMode::as_str), Some("Cover"));
        assert_eq!(
            SquadMode::from_i32(12).map(SquadMode::as_str),
            Some("ScarabKill")
        );
        assert_eq!(SquadMode::from_i32(13), None);
        assert_eq!(
            SquadMode::from_authored(" hitandrun "),
            Some(SquadMode::HitAndRun)
        );
        assert_eq!(SquadMode::from_authored("unknown"), None);
    }
}
