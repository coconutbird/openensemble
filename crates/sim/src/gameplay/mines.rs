//! Authored Mines abilities joined to their unit tactic actions.

use super::{AttackQuery, GameplayCatalog};
use pipeline::database::hw1::tactics::Action;

const DEFAULT_WORK_RANGE: f32 = 0.1;

/// Immutable data needed by one retail `BUnitActionMines` execution.
#[derive(Debug, Clone, PartialEq)]
pub struct MineActionProfile {
    action_name: String,
    mine_object_name: String,
    work_range: f32,
    ammunition_cost: f32,
}

impl MineActionProfile {
    /// Return the selected tactic action name.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Return the first proto object authored by the ability.
    #[must_use]
    pub fn mine_object_name(&self) -> &str {
        &self.mine_object_name
    }

    /// Return the fallback range used when the command supplies none.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Return the ammunition charged for each successfully placed object.
    #[must_use]
    pub const fn ammunition_cost(&self) -> f32 {
        self.ammunition_cost
    }
}

impl GameplayCatalog {
    /// Select a context-valid `Mines` tactic action and join its ability data.
    #[must_use]
    pub fn select_mine_action(
        &self,
        proto_object_name: &str,
        query: &AttackQuery<'_>,
        action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<MineActionProfile> {
        let ability = self.resolve_order_ability(proto_object_name, query.ability_id?)?;
        // Tactic rules compare the requested ID (including generic Command),
        // while the unit action reads object and ammo data from AbilityCommand.
        let action = self.select_work_action(proto_object_name, query, action_is_enabled)?;
        action
            .action_type
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("Mines"))
            .then_some(())?;
        let mine_object_name = ability.objects().first()?.trim();
        if mine_object_name.is_empty() {
            return None;
        }
        Some(MineActionProfile {
            action_name: action.name.clone(),
            mine_object_name: mine_object_name.to_owned(),
            work_range: action
                .work_range
                .filter(|range| range.is_finite() && *range >= 0.0)
                .unwrap_or(DEFAULT_WORK_RANGE),
            ammunition_cost: ability.ammunition_cost(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::SquadMode;
    use pipeline::database::hw1::tactics::{TacticData, TacticRules, TargetRule};
    use pipeline::database::hw1::{Ability, Database, ProtoObject};

    #[test]
    fn command_mapping_joins_mines_action_object_cost_and_default_range() {
        let mut database = Database::new();
        database.abilities.extend([
            Ability {
                name: "Command".to_owned(),
                ..Ability::default()
            },
            Ability {
                name: "LayMines".to_owned(),
                ability_type: Some("Work".to_owned()),
                target_type: Some("Location".to_owned()),
                objects: vec!["test_mine".to_owned()],
                ammo_cost: Some(12.5),
                ..Ability::default()
            },
        ]);
        database.objects.push(ProtoObject {
            name: "minelayer".to_owned(),
            ability_command: Some("LayMines".to_owned()),
            tactics: Some("minelayer.tactics".to_owned()),
            ..ProtoObject::default()
        });
        let tactics = TacticData {
            actions: vec![Action {
                name: "PlaceMine".to_owned(),
                action_type: Some("Mines".to_owned()),
                ..Action::default()
            }],
            tactic: Some(TacticRules {
                target_rules: vec![TargetRule {
                    action: Some("PlaceMine".to_owned()),
                    ability: Some("Command".to_owned()),
                    relation: Some("Any".to_owned()),
                    ..TargetRule::default()
                }],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let gameplay =
            GameplayCatalog::from_tactics(&database, [("minelayer".to_owned(), tactics)]);
        let query = AttackQuery {
            relation: super::super::TacticRelation::SelfPlayer,
            squad_mode: SquadMode::Normal,
            ability_id: Some(0),
            target_proto_object_name: None,
            tactic_state: None,
            flags: super::super::AttackQueryFlags::empty(),
        };

        let profile = gameplay
            .select_mine_action("minelayer", &query, |_| true)
            .expect("mapped Mines action");
        assert_eq!(profile.action_name(), "PlaceMine");
        assert_eq!(profile.mine_object_name(), "test_mine");
        assert!((profile.ammunition_cost() - 12.5).abs() < f32::EPSILON);
        assert!((profile.work_range() - DEFAULT_WORK_RANGE).abs() < f32::EPSILON);
    }
}
