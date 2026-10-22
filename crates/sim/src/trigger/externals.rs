//! Inputs that the launcher of a trigger script gives to the script.

use super::value::{Cost, Vec3};
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;

/// The external inputs of one trigger script (retail `BTriggerScriptExternals`).
///
/// Retail keeps one 144-byte record for each script (`BTriggerScript_getExternals`). Each value
/// has a flag bit that tells if the launcher set it. Here `None` means that the flag is clear.
/// The comments give the retail offset and flag bit of each value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerExternals {
    /// Location (`+0`, flag `+132 & 0x80`).
    pub location: Option<Vec3>,
    /// Location list (`+16`, flag `+133 & 0x01`).
    pub location_list: Option<Vec<Vec3>>,
    /// Squad list (`+40`, flag `+132 & 0x08`).
    pub squad_list: Option<Vec<EntityId>>,
    /// Unit list (`+56`, flag `+132 & 0x20`).
    pub unit_list: Option<Vec<EntityId>>,
    /// Cost (`+72`, flag `+132 & 0x40`).
    pub cost: Option<Cost>,
    /// Float (`+112`, flag `+133 & 0x04`).
    pub float: Option<f32>,
    /// Squad (`+116`, flag `+132 & 0x04`).
    pub squad: Option<EntityId>,
    /// Unit (`+120`, flag `+132 & 0x10`).
    pub unit: Option<EntityId>,
    /// Player (`+124`, flag `+132 & 0x01`, `BTriggerManager_setScriptPlayer`).
    pub player: Option<i32>,
    /// Proto power ID (`+128`, flag `+132 & 0x02`).
    pub power: Option<i32>,
}

impl TriggerExternals {
    /// Make externals that set only the player (`BTriggerManager_setScriptPlayer`).
    #[must_use]
    pub fn with_player(player: i32) -> Self {
        Self {
            player: Some(player),
            ..Self::default()
        }
    }

    pub(super) fn hash_state(&self, checksum: &mut SyncChecksum) {
        hash_option(checksum, self.location.as_ref(), |checksum, value| {
            checksum.hash_vec3(value.x, value.y, value.z);
        });
        hash_option(checksum, self.location_list.as_ref(), |checksum, values| {
            hash_len(checksum, values.len());
            for value in values {
                checksum.hash_vec3(value.x, value.y, value.z);
            }
        });
        hash_option(checksum, self.squad_list.as_ref(), |checksum, values| {
            hash_entities(checksum, values);
        });
        hash_option(checksum, self.unit_list.as_ref(), |checksum, values| {
            hash_entities(checksum, values);
        });
        hash_option(checksum, self.cost.as_ref(), |checksum, value| {
            for amount in value.amounts() {
                checksum.hash_f32(amount);
            }
        });
        hash_option(checksum, self.float.as_ref(), |checksum, value| {
            checksum.hash_f32(*value);
        });
        hash_option(checksum, self.squad.as_ref(), |checksum, value| {
            checksum.hash_u32(value.as_u32());
        });
        hash_option(checksum, self.unit.as_ref(), |checksum, value| {
            checksum.hash_u32(value.as_u32());
        });
        hash_option(checksum, self.player.as_ref(), |checksum, value| {
            checksum.hash_i32(*value);
        });
        hash_option(checksum, self.power.as_ref(), |checksum, value| {
            checksum.hash_i32(*value);
        });
    }
}

fn hash_option<T>(
    checksum: &mut SyncChecksum,
    value: Option<&T>,
    hash: impl FnOnce(&mut SyncChecksum, &T),
) {
    if let Some(value) = value {
        checksum.hash_u32(1);
        hash(checksum, value);
    } else {
        checksum.hash_u32(0);
    }
}

fn hash_entities(checksum: &mut SyncChecksum, values: &[EntityId]) {
    hash_len(checksum, values.len());
    for value in values {
        checksum.hash_u32(value.as_u32());
    }
}

fn hash_len(checksum: &mut SyncChecksum, len: usize) {
    checksum.hash_u32(u32::try_from(len).unwrap_or(u32::MAX));
}
