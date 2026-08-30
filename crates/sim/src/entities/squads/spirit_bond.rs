//! Persistent two-member Hunter `SpiritBond` state.

use super::Squad;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SpiritBondPhase {
    #[default]
    Dormant = 0,
    Active = 1,
    Done = 2,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SquadSpiritBond {
    phase: SpiritBondPhase,
    beam_object_id: Option<EntityId>,
}

impl SquadSpiritBond {
    pub(crate) const fn phase(self) -> SpiritBondPhase {
        self.phase
    }

    pub(crate) fn activate(&mut self, beam_object_id: Option<EntityId>) {
        self.phase = SpiritBondPhase::Active;
        self.beam_object_id = beam_object_id;
    }

    pub(crate) fn set_beam(&mut self, beam_object_id: Option<EntityId>) {
        self.beam_object_id = beam_object_id;
    }

    pub(crate) fn finish(&mut self) -> Option<EntityId> {
        self.phase = SpiritBondPhase::Done;
        self.beam_object_id.take()
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.beam_object_id.map_or(u32::MAX, EntityId::as_u32));
    }
}

impl Squad {
    /// Whether both live squadmates currently receive the `SpiritBond` buff.
    #[must_use]
    pub const fn spirit_bond_active(&self) -> bool {
        matches!(self.spirit_bond.phase(), SpiritBondPhase::Active)
    }

    /// Authoritative class-zero beam visual owned by `SpiritBond`.
    #[must_use]
    pub const fn spirit_bond_beam(&self) -> Option<EntityId> {
        self.spirit_bond.beam_object_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn finishing_is_terminal_and_releases_the_beam_reference() {
        let beam = EntityId::new(EntityClass::Object, 4);
        let mut state = SquadSpiritBond::default();
        state.activate(Some(beam));
        assert_eq!(state.phase(), SpiritBondPhase::Active);
        assert_eq!(state.finish(), Some(beam));
        assert_eq!(state.phase(), SpiritBondPhase::Done);
        assert_eq!(state.finish(), None);
    }
}
