//! Scenario-layered retail `Join` action selection and immutable settings.

use super::{AttackQuery, GameplayCatalog};
pub use crate::entities::{JoinKind, JoinMergeType};
use pipeline::database::hw1::tactics::{Action, ProtoObjectRef};

mod merged_squads;
mod values;

pub use merged_squads::MergedSquadProfile;
pub(super) use merged_squads::MergedSquadProfiles;
pub(crate) use values::{JoinAttachmentProfile, JoinDatabaseProfiles};

impl JoinKind {
    fn from_action(action: &Action) -> Self {
        let Some(kind) = action.join_type.as_ref().map(|join| join.kind.trim()) else {
            return Self::Follow;
        };
        if kind.eq_ignore_ascii_case("Merge") {
            Self::Merge
        } else if kind.eq_ignore_ascii_case("Board") {
            Self::Board
        } else if kind.eq_ignore_ascii_case("FollowAttack") {
            Self::FollowAttack
        } else {
            // Retail initializes proto actions to Follow and leaves that value
            // unchanged for an absent or unrecognized JoinType string.
            Self::Follow
        }
    }
}

impl JoinMergeType {
    fn from_action(action: &Action) -> Self {
        if action
            .merge_type
            .as_deref()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("Air"))
        {
            Self::Air
        } else {
            // Retail initializes the field to Ground and ignores unknown text.
            Self::Ground
        }
    }
}

/// Immutable authored inputs for one context-selected retail Join action.
#[derive(Debug, Clone)]
pub struct JoinActionProfile {
    action_name: String,
    kind: JoinKind,
    merge_type: JoinMergeType,
    work_range: f32,
    revert_damage_fraction: f32,
    veterancy_override: bool,
    board_time: f32,
    board_animation: Option<String>,
    unjoin_max_distance: f32,
    levels: i32,
    damage_modifier: f32,
    damage_taken_modifier: f32,
    damage_by_combat_value: bool,
    attachment: Option<ProtoObjectRef>,
}

impl JoinActionProfile {
    fn from_action(action: &Action) -> Self {
        let join = action.join_type.as_ref();
        let modifiers = action.damage_modifiers.as_ref();
        Self {
            action_name: action.name.clone(),
            kind: JoinKind::from_action(action),
            merge_type: JoinMergeType::from_action(action),
            work_range: nonnegative(action.work_range),
            revert_damage_fraction: nonnegative(join.and_then(|join| join.revert_damage_pct)),
            veterancy_override: join.and_then(|join| join.veterancy_override) == Some(true),
            board_time: nonnegative(join.and_then(|join| join.board_time)),
            board_animation: join
                .and_then(|join| join.board_anim.as_deref())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            unjoin_max_distance: nonnegative(join.and_then(|join| join.unjoin_max_dist)),
            levels: join.and_then(|join| join.levels).unwrap_or_default(),
            damage_modifier: finite_or(modifiers.and_then(|value| value.damage), 1.0),
            damage_taken_modifier: finite_or(modifiers.and_then(|value| value.damage_taken), 1.0),
            damage_by_combat_value: modifiers.and_then(|value| value.by_combat_value) == Some(true),
            attachment: action.proto_object.clone(),
        }
    }

    /// Authored action name used by technology enable/disable effects.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Join behavior selected by `JoinType`.
    #[must_use]
    pub const fn kind(&self) -> JoinKind {
        self.kind
    }

    /// Independent target occupancy channel selected by `MergeType`.
    #[must_use]
    pub const fn merge_type(&self) -> JoinMergeType {
        self.merge_type
    }

    /// Horizontal distance at which the Join connects.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Fraction of maximum HP dealt to the joining unit on reversion.
    #[must_use]
    pub const fn revert_damage_fraction(&self) -> f32 {
        self.revert_damage_fraction
    }

    /// Whether boarding transfers the Spartan veterancy contract.
    #[must_use]
    pub const fn veterancy_override(&self) -> bool {
        self.veterancy_override
    }

    /// Authored non-fatality boarding delay in seconds.
    #[must_use]
    pub const fn board_time(&self) -> f32 {
        self.board_time
    }

    /// Presentation animation requested during timed boarding.
    #[must_use]
    pub fn board_animation(&self) -> Option<&str> {
        self.board_animation.as_deref()
    }

    /// Maximum placement search distance when a boarded unit unjoins.
    #[must_use]
    pub const fn unjoin_max_distance(&self) -> f32 {
        self.unjoin_max_distance
    }

    /// Number of joining-unit veterancy levels applied to the target.
    #[must_use]
    pub const fn levels(&self) -> i32 {
        self.levels
    }

