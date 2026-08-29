//! Sticky projectile transform and resting-state ownership.

use super::{Projectile, ProjectileRuntimeFlags, ProjectileTargetMotion};
use crate::entities::squads::{formation_offset_to_local, formation_offset_to_world};
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

#[derive(Debug, Clone)]
pub(super) enum ProjectileMotionState {
    Flying,
    Stuck(ProjectileStickState),
    Rest,
}

#[derive(Debug, Clone)]
pub(super) struct ProjectileStickState {
    unit_id: EntityId,
    local_position: Vec3,
    local_forward: Vec3,
}

impl Projectile {
    pub(crate) fn has_timed_lifecycle(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::EXPLODE_ON_TIMER)
            || self
                .runtime_flags
                .contains(ProjectileRuntimeFlags::EXPIRE_ON_TIMER)
    }

    pub(crate) fn explodes_on_timer(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::EXPLODE_ON_TIMER)
    }

    pub(crate) fn expires_on_timer(&self) -> bool {
        self.runtime_flags
            .contains(ProjectileRuntimeFlags::EXPIRE_ON_TIMER)
    }

    pub(crate) fn is_sticky(&self) -> bool {
        self.runtime_flags.contains(ProjectileRuntimeFlags::STICKY)
    }

    pub(crate) fn is_flying(&self) -> bool {
        matches!(self.motion_state, ProjectileMotionState::Flying)
    }

    pub(crate) fn stuck_to_unit(&self) -> Option<EntityId> {
        match &self.motion_state {
            ProjectileMotionState::Stuck(stick) => Some(stick.unit_id),
            ProjectileMotionState::Flying | ProjectileMotionState::Rest => None,
        }
    }

    pub(crate) fn stick_to_unit(
        &mut self,
        unit_id: EntityId,
        impact_position: Vec3,
        unit_position: Vec3,
        unit_forward: Vec3,
    ) {
        self.base.position = impact_position;
        self.motion_state = ProjectileMotionState::Stuck(ProjectileStickState {
            unit_id,
            local_position: formation_offset_to_local(
                unit_forward,
                impact_position - unit_position,
            ),
            local_forward: formation_offset_to_local(unit_forward, self.base.forward),
        });
        self.tracking = false;
    }

    pub(crate) fn rest_at(&mut self, position: Vec3) {
        self.base.position = position;
        self.motion_state = ProjectileMotionState::Rest;
        self.tracking = false;
    }

    pub(super) fn update_non_flying_motion(&mut self, target: Option<ProjectileTargetMotion>) {
        let ProjectileMotionState::Stuck(stick) = &self.motion_state else {
            return;
        };
        let Some(target) = target else {
            return;
        };
        self.base.position =
            target.position + formation_offset_to_world(target.forward, stick.local_position);
        self.base.set_forward(formation_offset_to_world(
            target.forward,
            stick.local_forward,
        ));
    }

    pub(super) fn hash_motion_state(&self, checksum: &mut SyncChecksum) {
        match &self.motion_state {
            ProjectileMotionState::Flying => checksum.hash_u32(0),
            ProjectileMotionState::Stuck(stick) => {
                checksum.hash_u32(1);
                checksum.hash_u32(stick.unit_id.as_u32());
                checksum.hash_vec3(
                    stick.local_position.x,
                    stick.local_position.y,
                    stick.local_position.z,
                );
                checksum.hash_vec3(
                    stick.local_forward.x,
                    stick.local_forward.y,
                    stick.local_forward.z,
                );
            }
            ProjectileMotionState::Rest => checksum.hash_u32(2),
        }
    }
}
