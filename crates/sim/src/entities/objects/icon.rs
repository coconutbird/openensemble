//! Authoritative runtime state for retail class-zero minimap icon objects.

use pipeline::database::hw1::ProtoObject;

/// Simulation-owned state added to a class-zero object by icon prototypes.
///
/// The prototype selects the minimap artwork. Runtime state only retains the
/// values that retail can change after creation: its color override and
/// visibility flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconObject {
    color_override: Option<[u8; 3]>,
    visibility_flags: u8,
}

const VISIBLE_FOR_OWNER_ONLY: u8 = 1 << 0;
const VISIBLE_FOR_TEAM_ONLY: u8 = 1 << 1;
const VISIBLE_TO_ALL: u8 = 1 << 2;
const ALWAYS_VISIBLE_ON_MINIMAP: u8 = 1 << 3;

impl IconObject {
    pub(crate) fn from_prototype(
        prototype: &ProtoObject,
        color_override: Option<[u8; 3]>,
        force_visible_to_all: bool,
    ) -> Self {
        let mut visible_for_owner_only = has_flag(prototype, "VisibleForOwnerOnly");
        let visible_for_team_only = has_flag(prototype, "VisibleForTeamOnly");
        let mut visible_to_all = has_flag(prototype, "VisibleToAll");
        if force_visible_to_all {
            // Retail V3 clears only this flag before setting VisibleToAll.
            visible_for_owner_only = false;
            visible_to_all = true;
        }
        let mut visibility_flags = 0;
        set_flag(
            &mut visibility_flags,
            VISIBLE_FOR_OWNER_ONLY,
            visible_for_owner_only,
        );
        set_flag(
            &mut visibility_flags,
            VISIBLE_FOR_TEAM_ONLY,
            visible_for_team_only,
        );
        set_flag(&mut visibility_flags, VISIBLE_TO_ALL, visible_to_all);
        set_flag(
            &mut visibility_flags,
            ALWAYS_VISIBLE_ON_MINIMAP,
            has_flag(prototype, "AlwaysVisibleOnMinimap"),
        );
        Self {
            color_override,
            visibility_flags,
        }
    }

    /// Trigger-authored RGB override. Alpha is not changed by retail's effect.
    #[must_use]
    pub const fn color_override(self) -> Option<[u8; 3]> {
        self.color_override
    }

    /// Whether the prototype restricts the icon to its owning side.
    #[must_use]
    pub const fn visible_for_owner_only(self) -> bool {
        self.visibility_flags & VISIBLE_FOR_OWNER_ONLY != 0
    }

    /// Whether the prototype restricts normal visibility to its owning team.
    #[must_use]
    pub const fn visible_for_team_only(self) -> bool {
        self.visibility_flags & VISIBLE_FOR_TEAM_ONLY != 0
    }

    /// Whether the icon bypasses ordinary fog visibility.
    #[must_use]
    pub const fn visible_to_all(self) -> bool {
        self.visibility_flags & VISIBLE_TO_ALL != 0
    }

    /// Whether the prototype always appears on the minimap.
    #[must_use]
    pub const fn always_visible_on_minimap(self) -> bool {
        self.visibility_flags & ALWAYS_VISIBLE_ON_MINIMAP != 0
    }
}

pub(crate) fn is_icon_prototype(prototype: &ProtoObject) -> bool {
    prototype
        .object_types
        .iter()
        .any(|object_type| object_type.trim().eq_ignore_ascii_case("Icon"))
}

fn has_flag(prototype: &ProtoObject, expected: &str) -> bool {
    prototype
        .flags
        .iter()
        .any(|flag| flag.trim().eq_ignore_ascii_case(expected))
}

fn set_flag(flags: &mut u8, flag: u8, enabled: bool) {
    if enabled {
        *flags |= flag;
    }
}