    /// Damage multiplier applied to target squadmates while joined.
    #[must_use]
    pub const fn damage_modifier(&self) -> f32 {
        self.damage_modifier
    }

    /// Incoming-damage multiplier applied to target squadmates while joined.
    #[must_use]
    pub const fn damage_taken_modifier(&self) -> f32 {
        self.damage_taken_modifier
    }

    /// Whether modifiers are derived from source/target combat-value ratio.
    #[must_use]
    pub const fn damage_by_combat_value(&self) -> bool {
        self.damage_by_combat_value
    }

    /// Optional effect object attached to a successfully boarded target.
    #[must_use]
    pub const fn attachment(&self) -> Option<&ProtoObjectRef> {
        self.attachment.as_ref()
    }
}

impl GameplayCatalog {
    /// Resolve the scenario-layered synthetic squad produced by a Merge join.
    #[must_use]
    pub fn merged_squad_profile(
        &self,
        joining_proto_squad: &str,
        target_proto_squad: &str,
    ) -> Option<&MergedSquadProfile> {
        self.merged_squads
            .resolve(joining_proto_squad, target_proto_squad)
    }

    pub(crate) fn resolve_join_damage_modifiers(
        &self,
        action: &JoinActionProfile,
        joining_proto_object: &str,
        target_proto_squad: &str,
    ) -> (f32, f32) {
        if !action.damage_by_combat_value() {
            return (action.damage_modifier(), action.damage_taken_modifier());
        }
        self.veterancy.join_modifiers(
            joining_proto_object,
            target_proto_squad,
            action.damage_modifier(),
            action.damage_taken_modifier(),
        )
    }

    pub(crate) fn resolve_join_attachment(
        &self,
        action: &JoinActionProfile,
    ) -> Option<&JoinAttachmentProfile> {
        self.join_database.resolve_attachment(action.attachment())
    }

    pub(crate) fn join_veterancy_modifiers(
        &self,
        proto_object: &str,
        start_level: i32,
        target_level: i32,
    ) -> crate::entities::UnitScalarModifiers {
        self.veterancy
            .object_modifiers(proto_object, start_level, target_level)
    }

    /// Select a context-valid Join through authored tactic target-rule order.
    #[must_use]
    pub fn select_join_action(
        &self,
        proto_object_name: &str,
        query: &AttackQuery<'_>,
        action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<JoinActionProfile> {
        self.select_join_action_for_squads(proto_object_name, None, None, query, action_is_enabled)
    }

    /// Select Join while applying retail proto-squad Merge compatibility.
    #[must_use]
    pub fn select_join_action_for_squads(
        &self,
        proto_object_name: &str,
        joining_proto_squad: Option<&str>,
        target_proto_squad: Option<&str>,
        query: &AttackQuery<'_>,
        mut action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<JoinActionProfile> {
        let action = self
            .select_work_action(proto_object_name, query, &mut action_is_enabled)
            .or_else(|| {
                let (joining, target) = joining_proto_squad.zip(target_proto_squad)?;
                self.merged_squad_profile(joining, target)?;
                unique_join_action(
                    self,
                    proto_object_name,
                    &mut action_is_enabled,
                    Some(JoinKind::Merge),
                )
            })
            .or_else(|| {
                unique_join_action(self, proto_object_name, &mut action_is_enabled, None)
            })?;
        action
            .action_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("Join"))
            .then(|| JoinActionProfile::from_action(action))
    }

    #[cfg(test)]
    pub(crate) fn load_test_merged_squads_document(
        &mut self,
        database: &pipeline::database::hw1::Database,
        document: &pipeline::xmb::Document,
    ) {
        self.merged_squads = MergedSquadProfiles::from_document(database, document);
    }
}

fn unique_join_action<'a>(
    gameplay: &'a GameplayCatalog,
    proto_object_name: &str,
    action_is_enabled: &mut impl FnMut(&Action) -> bool,
    kind: Option<JoinKind>,
) -> Option<&'a Action> {
    let mut joins = gameplay
        .object(proto_object_name)?
        .tactics
        .actions
        .iter()
        .filter(|action| {
            is_join(action)
                && kind.is_none_or(|expected| JoinKind::from_action(action) == expected)
                && action_is_enabled(action)
        });
    let first = joins.next()?;
    joins.next().is_none().then_some(first)
}

fn is_join(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Join"))
}

fn nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn finite_or(value: Option<f32>, fallback: f32) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or(fallback)
}

#[cfg(test)]
mod tests;
