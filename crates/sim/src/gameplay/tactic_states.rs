//! Authored tactic-state lookup and action-membership rules.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::{Action, TacticState};

/// Retail's per-unit `uint8` index into one prototype's tactic states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TacticStateId(u8);

impl TacticStateId {
    /// Convert a parsed tactic-state index to retail's synchronized width.
    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        u8::try_from(index).ok().map(Self)
    }

    /// Return the zero-based index in the owning prototype's tactic data.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Return retail's serialized 8-bit value.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

/// One immutable state from the scenario-layered tactic catalog.
#[derive(Debug, Clone, Copy)]
pub struct TacticStateProfile<'gameplay> {
    id: TacticStateId,
    state: &'gameplay TacticState,
}

impl<'gameplay> TacticStateProfile<'gameplay> {
    /// Return the checksummed state ID stored on a unit.
    #[must_use]
    pub const fn id(self) -> TacticStateId {
        self.id
    }

    /// Return the authored state name.
    #[must_use]
    pub fn name(self) -> &'gameplay str {
        &self.state.name
    }

    /// Return the optional idle-animation override.
    #[must_use]
    pub fn idle_animation(self) -> Option<&'gameplay str> {
        self.state.idle_anim.as_deref()
    }

    /// Return the optional walking-animation override.
    #[must_use]
    pub fn walk_animation(self) -> Option<&'gameplay str> {
        self.state.walk_anim.as_deref()
    }

    /// Return the optional jogging-animation override.
    #[must_use]
    pub fn jog_animation(self) -> Option<&'gameplay str> {
        self.state.jog_anim.as_deref()
    }

    /// Return the optional running-animation override.
    #[must_use]
    pub fn run_animation(self) -> Option<&'gameplay str> {
        self.state.run_anim.as_deref()
    }

    /// Return the optional death-animation override.
    #[must_use]
    pub fn death_animation(self) -> Option<&'gameplay str> {
        self.state.death_anim.as_deref()
    }
}

impl GameplayCatalog {
    /// Resolve a named tactic state for one prototype.
    #[must_use]
    pub fn tactic_state_id(
        &self,
        proto_object_name: &str,
        state_name: &str,
    ) -> Option<TacticStateId> {
        self.object(proto_object_name)?.tactic_state_id(state_name)
    }

    /// Resolve one indexed tactic state and its presentation overrides.
    #[must_use]
    pub fn tactic_state(
        &self,
        proto_object_name: &str,
        state_id: TacticStateId,
    ) -> Option<TacticStateProfile<'_>> {
        self.object(proto_object_name)?.tactic_state(state_id)
    }
}

impl ObjectGameplay {
    pub(super) fn tactic_state_id(&self, state_name: &str) -> Option<TacticStateId> {
        self.tactics
            .states
            .iter()
            .position(|state| state.name.eq_ignore_ascii_case(state_name))
            .and_then(TacticStateId::from_index)
    }

    pub(super) fn tactic_state(&self, id: TacticStateId) -> Option<TacticStateProfile<'_>> {
        self.tactics
            .states
            .get(id.index())
            .map(|state| TacticStateProfile { id, state })
    }

    pub(super) fn action_available_in_tactic_state(
        &self,
        state_id: Option<TacticStateId>,
        action: &Action,
    ) -> bool {
        let Some(state) = state_id.and_then(|id| self.tactics.states.get(id.index())) else {
            return true;
        };
        state.actions.is_empty()
            || state
                .actions
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&action.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AttackQuery, TacticRelation};
    use pipeline::database::hw1::tactics::{TacticData, TacticRules, TargetRule};
    use pipeline::database::hw1::{Database, ProtoObject};

    #[test]
    fn state_lookup_preserves_animation_overrides_and_retail_membership_fallback() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "unit".to_owned(),
            tactics: Some("unit.tactics".to_owned()),
            ..ProtoObject::default()
        });
        let tactics = TacticData {
            actions: vec![action("Other"), action("Allowed")],
            states: vec![
                TacticState {
                    name: "Restricted".to_owned(),
                    run_anim: Some("Charge".to_owned()),
                    actions: vec!["allowed".to_owned()],
                    ..TacticState::default()
                },
                TacticState {
                    name: "Unrestricted".to_owned(),
                    ..TacticState::default()
                },
            ],
            tactic: Some(TacticRules {
                target_rules: vec![target_rule("Other"), target_rule("Allowed")],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let catalog = GameplayCatalog::from_tactics(&database, [("unit".to_owned(), tactics)]);
        let object = catalog.object("unit").unwrap();
        let restricted = catalog.tactic_state_id("UNIT", "restricted").unwrap();
        let unrestricted = catalog.tactic_state_id("unit", "Unrestricted").unwrap();

        let profile = catalog.tactic_state("unit", restricted).unwrap();
        assert_eq!(profile.name(), "Restricted");
        assert_eq!(profile.run_animation(), Some("Charge"));
        assert!(object.action_available_in_tactic_state(None, &action("Other")));
        assert!(object.action_available_in_tactic_state(Some(restricted), &action("ALLOWED")));
        assert!(!object.action_available_in_tactic_state(Some(restricted), &action("Other")));
        assert!(object.action_available_in_tactic_state(Some(unrestricted), &action("Other")));

        let mut query = AttackQuery {
            relation: TacticRelation::Enemy,
            ..AttackQuery::default()
        };
        assert_eq!(selected_action(&catalog, &query), Some("Other"));
        query.tactic_state = Some(restricted);
        assert_eq!(selected_action(&catalog, &query), Some("Allowed"));
        query.tactic_state = Some(unrestricted);
        assert_eq!(selected_action(&catalog, &query), Some("Other"));
    }

    fn action(name: &str) -> Action {
        Action {
            name: name.to_owned(),
            ..Action::default()
        }
    }

    fn target_rule(action: &str) -> TargetRule {
        TargetRule {
            action: Some(action.to_owned()),
            ..TargetRule::default()
        }
    }

    fn selected_action<'catalog>(
        catalog: &'catalog GameplayCatalog,
        query: &AttackQuery<'_>,
    ) -> Option<&'catalog str> {
        catalog
            .select_work_action("unit", query, |_| true)
            .map(|action| action.name.as_str())
    }
}
