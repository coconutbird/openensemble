//! Immutable retail revival definitions resolved from layered database data.

use pipeline::database::hw1::tactics::Action;
use pipeline::database::hw1::{Database, ProtoObject};

/// Global game-data values used by the hero-death action.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HeroRevivalProfile {
    /// Time in seconds for a downed hero to regenerate from empty to full HP.
    pub hp_regen_time: f32,
    /// Radius in which another live allied squad permits revival.
    pub revival_distance: f32,
    /// Fraction of maximum HP required before the ally check may succeed.
    pub hitpoint_threshold: f32,
}

/// Authored values used by a tactic's persistent `Revive` action.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReviveActionProfile {
    /// Delay after damage before ordinary regeneration resumes.
    pub revive_delay: f32,
    /// Delay before a zero-HP hibernating unit becomes trigger-alive again.
    pub hibernate_delay: f32,
    /// Hit points restored per second while the action is working.
    pub revive_rate: f32,
}

/// Revival behavior owned by one unit prototype.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnitRevivalProfile {
    /// Retail `_HeroDeath` behavior.
    Hero(HeroRevivalProfile),
    /// A persistent tactic `Revive` action.
    Revive(ReviveActionProfile),
}

pub(super) fn hero_profile(database: &Database) -> HeroRevivalProfile {
    let game_data = database.game_data.as_ref();
    HeroRevivalProfile {
        hp_regen_time: game_data
            .and_then(|data| data.hero_hp_regen_time)
            .and_then(nonnegative)
            .unwrap_or_default(),
        revival_distance: game_data
            .and_then(|data| data.hero_revival_distance)
            .and_then(nonnegative)
            .unwrap_or_default(),
        hitpoint_threshold: game_data
            .and_then(|data| data.hero_percent_hp_revival_threshhold)
            .and_then(nonnegative)
            .unwrap_or_default()
            .clamp(0.0, 1.0),
    }
}

pub(super) fn is_hero_death_object(database: &Database, object: &ProtoObject) -> bool {
    let Some(hero_type) = database
        .game_data
        .as_ref()
        .and_then(|data| data.code_object_types.as_ref())
        .and_then(|types| {
            types
                .entries
                .iter()
                .find(|entry| entry.object_type.trim().eq_ignore_ascii_case("HeroDeath"))
        })
        .map(|entry| entry.value.trim())
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    object
        .object_types
        .iter()
        .any(|object_type| object_type.trim().eq_ignore_ascii_case(hero_type))
}

pub(super) fn revive_action_profile(actions: &[Action]) -> Option<ReviveActionProfile> {
    let action = actions.iter().find(|action| {
        action
            .action_type
            .as_deref()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("Revive"))
    })?;
    Some(ReviveActionProfile {
        revive_delay: action
            .revive_delay
            .and_then(nonnegative)
            .unwrap_or_default(),
        hibernate_delay: action
            .hibernate_revive_delay
            .and_then(nonnegative)
            .unwrap_or_default(),
        revive_rate: action.revive_rate.and_then(nonnegative).unwrap_or_default(),
    })
}

fn nonnegative(value: f32) -> Option<f32> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::GameData;
    use pipeline::database::hw1::gamedata::{CodeObjectType, CodeObjectTypesWrapper};
    use pipeline::database::hw1::tactics::TacticData;

    #[test]
    fn layered_game_data_and_tactics_resolve_distinct_revival_profiles() {
        let mut database = Database::new();
        database.game_data = Some(GameData {
            code_object_types: Some(CodeObjectTypesWrapper {
                entries: vec![CodeObjectType {
                    object_type: "HeroDeath".to_owned(),
                    value: "_HeroDeath".to_owned(),
                }],
            }),
            hero_hp_regen_time: Some(90.0),
            hero_revival_distance: Some(15.0),
            hero_percent_hp_revival_threshhold: Some(0.5),
            ..GameData::default()
        });
        database.objects.extend([
            ProtoObject {
                name: "test_hero".to_owned(),
                tactics: Some("test_hero.tactics".to_owned()),
                object_types: vec!["_HeroDeath".to_owned()],
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "test_reviver".to_owned(),
                tactics: Some("test_reviver.tactics".to_owned()),
                ..ProtoObject::default()
            },
        ]);
        let revive = Action {
            action_type: Some("Revive".to_owned()),
            revive_delay: Some(4.0),
            hibernate_revive_delay: Some(2.0),
            revive_rate: Some(8.0),
            ..Action::default()
        };
        let catalog = crate::gameplay::GameplayCatalog::from_tactics(
            &database,
            [
                (
                    "test_hero".to_owned(),
                    TacticData {
                        actions: vec![revive.clone()],
                        ..TacticData::default()
                    },
                ),
                (
                    "test_reviver".to_owned(),
                    TacticData {
                        actions: vec![revive],
                        ..TacticData::default()
                    },
                ),
            ],
        );

        assert_eq!(
            catalog.unit_revival_profile("TEST_HERO"),
            Some(UnitRevivalProfile::Hero(HeroRevivalProfile {
                hp_regen_time: 90.0,
                revival_distance: 15.0,
                hitpoint_threshold: 0.5,
            }))
        );
        assert_eq!(
            catalog.unit_revival_profile("test_reviver"),
            Some(UnitRevivalProfile::Revive(ReviveActionProfile {
                revive_delay: 4.0,
                hibernate_delay: 2.0,
                revive_rate: 8.0,
            }))
        );
    }
}
