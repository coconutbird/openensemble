//! Retail ground-tie flags of a unit.
//!
//! `BEntity_tieToGround` @ `0x140202EB0` reads `mFlagTiesToGround` (`BEntity` `+0xC0 & 0x10`).
//! A unit sets it unless its prototype or scenario object has `NoTieToGround`
//! (`BProtoObject` `+181 & 8`). The prototype flag `OrientUnitWithGround` (`+0x428 & 8`) turns
//! on the terrain orientation. `DoNotFilterOrient` clears `FilterOrient` (`+0x42F & 1`).

use super::Unit;
use crate::sync::SyncChecksum;

/// Retail ground-tie flags of one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitGroundTie(u8);

impl UnitGroundTie {
    const TIES_TO_GROUND: u8 = 1 << 0;
    const ORIENT_WITH_GROUND: u8 = 1 << 1;
    const FILTER_ORIENT: u8 = 1 << 2;

    const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    fn set(&mut self, flag: u8, enabled: bool) {
        if enabled {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

impl Default for UnitGroundTie {
    fn default() -> Self {
        Self(Self::TIES_TO_GROUND | Self::FILTER_ORIENT)
    }
}

impl Unit {
    /// Apply the prototype flags `NoTieToGround`, `OrientUnitWithGround` and
    /// `DoNotFilterOrient`.
    pub(crate) fn configure_ground_tie(
        &mut self,
        no_tie_to_ground: bool,
        orient_with_ground: bool,
        do_not_filter_orient: bool,
    ) {
        let flags = &mut self.ground_tie;
        flags.set(UnitGroundTie::TIES_TO_GROUND, !no_tie_to_ground);
        flags.set(UnitGroundTie::ORIENT_WITH_GROUND, orient_with_ground);
        flags.set(UnitGroundTie::FILTER_ORIENT, !do_not_filter_orient);
    }

    /// Change `mFlagTiesToGround`, for example for a scenario object with `NoTieToGround`.
    pub(crate) fn set_ties_to_ground(&mut self, enabled: bool) {
        self.ground_tie.set(UnitGroundTie::TIES_TO_GROUND, enabled);
    }

    /// Return whether the world keeps this unit on the terrain (`mFlagTiesToGround`).
    #[must_use]
    pub const fn ties_to_ground(&self) -> bool {
        self.ground_tie.contains(UnitGroundTie::TIES_TO_GROUND)
    }

    /// Return whether the unit tilts with the terrain (`OrientUnitWithGround`).
    #[must_use]
    pub const fn orients_with_ground(&self) -> bool {
        self.ground_tie.contains(UnitGroundTie::ORIENT_WITH_GROUND)
    }

    /// Return whether the terrain tilt is smoothed (`FilterOrient`).
    #[must_use]
    pub const fn filters_ground_orientation(&self) -> bool {
        self.ground_tie.contains(UnitGroundTie::FILTER_ORIENT)
    }

    pub(crate) fn hash_ground_tie_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.ground_tie.0));
    }
}
