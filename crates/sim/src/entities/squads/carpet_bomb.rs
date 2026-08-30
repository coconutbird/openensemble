//! Checksummed state for retail's trigger-created squad carpet-bomb action.

use super::{Squad, SquadState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

const MAX_ATTACK_LOCATIONS: u16 = 100;

/// Observable phase of a legacy squad carpet-bomb action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum SquadCarpetBombPhase {
    /// No carpet-bomb action exists.
    #[default]
    Inactive = 0,
    /// The action is waiting to construct its attack locations.
    Preparing = 1,
    /// The squad is executing the generated position attacks.
    Working = 2,
    /// `MoveAir` children have been ordered back to their captured bases.
    Returning = 3,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CarpetBombOrder {
    pub(crate) target_position: Vec3,
    pub(crate) launch_position: Option<Vec3>,
    pub(crate) attack_run_distance: f32,
    pub(crate) attack_count: i32,
}

/// Persistent `BSquadActionCarpetBomb` state.
#[derive(Debug, Clone, Default)]
pub(crate) struct SquadCarpetBomb {
    phase: SquadCarpetBombPhase,
    order: Option<CarpetBombOrder>,
    remaining_locations: Vec<Vec3>,
    active_location: Option<Vec3>,
    completed_unit_ids: Vec<EntityId>,
}

impl SquadCarpetBomb {
    fn begin(&mut self, order: CarpetBombOrder) {
        self.phase = SquadCarpetBombPhase::Preparing;
        self.order = Some(order);
        self.remaining_locations.clear();
        self.active_location = None;
        self.completed_unit_ids.clear();
    }

    pub(crate) const fn phase(&self) -> SquadCarpetBombPhase {
        self.phase
    }

    pub(crate) const fn order(&self) -> Option<CarpetBombOrder> {
        self.order
    }

    pub(crate) const fn active_location(&self) -> Option<Vec3> {
        self.active_location
    }

    pub(crate) fn prepare(&mut self) -> bool {
        let Some(order) = self.order else {
            return false;
        };
        let Some(launch_position) = order.launch_position else {
            return false;
        };
        self.remaining_locations = attack_locations(order, launch_position);
        self.phase = SquadCarpetBombPhase::Working;
        self.activate_next_location()
    }

    pub(crate) fn activate_next_location(&mut self) -> bool {
        if self.active_location.is_some() {
            return true;
        }
        if self.remaining_locations.is_empty() {
            return false;
        }
        self.active_location = Some(self.remaining_locations.remove(0));
        self.completed_unit_ids.clear();
        true
    }

    pub(crate) fn mark_unit_complete(&mut self, unit_id: EntityId) {
        if let Err(index) = self.completed_unit_ids.binary_search(&unit_id) {
            self.completed_unit_ids.insert(index, unit_id);
        }
    }

    pub(crate) fn unit_complete(&self, unit_id: EntityId) -> bool {
        self.completed_unit_ids.binary_search(&unit_id).is_ok()
    }

    pub(crate) fn all_units_complete(&self, participants: &[EntityId]) -> bool {
        !participants.is_empty()
            && participants
                .iter()
                .all(|unit_id| self.completed_unit_ids.binary_search(unit_id).is_ok())
    }

    pub(crate) fn complete_active_location(&mut self) {
        self.active_location = None;
        self.completed_unit_ids.clear();
    }

    pub(crate) fn has_remaining_locations(&self) -> bool {
        !self.remaining_locations.is_empty()
    }

    pub(crate) fn enter_returning(&mut self) {
        self.phase = SquadCarpetBombPhase::Returning;
        self.active_location = None;
        self.completed_unit_ids.clear();
    }

    pub(super) fn cancel(&mut self) {
        *self = Self::default();
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        if let Some(order) = self.order {
            checksum.hash_u32(1);
            checksum.hash_vec3(
                order.target_position.x,
                order.target_position.y,
                order.target_position.z,
            );
            hash_optional_vec3(checksum, order.launch_position);
            checksum.hash_f32(order.attack_run_distance);
            checksum.hash_i32(order.attack_count);
        } else {
            checksum.hash_u32(0);
        }
        checksum.hash_u32(u32::try_from(self.remaining_locations.len()).unwrap_or(u32::MAX));
        for location in &self.remaining_locations {
            checksum.hash_vec3(location.x, location.y, location.z);
        }
        hash_optional_vec3(checksum, self.active_location);
        checksum.hash_u32(u32::try_from(self.completed_unit_ids.len()).unwrap_or(u32::MAX));
        for unit_id in &self.completed_unit_ids {
            checksum.hash_u32(unit_id.as_u32());
        }
    }
}

impl Squad {
    /// Return whether this squad owns a legacy carpet-bomb action.
    #[must_use]
    pub fn is_carpet_bombing(&self) -> bool {
        !matches!(self.carpet_bomb.phase(), SquadCarpetBombPhase::Inactive)
    }

    /// Return the action's authoritative phase.
    #[must_use]
    pub const fn carpet_bomb_phase(&self) -> SquadCarpetBombPhase {
        self.carpet_bomb.phase()
    }

    /// Return the center of the authored carpet-bomb run.
    #[must_use]
    pub fn carpet_bomb_target(&self) -> Option<Vec3> {
        self.carpet_bomb.order().map(|order| order.target_position)
    }

    /// Return the ground position currently owned by the child attack action.
    #[must_use]
    pub const fn carpet_bomb_attack_position(&self) -> Option<Vec3> {
        self.carpet_bomb.active_location()
    }

    /// Return retail's persistent `IgnoreLeash` squad flag.
    #[must_use]
    pub const fn ignores_leash(&self) -> bool {
        self.ignore_leash
    }

    pub(crate) fn begin_carpet_bomb_order(&mut self, order: CarpetBombOrder) -> bool {
        if !self.is_alive()
            || self.garrison.is_garrisoned()
            || self.unit_ids.is_empty()
            || !order.target_position.is_finite()
            || order
                .launch_position
                .is_some_and(|position| !position.is_finite())
            || !order.attack_run_distance.is_finite()
        {
            return false;
        }
        self.remove_all_orders();
        self.ignore_leash = true;
        self.carpet_bomb.begin(order);
        self.state = SquadState::Working;
        self.base.velocity = Vec3::ZERO;
        self.cancel_idle_action();
        true
    }

    pub(crate) fn finish_carpet_bomb_order(&mut self) {
        self.carpet_bomb.cancel();
        self.move_target = None;
        self.base.velocity = Vec3::ZERO;
        if self.is_alive() {
            self.state = SquadState::Idle;
        }
        self.cancel_idle_action();
    }

    pub(crate) fn hash_carpet_bomb_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.ignore_leash));
        self.carpet_bomb.hash_state(checksum);
    }
}

