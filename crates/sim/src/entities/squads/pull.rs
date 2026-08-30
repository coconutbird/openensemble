//! Uninterruptible target-squad state for retail `JumpPull` orders.

use super::Squad;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

const MIN_DISTANCE_SQUARED: f32 = 0.000_001;

/// Authoritative phase of a Brute Chief squad pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SquadPullPhase {
    /// No pull owns this squad.
    #[default]
    Inactive,
    /// The squad Jump action has connected and is queuing member opportunities.
    Pending,
    /// Members are moving along their individual quadratic splines.
    Flying,
}

#[derive(Debug, Clone)]
pub(crate) struct PullMemberPlan {
    unit_id: EntityId,
    coefficient_0: Vec3,
    coefficient_1: Vec3,
    coefficient_2: Vec3,
    planar_distance: f32,
    parameter: f32,
}

impl PullMemberPlan {
    pub(crate) fn new(unit_id: EntityId, start: Vec3, end: Vec3) -> Self {
        let delta = end - start;
        let planar_distance = Vec3::new(delta.x, 0.0, delta.z).length();
        let midpoint = Vec3::new(
            0.75f32.mul_add(start.x, 0.25 * end.x),
            start.y.max(end.y) + planar_distance * 0.25,
            0.75f32.mul_add(start.z, 0.25 * end.z),
        );
        let coefficient_2 = ((midpoint - start) - delta * 0.5) / -0.25;
        Self {
            unit_id,
            coefficient_0: start,
            coefficient_1: delta - coefficient_2,
            coefficient_2,
            planar_distance,
            parameter: 0.0,
        }
    }

    fn advance(&mut self, dt: f32, velocity_scalar: f32) -> (Vec3, bool) {
        if self.planar_distance * self.planar_distance <= MIN_DISTANCE_SQUARED {
            self.parameter = 1.0;
        } else {
            self.parameter += dt * (velocity_scalar / self.planar_distance);
            if self.parameter >= 1.0 {
                self.parameter = 1.0;
            }
        }
        let position = self.coefficient_2 * self.parameter * self.parameter
            + self.coefficient_1 * self.parameter
            + self.coefficient_0;
        (position, self.parameter >= 1.0)
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.unit_id.as_u32());
        hash_vec3(checksum, self.coefficient_0);
        hash_vec3(checksum, self.coefficient_1);
        hash_vec3(checksum, self.coefficient_2);
        checksum.hash_f32(self.planar_distance);
        checksum.hash_f32(self.parameter);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SquadPull {
    phase: SquadPullPhase,
    attacker_id: EntityId,
    action_name: String,
    target_anchor: Vec3,
    velocity_scalar: f32,
    end_animation_type: Option<String>,
    members: Vec<PullMemberPlan>,
}

impl Default for SquadPull {
    fn default() -> Self {
        Self {
            phase: SquadPullPhase::Inactive,
            attacker_id: EntityId::INVALID,
            action_name: String::new(),
            target_anchor: Vec3::ZERO,
            velocity_scalar: 0.0,
            end_animation_type: None,
            members: Vec::new(),
        }
    }
}

pub(crate) struct PullAdvance {
    pub(crate) positions: Vec<(EntityId, Vec3)>,
    pub(crate) complete: bool,
}

impl SquadPull {
    pub(crate) fn begin(
        &mut self,
        attacker_id: EntityId,
        action_name: &str,
        target_anchor: Vec3,
        velocity_scalar: f32,
        end_animation_type: Option<&str>,
        mut members: Vec<PullMemberPlan>,
    ) -> bool {
        if self.phase != SquadPullPhase::Inactive
            || attacker_id.is_invalid()
            || !target_anchor.is_finite()
            || !velocity_scalar.is_finite()
            || members.is_empty()
        {
            return false;
        }
        members.sort_by_key(|member| member.unit_id);
        self.phase = SquadPullPhase::Pending;
        self.attacker_id = attacker_id;
        action_name.clone_into(&mut self.action_name);
        self.target_anchor = target_anchor;
        self.velocity_scalar = velocity_scalar;
        self.end_animation_type = end_animation_type.map(str::to_owned);
        self.members = members;
        true
    }

    pub(crate) fn advance(&mut self, dt: f32) -> PullAdvance {
        if self.phase == SquadPullPhase::Pending {
            self.phase = SquadPullPhase::Flying;
            return PullAdvance {
                positions: Vec::new(),
                complete: false,
            };
        }
        if self.phase != SquadPullPhase::Flying || !dt.is_finite() || dt <= 0.0 {
            return PullAdvance {
                positions: Vec::new(),
                complete: false,
            };
        }
        let mut complete = true;
        let positions = self
            .members
            .iter_mut()
            .map(|member| {
                let (position, member_complete) = member.advance(dt, self.velocity_scalar);
                complete &= member_complete;
                (member.unit_id, position)
            })
            .collect();
        PullAdvance {
            positions,
            complete,
        }
    }

    pub(crate) fn cancel(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn attacker_id(&self) -> Option<EntityId> {
        (!self.attacker_id.is_invalid()).then_some(self.attacker_id)
    }

    pub(crate) fn target_anchor(&self) -> Option<Vec3> {
        (!matches!(self.phase, SquadPullPhase::Inactive)).then_some(self.target_anchor)
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(self.attacker_id.as_u32());
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
        hash_vec3(checksum, self.target_anchor);
        checksum.hash_f32(self.velocity_scalar);
        checksum.hash_u32(u32::from(self.end_animation_type.is_some()));
        if let Some(animation) = &self.end_animation_type {
            checksum.hash_u32(u32::try_from(animation.len()).unwrap_or(u32::MAX));
            checksum.hash_bytes(animation.as_bytes());
        }
        checksum.hash_u32(u32::try_from(self.members.len()).unwrap_or(u32::MAX));
        for member in &self.members {
            member.hash_state(checksum);
        }
    }
}

impl Squad {
    /// Return whether a critical enemy `JumpPull` currently owns this squad.
    #[must_use]
    pub fn is_being_pulled(&self) -> bool {
        self.pull.phase != SquadPullPhase::Inactive
    }

    /// Return the current authoritative `JumpPull` phase.
    #[must_use]
    pub const fn pull_phase(&self) -> SquadPullPhase {
        self.pull.phase
    }

    /// Return the attacking unit responsible for this pull.
    #[must_use]
    pub fn pulled_by(&self) -> Option<EntityId> {
        self.pull.attacker_id()
    }

    /// Return the squad anchor selected beside the puller.
    #[must_use]
    pub fn pull_target(&self) -> Option<Vec3> {
        self.pull.target_anchor()
    }

    pub(crate) fn hash_pull_state(&self, checksum: &mut SyncChecksum) {
        self.pull.hash_state(checksum);
    }
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_uses_retail_asymmetric_quadratic_curve() {
        let mut plan = PullMemberPlan::new(
            EntityId::new(crate::entity_id::EntityClass::Unit, 0),
            Vec3::ZERO,
            Vec3::new(40.0, 0.0, 0.0),
        );
        let (quarter, complete) = plan.advance(0.25, 40.0);
        assert!(!complete);
        assert!((quarter.x - 2.5).abs() < 0.000_1);
        assert!((quarter.y - 7.5).abs() < 0.000_1);
        let (end, complete) = plan.advance(0.75, 40.0);
        assert!(complete);
        assert_eq!(end, Vec3::new(40.0, 0.0, 0.0));
    }
}
