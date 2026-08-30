//! Persistent retail infection action and victim lifecycle state.

use super::Unit;
use crate::entity_id::EntityId;
use crate::player::PlayerId;
use crate::sync::SyncChecksum;

/// Observable phase of a unit being converted by a retail `Infect` action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum InfectionPhase {
    /// The unit is not undergoing infection.
    #[default]
    None = 0,
    /// Infection set hit points to zero; the next pre-async pass will handle death.
    Marked = 1,
    /// The same unit ID has taken its infected form under temporary Gaia control.
    Transforming = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InfectionVisual {
    pub(crate) unit_id: EntityId,
    pub(crate) attachment_id: EntityId,
}

/// One target squad retained by a persistent retail `Infect` action.
#[derive(Debug, Clone, PartialEq)]
pub struct InfectionExposure {
    pub(crate) squad_id: EntityId,
    pub(crate) elapsed_seconds: f32,
    pub(crate) combat_value_bank: f32,
    pub(crate) visuals: Vec<InfectionVisual>,
}

impl InfectionExposure {
    /// Target squad retained even after it leaves the infector's scan radius.
    #[must_use]
    pub const fn squad_id(&self) -> EntityId {
        self.squad_id
    }

    /// Time accumulated since this squad was first exposed.
    #[must_use]
    pub const fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    /// Combat-value work currently banked for this squad.
    #[must_use]
    pub const fn combat_value_bank(&self) -> f32 {
        self.combat_value_bank
    }

    /// Number of live attachment references retained by the action.
    #[must_use]
    pub const fn visual_count(&self) -> usize {
        self.visuals.len()
    }
}

/// All persistent state owned by one unit's infection action and victim flags.
#[derive(Debug, Clone, Default)]
pub(crate) struct UnitInfection {
    pub(crate) source_proto_name: String,
    pub(crate) action_name: String,
    pub(crate) infected_count: u32,
    pub(crate) time_until_next_scan: f32,
    pub(crate) exposures: Vec<InfectionExposure>,
    phase: InfectionPhase,
    infection_player_id: Option<PlayerId>,
}

impl UnitInfection {
    pub(crate) fn action_matches(&self, proto_name: &str, action_name: &str) -> bool {
        self.source_proto_name.eq_ignore_ascii_case(proto_name)
            && self.action_name.eq_ignore_ascii_case(action_name)
    }

    pub(crate) fn connect_action(&mut self, proto_name: &str, action_name: &str) {
        proto_name.clone_into(&mut self.source_proto_name);
        action_name.clone_into(&mut self.action_name);
        self.infected_count = 0;
        self.time_until_next_scan = 0.0;
        self.exposures.clear();
    }

    pub(crate) fn disconnect_action(&mut self) -> Vec<EntityId> {
        let attachment_ids = self
            .exposures
            .drain(..)
            .flat_map(|exposure| exposure.visuals)
            .map(|visual| visual.attachment_id)
            .collect();
        self.source_proto_name.clear();
        self.action_name.clear();
        self.infected_count = 0;
        self.time_until_next_scan = 0.0;
        attachment_ids
    }

    fn mark(&mut self, player_id: PlayerId) {
        self.phase = InfectionPhase::Marked;
        self.infection_player_id = Some(player_id);
    }

    fn begin_transform(&mut self, player_id: PlayerId) {
        self.phase = InfectionPhase::Transforming;
        self.infection_player_id = Some(player_id);
    }

    fn clear_victim(&mut self) {
        self.phase = InfectionPhase::None;
        self.infection_player_id = None;
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_string(checksum, &self.source_proto_name);
        hash_string(checksum, &self.action_name);
        checksum.hash_u32(self.infected_count);
        checksum.hash_f32(self.time_until_next_scan);
        checksum.hash_u32(u32::try_from(self.exposures.len()).unwrap_or(u32::MAX));
        for exposure in &self.exposures {
            checksum.hash_u32(exposure.squad_id.as_u32());
            checksum.hash_f32(exposure.elapsed_seconds);
            checksum.hash_f32(exposure.combat_value_bank);
            checksum.hash_u32(u32::try_from(exposure.visuals.len()).unwrap_or(u32::MAX));
            for visual in &exposure.visuals {
                checksum.hash_u32(visual.unit_id.as_u32());
                checksum.hash_u32(visual.attachment_id.as_u32());
            }
        }
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.infection_player_id.map_or(u32::MAX, u32::from));
    }
}

impl Unit {
    /// Return the current victim-side infection lifecycle phase.
    #[must_use]
    pub const fn infection_phase(&self) -> InfectionPhase {
        self.infection.phase
    }

    /// Return the player that will receive this unit after conversion.
    #[must_use]
    pub const fn infection_player_id(&self) -> Option<PlayerId> {
        self.infection.infection_player_id
    }

    /// Return how many units this unit's persistent action has infected.
    #[must_use]
    pub const fn infected_count(&self) -> u32 {
        self.infection.infected_count
    }

    /// Return target squads currently retained by this unit's infection action.
    #[must_use]
    pub fn infection_exposures(&self) -> &[InfectionExposure] {
        &self.infection.exposures
    }

    /// Return whether infection currently suppresses normal unit behavior.
    #[must_use]
    pub const fn is_undergoing_infection(&self) -> bool {
        !matches!(self.infection.phase, InfectionPhase::None)
    }

    pub(crate) fn mark_for_infection(&mut self, player_id: PlayerId) {
        self.infection.mark(player_id);
        self.hitpoints = 0.0;
        self.cancel_for_incapacitation();
    }

    pub(crate) fn begin_infection_transform(&mut self, player_id: PlayerId) {
        self.infection.begin_transform(player_id);
    }

    pub(crate) fn clear_infection_victim(&mut self) {
        self.infection.clear_victim();
    }

    pub(crate) fn hash_infection_state(&self, checksum: &mut SyncChecksum) {
        self.infection.hash_state(checksum);
    }
}

fn hash_string(checksum: &mut SyncChecksum, value: &str) {
    checksum.hash_u32(u32::try_from(value.len()).unwrap_or(u32::MAX));
    checksum.hash_bytes(value.as_bytes());
}
