//! Retail queue state for squads waiting to leave a trainer.

use super::BuildingProduction;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// One completed squad waiting for its trainer-owned birth sequence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainedSquadBirth {
    squad_id: EntityId,
    plotted_position: Option<Vec3>,
    play_sound: bool,
}

impl TrainedSquadBirth {
    pub(crate) const fn new(squad_id: EntityId, play_sound: bool) -> Self {
        Self {
            squad_id,
            plotted_position: None,
            play_sound,
        }
    }

    /// Completed squad controlled by this queued birth.
    #[must_use]
    pub const fn squad_id(self) -> EntityId {
        self.squad_id
    }

    /// Formation-plotted destination near a direct rally point, when known.
    #[must_use]
    pub const fn plotted_position(self) -> Option<Vec3> {
        self.plotted_position
    }

    /// Whether presentation should emit the trained-unit notification.
    #[must_use]
    pub const fn play_sound(self) -> bool {
        self.play_sound
    }

    pub(crate) fn set_plotted_position(&mut self, position: Vec3) {
        self.plotted_position = position.is_finite().then_some(position);
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.squad_id.as_u32());
        checksum.hash_u32(u32::from(self.plotted_position.is_some()));
        if let Some(position) = self.plotted_position {
            checksum.hash_vec3(position.x, position.y, position.z);
        }
        checksum.hash_u32(u32::from(self.play_sound));
    }
}

impl BuildingProduction {
    /// Completed squads still contained by this trainer, in retail queue order.
    pub fn trained_squad_births(&self) -> impl Iterator<Item = TrainedSquadBirth> + '_ {
        self.trained_squads.iter().copied()
    }

    /// Shared delay before the next queued squad may leave the trainer.
    #[must_use]
    pub const fn trained_squad_birth_time(&self) -> f32 {
        self.trained_squad_birth_time
    }

    pub(crate) fn queue_trained_squad(&mut self, squad_id: EntityId, play_sound: bool) {
        self.trained_squads
            .push(TrainedSquadBirth::new(squad_id, play_sound));
    }

    pub(crate) fn has_trained_squad_birth_work(&self) -> bool {
        !self.trained_squads.is_empty() || self.trained_squad_birth_time > 0.0
    }

    /// Match retail's timer edge: reaching exactly zero waits until the next update.
    pub(crate) fn advance_trained_squad_birth_time(&mut self, dt: f32) -> bool {
        if self.trained_squad_birth_time > 0.0 {
            self.trained_squad_birth_time -= dt;
            if self.trained_squad_birth_time < 0.0 {
                self.trained_squad_birth_time = 0.0;
            } else {
                return false;
            }
        }
        !self.trained_squads.is_empty()
    }

    pub(crate) fn set_trained_squad_birth_time(&mut self, duration: f32) {
        self.trained_squad_birth_time = if duration.is_finite() && duration > 0.0 {
            duration
        } else {
            0.0
        };
    }

    pub(crate) fn remove_missing_trained_squads(
        &mut self,
        mut exists: impl FnMut(EntityId) -> bool,
    ) {
        self.trained_squads.retain(|birth| exists(birth.squad_id));
    }

    pub(crate) fn first_trained_squad(&self) -> Option<TrainedSquadBirth> {
        self.trained_squads.first().copied()
    }

    pub(crate) fn set_trained_squad_plotted_position(
        &mut self,
        squad_id: EntityId,
        position: Vec3,
    ) {
        if let Some(birth) = self
            .trained_squads
            .iter_mut()
            .find(|birth| birth.squad_id == squad_id)
        {
            birth.set_plotted_position(position);
        }
    }

    pub(crate) fn remove_trained_squad(&mut self, squad_id: EntityId) -> bool {
        let Some(index) = self
            .trained_squads
            .iter()
            .position(|birth| birth.squad_id == squad_id)
        else {
            return false;
        };
        self.trained_squads.remove(index);
        true
    }

    pub(super) fn hash_trained_squad_birth_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.trained_squads.len()).unwrap_or(u32::MAX));
        for birth in &self.trained_squads {
            birth.hash_state(checksum);
        }
        checksum.hash_f32(self.trained_squad_birth_time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn birth_timer_preserves_retail_exact_zero_edge() {
        let mut production = BuildingProduction::default();
        production.queue_trained_squad(EntityId::new(EntityClass::Squad, 7), true);
        production.set_trained_squad_birth_time(1.0);

        assert!(!production.advance_trained_squad_birth_time(1.0));
        assert!(production.advance_trained_squad_birth_time(0.05));
    }

    #[test]
    fn queued_birth_and_timer_are_part_of_the_sync_checksum() {
        let squad_id = EntityId::new(EntityClass::Squad, 9);
        let mut empty = BuildingProduction::default();
        let mut queued = empty.clone();
        queued.queue_trained_squad(squad_id, true);
        let empty_checksum = checksum(&empty);

        assert_ne!(checksum(&queued), empty_checksum);

        queued.set_trained_squad_plotted_position(squad_id, Vec3::X);
        let plotted_checksum = checksum(&queued);
        assert_ne!(plotted_checksum, empty_checksum);

        queued.set_trained_squad_birth_time(2.0);
        assert_ne!(checksum(&queued), plotted_checksum);

        empty.queue_trained_squad(squad_id, false);
        assert_ne!(checksum(&empty), checksum(&queued));
    }

    fn checksum(production: &BuildingProduction) -> u32 {
        let mut checksum = SyncChecksum::new();
        production.hash_trained_squad_birth_state(&mut checksum);
        checksum.value()
    }
}
