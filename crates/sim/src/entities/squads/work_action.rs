//! Squad action state that the Repair, Unpack, `PointBlankAttack` and
//! `PlayBlockingAnimation` Work orders create (`BSquad_executeOrder` @ `0x140399780`).

use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;

/// A damaged squad member and its hitpoints when the repair started.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RepairMember {
    pub(crate) unit_id: EntityId,
    pub(crate) start_hitpoints: f32,
}

/// `BSquadActionRepair`: the squad repairs and reinforces itself over the repair time of its
/// player.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SquadRepairAction {
    /// Damaged members at the start (`mUnits`).
    pub(crate) members: Vec<RepairMember>,
    /// Prototype names of the missing members (`mReinforcements`).
    pub(crate) reinforcements: Vec<String>,
    /// Elapsed repair time in seconds (`mTimer`).
    pub(crate) timer: f32,
    /// Repair time of the squad owner in seconds (`BPlayer+756`).
    pub(crate) repair_time: f32,
    /// Squad last-damaged time at the start (`mDamagedTime`).
    pub(crate) damaged_time: u32,
    /// Part of the repair time that heals members; the rest reinforces (`mRepairPercent`).
    pub(crate) repair_percent: f32,
    /// Number of reinforcements already created (`mReinforceIndex`).
    pub(crate) reinforce_index: usize,
}

/// The work action that one squad Work order runs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SquadWorkAction {
    /// Order 4 (`BSquadActionRepair`, squad action 65).
    Repair(SquadRepairAction),
    /// Order 20 (`BSquadActionWork`, squad action 74, unit opportunity 19 Unpack): go to the
    /// target, then replace the squad with the first object of the ability.
    Unpack {
        /// Target position of the order.
        target: Vec3,
        /// Requested ability of the order.
        ability_id: Option<u8>,
        /// Facing angle of the order in degrees.
        angle: f32,
    },
    /// Order 30: a command-priority `PointBlankAttack` opportunity (type 33) for each member.
    PointBlankAttack {
        /// Target entity of the order.
        target_id: Option<EntityId>,
        /// Target position of the order.
        target_position: Option<Vec3>,
    },
    /// Order 17 (`BSquadActionPlayBlockingAnimation`): a looping Misc-state animation.
    PlayBlockingAnimation {
        /// Animation type of the order (`BSimOrder+88`).
        animation_type: u8,
    },
}

impl SquadWorkAction {
    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        match self {
            Self::Repair(repair) => {
                checksum.hash_u32(0);
                checksum.hash_u32(u32::try_from(repair.members.len()).unwrap_or(u32::MAX));
                for member in &repair.members {
                    checksum.hash_u32(member.unit_id.as_u32());
                    checksum.hash_f32(member.start_hitpoints);
                }
                checksum.hash_u32(u32::try_from(repair.reinforcements.len()).unwrap_or(u32::MAX));
                for prototype in &repair.reinforcements {
                    checksum.hash_string(prototype);
                }
                checksum.hash_f32(repair.timer);
                checksum.hash_f32(repair.repair_time);
                checksum.hash_u32(repair.damaged_time);
                checksum.hash_f32(repair.repair_percent);
                checksum.hash_u32(u32::try_from(repair.reinforce_index).unwrap_or(u32::MAX));
            }
            Self::Unpack {
                target,
                ability_id,
                angle,
            } => {
                checksum.hash_u32(1);
                checksum.hash_vec3(target.x, target.y, target.z);
                checksum.hash_u32(ability_id.map_or(u32::MAX, u32::from));
                checksum.hash_f32(*angle);
            }
            Self::PointBlankAttack {
                target_id,
                target_position,
            } => {
                checksum.hash_u32(2);
                checksum.hash_u32(target_id.map_or(u32::MAX, EntityId::as_u32));
                if let Some(position) = target_position {
                    checksum.hash_vec3(position.x, position.y, position.z);
                }
            }
            Self::PlayBlockingAnimation { animation_type } => {
                checksum.hash_u32(3);
                checksum.hash_u32(u32::from(*animation_type));
            }
        }
    }
}

impl super::Squad {
    /// Whether the squad runs a self-repair action (`mFlagIsRepairing`, `squad+741` bit 7).
    pub(crate) const fn is_self_repairing(&self) -> bool {
        matches!(self.work_action, Some(SquadWorkAction::Repair(_)))
    }
}