fn attack_locations(order: CarpetBombOrder, launch_position: Vec3) -> Vec<Vec3> {
    let attack_count = u16::try_from(order.attack_count.max(1))
        .unwrap_or(u16::MAX)
        .min(MAX_ATTACK_LOCATIONS);
    if attack_count == 1 || order.attack_run_distance == 0.0 {
        return vec![order.target_position];
    }
    let direction = Vec3::new(
        order.target_position.x - launch_position.x,
        0.0,
        order.target_position.z - launch_position.z,
    )
    .normalize_or_zero();
    let half_run = direction * (order.attack_run_distance * 0.5);
    let first = order.target_position - half_run;
    let spacing = direction * (order.attack_run_distance / f32::from(attack_count - 1));
    (0..attack_count)
        .map(|index| first + spacing * f32::from(index))
        .collect()
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        checksum.hash_vec3(value.x, value.y, value.z);
    } else {
        checksum.hash_u32(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn attack_run_is_centered_ordered_and_save_compatible() {
        let locations = attack_locations(
            CarpetBombOrder {
                target_position: Vec3::new(20.0, 3.0, 0.0),
                launch_position: Some(Vec3::ZERO),
                attack_run_distance: 10.0,
                attack_count: 3,
            },
            Vec3::ZERO,
        );
        assert_eq!(locations.len(), 3);
        assert_eq!(locations[0], Vec3::new(15.0, 3.0, 0.0));
        assert_eq!(locations[1], Vec3::new(20.0, 3.0, 0.0));
        assert_eq!(locations[2], Vec3::new(25.0, 3.0, 0.0));

        let capped = attack_locations(
            CarpetBombOrder {
                attack_count: 101,
                ..CarpetBombOrder {
                    target_position: Vec3::X,
                    launch_position: Some(Vec3::ZERO),
                    attack_run_distance: 1.0,
                    attack_count: 1,
                }
            },
            Vec3::ZERO,
        );
        assert_eq!(capped.len(), usize::from(MAX_ATTACK_LOCATIONS));
    }

    #[test]
    fn lifecycle_and_per_member_progress_are_checksummed() {
        let mut squad = Squad::new(EntityId::new(EntityClass::Squad, 1), 1);
        let unit = EntityId::new(EntityClass::Unit, 1);
        squad.unit_ids.push(unit);
        assert!(squad.begin_carpet_bomb_order(CarpetBombOrder {
            target_position: Vec3::X * 10.0,
            launch_position: Some(Vec3::ZERO),
            attack_run_distance: 20.0,
            attack_count: 2,
        }));
        assert!(squad.ignores_leash());
        assert!(squad.carpet_bomb.prepare());
        let before = hash(&squad);
        squad.carpet_bomb.mark_unit_complete(unit);
        assert_ne!(before, hash(&squad));
        assert!(squad.carpet_bomb.all_units_complete(&[unit]));
        squad.carpet_bomb.complete_active_location();
        assert!(squad.carpet_bomb.activate_next_location());
        squad.carpet_bomb.enter_returning();
        assert_eq!(squad.carpet_bomb_phase(), SquadCarpetBombPhase::Returning);
    }

    fn hash(squad: &Squad) -> u32 {
        let mut checksum = SyncChecksum::new();
        squad.hash_carpet_bomb_state(&mut checksum);
        checksum.value()
    }
}
