//! Squad-level garrison orders and transport lifecycle state.

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// Authoritative phase of a squad's garrison or ungarrison action.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SquadContainmentState {
    /// The squad is in the world and has no containment action.
    #[default]
    Free,
    /// The squad is moving into range of a container.
    Garrisoning {
        /// Target container unit.
        target: EntityId,
        /// Command range override; zero selects authored/default range.
        range: f32,
        /// World time when the order began.
        started_at_ms: u32,
    },
    /// Every surviving member is contained by the target squad's leader unit.
    Garrisoned {
        /// Unit that contains these members.
        container: EntityId,
        /// World time when containment completed.
        since_ms: u32,
    },
    /// Members remain contained while an authored exit action runs.
    Ungarrisoning {
        /// Original container unit.
        container: EntityId,
        /// Unit defining the exit transform.
        destination: EntityId,
        /// Spawn transform selected for the squad.
        exit_position: Vec3,
        /// Optional post-exit rally point.
        rally_position: Option<Vec3>,
        /// World time when the exit began.
        started_at_ms: u32,
    },
}

/// Garrison state owned by one logical squad.
#[derive(Debug, Clone, Default)]
pub struct SquadGarrison {
    state: SquadContainmentState,
    contained_squad_ids: Vec<EntityId>,
}

impl SquadGarrison {
    /// Current action/lifecycle state.
    #[must_use]
    pub const fn state(&self) -> SquadContainmentState {
        self.state
    }

    /// Sorted logical squads currently contained by this squad's units.
    #[must_use]
    pub fn contained_squad_ids(&self) -> &[EntityId] {
        &self.contained_squad_ids
    }

    /// Return whether members are still physically contained.
    #[must_use]
    pub const fn is_garrisoned(&self) -> bool {
        matches!(
            self.state,
            SquadContainmentState::Garrisoned { .. } | SquadContainmentState::Ungarrisoning { .. }
        )
    }

    /// Current container for a fully garrisoned or exiting squad.
    #[must_use]
    pub const fn container_id(&self) -> Option<EntityId> {
        match self.state {
            SquadContainmentState::Garrisoned { container, .. }
            | SquadContainmentState::Ungarrisoning { container, .. } => Some(container),
            _ => None,
        }
    }

    pub(crate) fn begin_garrison(&mut self, target: EntityId, range: f32, now_ms: u32) {
        self.state = SquadContainmentState::Garrisoning {
            target,
            range,
            started_at_ms: now_ms,
        };
    }

    pub(crate) fn mark_garrisoned(&mut self, container: EntityId, now_ms: u32) {
        self.state = SquadContainmentState::Garrisoned {
            container,
            since_ms: now_ms,
        };
    }

    pub(crate) fn begin_ungarrison(
        &mut self,
        container: EntityId,
        destination: EntityId,
        exit_position: Vec3,
        rally_position: Option<Vec3>,
        now_ms: u32,
    ) {
        self.state = SquadContainmentState::Ungarrisoning {
            container,
            destination,
            exit_position,
            rally_position,
            started_at_ms: now_ms,
        };
    }

    pub(crate) fn finish_action(&mut self) {
        self.state = SquadContainmentState::Free;
    }

    pub(crate) fn cancel_pending(&mut self) {
        if matches!(self.state, SquadContainmentState::Garrisoning { .. }) {
            self.state = SquadContainmentState::Free;
        }
    }

    pub(crate) fn add_contained_squad(&mut self, squad_id: EntityId) -> bool {
        match self.contained_squad_ids.binary_search(&squad_id) {
            Ok(_) => false,
            Err(index) => {
                self.contained_squad_ids.insert(index, squad_id);
                true
            }
        }
    }

    pub(crate) fn remove_contained_squad(&mut self, squad_id: EntityId) -> bool {
        let Ok(index) = self.contained_squad_ids.binary_search(&squad_id) else {
            return false;
        };
        self.contained_squad_ids.remove(index);
        true
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_containment_state(checksum, self.state);
        checksum.hash_u32(u32::try_from(self.contained_squad_ids.len()).unwrap_or(u32::MAX));
        for id in &self.contained_squad_ids {
            checksum.hash_u32(id.as_u32());
        }
    }
}

fn hash_containment_state(checksum: &mut SyncChecksum, state: SquadContainmentState) {
    match state {
        SquadContainmentState::Free => checksum.hash_u32(0),
        SquadContainmentState::Garrisoning {
            target,
            range,
            started_at_ms,
        } => {
            checksum.hash_u32(1);
            checksum.hash_u32(target.as_u32());
            checksum.hash_f32(range);
            checksum.hash_u32(started_at_ms);
        }
        SquadContainmentState::Garrisoned {
            container,
            since_ms,
        } => {
            checksum.hash_u32(2);
            checksum.hash_u32(container.as_u32());
            checksum.hash_u32(since_ms);
        }
        SquadContainmentState::Ungarrisoning {
            container,
            destination,
            exit_position,
            rally_position,
            started_at_ms,
        } => {
            checksum.hash_u32(3);
            checksum.hash_u32(container.as_u32());
            checksum.hash_u32(destination.as_u32());
            checksum.hash_vec3(exit_position.x, exit_position.y, exit_position.z);
            hash_optional_vec3(checksum, rally_position);
            checksum.hash_u32(started_at_ms);
        }
    }
}

fn hash_optional_vec3(checksum: &mut SyncChecksum, value: Option<Vec3>) {
    checksum.hash_u32(u32::from(value.is_some()));
    if let Some(value) = value {
        checksum.hash_vec3(value.x, value.y, value.z);
    }
}
