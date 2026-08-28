//! Runtime evaluation of authored tactic target rules.

use super::{AttackProfile, GameplayCatalog, ObjectGameplay, RangedAction, is_ranged_attack};
use crate::entities::SquadMode;
use pipeline::database::hw1::tactics::{Action, TargetRule};
use pipeline::database::hw1::{Database, ProtoObject};
use std::collections::BTreeMap;

/// Relationship from the acting player to the target player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TacticRelation {
    /// The target belongs to the acting player.
    SelfPlayer,
    /// The target belongs to an allied player.
    Ally,
    /// The target belongs to an enemy player.
    Enemy,
    /// The target belongs to a diplomatically neutral player.
    Neutral,
}

/// Boolean runtime conditions consumed by tactic-rule evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttackQueryFlags(u16);

impl AttackQueryFlags {
    /// The opportunity was selected automatically rather than clicked.
    pub const AUTO_TARGET: Self = Self(1 << 0);
    /// The target is owned by Gaia/player zero.
    pub const TARGET_GAIA: Self = Self(1 << 1);
    /// Current hit points are below maximum hit points.
    pub const TARGET_DAMAGED: Self = Self(1 << 2);
    /// An authored manual-build target is not yet built.
    pub const TARGET_UNBUILT: Self = Self(1 << 3);
    /// The acting squad can currently capture the target.
    pub const TARGET_CAPTURABLE: Self = Self(1 << 4);
    /// An active shield is using the Shielded damage type.
    pub const TARGET_SHIELDED: Self = Self(1 << 5);
    /// Runtime state marks the target invulnerable.
    pub const TARGET_INVULNERABLE: Self = Self(1 << 6);
    /// The target is currently melee-attacking the acting unit.
    pub const TARGET_MELEE_ATTACKER: Self = Self(1 << 7);
    /// The acting unit simultaneously has contained and attached units.
    pub const SOURCE_CONTAINED_AND_ATTACHED: Self = Self(1 << 8);

    /// Return an empty condition set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Add one condition to this set.
    pub fn insert(&mut self, condition: Self) {
        self.0 |= condition.0;
    }

    /// Return whether this set contains a condition.
    #[must_use]
    pub const fn contains(self, condition: Self) -> bool {
        self.0 & condition.0 == condition.0
    }
}

/// Dynamic inputs used to evaluate one object-targeted attack opportunity.
#[derive(Debug, Clone, Copy)]
pub struct AttackQuery<'target> {
    /// Player relationship to the target.
    pub relation: TacticRelation,
    /// Current or changing squad mode.
    pub squad_mode: SquadMode,
    /// Requested ability database index, if this is an ability order.
    pub ability_id: Option<u8>,
    /// Concrete target proto-object name.
    pub target_proto_object_name: Option<&'target str>,
    /// Dynamic boolean conditions used by rule predicates.
    pub flags: AttackQueryFlags,
}

impl Default for AttackQuery<'_> {
    fn default() -> Self {
        Self {
            relation: TacticRelation::Enemy,
            squad_mode: SquadMode::Normal,
            ability_id: None,
            target_proto_object_name: None,
            flags: AttackQueryFlags::empty(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct ObjectTargetTraits {
    proto_object_name: String,
    damage_type: Option<String>,
    object_class: Option<String>,
    object_types: Vec<String>,
    neutral: bool,
    invulnerable: bool,
    invulnerable_when_gaia: bool,
}

impl GameplayCatalog {
    /// Evaluate ranged target rules in authored order and return the selected action.
    #[must_use]
    pub fn select_ranged_action<'catalog>(
        &'catalog self,
        proto_object_name: &str,
        query: &AttackQuery<'_>,
        action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<RangedAction<'catalog>> {
        self.object(proto_object_name)?.select_ranged_action(
            query,
            self.ability_name(query.ability_id),
            self.target_traits(query.target_proto_object_name),
            action_is_enabled,
        )
    }

    /// Evaluate target rules and join the selected action to its attack timing.
    #[must_use]
    pub fn select_attack_profile(
        &self,
        proto_object_name: &str,
        query: &AttackQuery<'_>,
        action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<&AttackProfile> {
        let object = self.object(proto_object_name)?;
        let action = object.select_ranged_action(
            query,
            self.ability_name(query.ability_id),
            self.target_traits(query.target_proto_object_name),
            action_is_enabled,
        )?;
        object.attack_profile(&action.action.name)
    }

    pub(super) fn initial_ranged_action_for(
        &self,
        proto_object_name: &str,
    ) -> Option<RangedAction<'_>> {
        self.select_ranged_action(proto_object_name, &AttackQuery::default(), authored_enabled)
    }

    fn ability_name(&self, ability_id: Option<u8>) -> Option<&str> {
        ability_id
            .and_then(|id| self.ability_names.get(usize::from(id)))
            .map(String::as_str)
    }

    fn target_traits(&self, target_name: Option<&str>) -> Option<&ObjectTargetTraits> {
        self.target_traits.get(&target_name?.to_ascii_lowercase())
    }
}

impl ObjectGameplay {
    fn select_ranged_action<'catalog>(
        &'catalog self,
        query: &AttackQuery<'_>,
        requested_ability: Option<&str>,
        target: Option<&ObjectTargetTraits>,
        mut action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<RangedAction<'catalog>> {
        let mut fallback = None;
        if let Some(tactic) = &self.tactics.tactic {
            for rule in &tactic.target_rules {
                let Some(action) = self.resolve_rule_action(rule) else {
                    continue;
                };
                if !action_is_enabled(action.action)
                    || !rule_matches(rule, action.action, query, requested_ability, target)
                {
                    continue;
                }
                if query.ability_id.is_none() || required_ability(rule).is_some() {
                    return Some(action);
                }
                fallback.get_or_insert(action);
            }
        }

        fallback
            .or_else(|| {
                self.ranged_actions()
                    .find(|action| action.action.default == Some(true))
            })
            .or_else(|| self.unambiguous_fixture_action(action_is_enabled))
    }

    fn resolve_rule_action(&self, rule: &TargetRule) -> Option<RangedAction<'_>> {
        let name = rule.action.as_deref()?;
        let action = self
            .tactics
            .actions
            .iter()
            .find(|action| action.name.eq_ignore_ascii_case(name))?;
        is_ranged_attack(action).then(|| self.resolve_action(action))?
    }

    fn unambiguous_fixture_action(
        &self,
        mut action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<RangedAction<'_>> {
        if self.tactics.tactic.is_some() {
            return None;
        }
        let mut enabled = self
            .ranged_actions()
            .filter(|action| action_is_enabled(action.action));
        let first = enabled.next()?;
        enabled.next().is_none().then_some(first)
    }
}

fn rule_matches(
    rule: &TargetRule,
    action: &Action,
    query: &AttackQuery<'_>,
    requested_ability: Option<&str>,
    target: Option<&ObjectTargetTraits>,
) -> bool {
    if query.flags.contains(AttackQueryFlags::AUTO_TARGET) && action.no_auto_target == Some(true) {
        return false;
    }
    if !ability_matches(rule, query.ability_id.is_some(), requested_ability) {
        return false;
    }
    if !squad_mode_matches(rule, query) {
        return false;
    }
    if rule.merge_squads == Some(true)
        || (rule.contains_units == Some(true)
            && query
                .flags
                .contains(AttackQueryFlags::SOURCE_CONTAINED_AND_ATTACHED))
    {
        return false;
    }
    if rule.targets_ground == Some(true)
        && (query.flags.contains(AttackQueryFlags::AUTO_TARGET) || action.target_air != Some(true))
    {
        return false;
    }
    if !ownership_matches(rule, query, target) || !target_state_matches(rule, query) {
        return false;
    }
    if is_ranged_attack(action)
        && (query.flags.contains(AttackQueryFlags::TARGET_INVULNERABLE)
            || target.is_some_and(|traits| {
                traits.is_invulnerable(query.flags.contains(AttackQueryFlags::TARGET_GAIA))
            }))
    {
        return false;
    }
    if rule.melee_attacker == Some(true)
        && !query
            .flags
            .contains(AttackQueryFlags::TARGET_MELEE_ATTACKER)
    {
        return false;
    }
    target_type_matches(rule, query, target)
}

fn ability_matches(
    rule: &TargetRule,
    ability_requested: bool,
    requested_ability: Option<&str>,
) -> bool {
    let Some(required) = required_ability(rule) else {
        return true;
    };
    ability_requested
        && requested_ability.is_some_and(|requested| requested.eq_ignore_ascii_case(required))
}

fn required_ability(rule: &TargetRule) -> Option<&str> {
    (rule.optional_ability.is_none())
        .then_some(rule.ability.as_deref())
        .flatten()
}

fn squad_mode_matches(rule: &TargetRule, query: &AttackQuery<'_>) -> bool {
    let (required_mode, auto_target_only) =
        if let Some(mode) = rule.auto_target_squad_mode.as_deref() {
            (Some(mode), true)
        } else {
            (rule.squad_mode.as_deref(), false)
        };
    if auto_target_only && !query.flags.contains(AttackQueryFlags::AUTO_TARGET) {
        return true;
    }
    required_mode.is_none_or(|mode| mode.eq_ignore_ascii_case(query.squad_mode.as_str()))
}

fn ownership_matches(
    rule: &TargetRule,
    query: &AttackQuery<'_>,
    target: Option<&ObjectTargetTraits>,
) -> bool {
    if rule.gaia_owned == Some(true) {
        return query.flags.contains(AttackQueryFlags::TARGET_GAIA);
    }
    match rule.relation.as_deref().unwrap_or("Enemy") {
        relation if relation.eq_ignore_ascii_case("Any") => true,
        relation if relation.eq_ignore_ascii_case("Self") => {
            query.relation == TacticRelation::SelfPlayer
        }
        relation if relation.eq_ignore_ascii_case("Ally") => {
            matches!(
                query.relation,
                TacticRelation::SelfPlayer | TacticRelation::Ally
            )
        }
        relation if relation.eq_ignore_ascii_case("Enemy") => {
            query.relation == TacticRelation::Enemy && !target.is_some_and(|traits| traits.neutral)
        }
        relation if relation.eq_ignore_ascii_case("Neutral") => {
            query.relation == TacticRelation::Enemy && target.is_some_and(|traits| traits.neutral)
        }
        _ => false,
    }
}

fn target_state_matches(rule: &TargetRule, query: &AttackQuery<'_>) -> bool {
    rule.target_states.iter().all(|state| {
        if state.eq_ignore_ascii_case("Unbuilt") {
            query.flags.contains(AttackQueryFlags::TARGET_UNBUILT)
        } else if state.eq_ignore_ascii_case("Damaged") {
            query.flags.contains(AttackQueryFlags::TARGET_DAMAGED)
        } else if state.eq_ignore_ascii_case("Capturable") {
            query.flags.contains(AttackQueryFlags::TARGET_CAPTURABLE)
        } else {
            true
        }
    })
}

fn target_type_matches(
    rule: &TargetRule,
    query: &AttackQuery<'_>,
    target: Option<&ObjectTargetTraits>,
) -> bool {
    let has_shield_rule = rule
        .damage_types
        .iter()
        .any(|kind| kind.eq_ignore_ascii_case("Shielded"));
    let has_regular_damage_rule = rule
        .damage_types
        .iter()
        .any(|kind| !kind.eq_ignore_ascii_case("Shielded"));
    if !has_shield_rule && !has_regular_damage_rule && rule.target_types.is_empty() {
        return true;
    }
    if has_shield_rule && query.flags.contains(AttackQueryFlags::TARGET_SHIELDED) {
        return true;
    }
    if query.relation == TacticRelation::Enemy
        && target.is_some_and(|traits| {
            traits.damage_type.as_deref().is_some_and(|target_type| {
                rule.damage_types.iter().any(|kind| {
                    !kind.eq_ignore_ascii_case("Shielded") && kind.eq_ignore_ascii_case(target_type)
                })
            })
        })
    {
        return true;
    }
    target.is_some_and(|traits| rule.target_types.iter().any(|kind| traits.is_type(kind)))
}

impl ObjectTargetTraits {
    fn from_proto(object: &ProtoObject) -> Self {
        Self {
            proto_object_name: object.name.clone(),
            damage_type: object.damage_type.clone(),
            object_class: object.object_class.clone(),
            object_types: object.object_types.clone(),
            neutral: has_flag(object, "Neutral"),
            invulnerable: has_flag(object, "Invulnerable"),
            invulnerable_when_gaia: has_flag(object, "InvulnerableWhenGaia"),
        }
    }

    fn is_type(&self, kind: &str) -> bool {
        self.proto_object_name.eq_ignore_ascii_case(kind)
            || self
                .object_class
                .as_deref()
                .is_some_and(|class| class.eq_ignore_ascii_case(kind))
            || self
                .object_types
                .iter()
                .any(|object_type| object_type.eq_ignore_ascii_case(kind))
    }

    const fn is_invulnerable(&self, is_gaia: bool) -> bool {
        self.invulnerable || (is_gaia && self.invulnerable_when_gaia)
    }
}

pub(super) fn collect_ability_names(database: &Database) -> Vec<String> {
    database
        .abilities
        .iter()
        .map(|ability| ability.name.clone())
        .collect()
}

pub(super) fn collect_target_traits(database: &Database) -> BTreeMap<String, ObjectTargetTraits> {
    database
        .objects
        .iter()
        .map(|object| {
            (
                object.name.to_ascii_lowercase(),
                ObjectTargetTraits::from_proto(object),
            )
        })
        .collect()
}

pub(super) fn proto_matches_type(
    catalog: &GameplayCatalog,
    proto_object_name: &str,
    expected_type: &str,
) -> bool {
    catalog
        .target_traits
        .get(&proto_object_name.to_ascii_lowercase())
        .is_some_and(|traits| traits.is_type(expected_type))
}

fn has_flag(object: &ProtoObject, name: &str) -> bool {
    object
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case(name))
}

fn authored_enabled(action: &Action) -> bool {
    action.start_disabled != Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::tactics::{TacticData, TacticRules, Weapon};
    use pipeline::database::hw1::{Ability, ProtoObject};
    use std::collections::BTreeMap;

    fn action(name: &str, weapon: &str, starts_disabled: bool) -> Action {
        Action {
            name: name.to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some(weapon.to_owned()),
            start_disabled: starts_disabled.then_some(true),
            ..Action::default()
        }
    }

    fn weapon(name: &str, range: f32) -> Weapon {
        Weapon {
            name: name.to_owned(),
            max_range: Some(range),
            ..Weapon::default()
        }
    }

    fn marine_catalog() -> GameplayCatalog {
        let mut database = Database::new();
        database.abilities.push(Ability {
            name: "Command".to_owned(),
            ..Ability::default()
        });
        database.objects.extend([
            ProtoObject {
                name: "marine".to_owned(),
                tactics: Some("marine.tactics".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "ground_target".to_owned(),
                object_class: Some("Unit".to_owned()),
                object_types: vec!["NonFlying".to_owned(), "Infantry".to_owned()],
                damage_type: Some("Light".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "air_target".to_owned(),
                object_class: Some("Unit".to_owned()),
                object_types: vec!["Flying".to_owned()],
                damage_type: Some("Heavy".to_owned()),
                ..ProtoObject::default()
            },
        ]);
        let tactics = TacticData {
            weapons: vec![
                weapon("CoverRocket", 60.0),
                weapon("CoverGrenade", 60.0),
                weapon("CoverRifle", 60.0),
                weapon("Rifle", 25.0),
                weapon("Grenade", 35.0),
                weapon("Rocket", 50.0),
            ],
            actions: vec![
                action("CoverRocket", "CoverRocket", true),
                action("CoverGrenade", "CoverGrenade", false),
                action("CoverRifle", "CoverRifle", false),
                action("Rifle", "Rifle", false),
                action("Grenade", "Grenade", false),
                action("Rocket", "Rocket", true),
            ],
            tactic: Some(TacticRules {
                target_rules: vec![
                    rule("CoverRocket", Some("Cover"), None, &[]),
                    rule("CoverGrenade", Some("Cover"), None, &["NonFlying"]),
                    rule("CoverRifle", Some("Cover"), None, &[]),
                    rule("Rifle", Some("Normal"), None, &[]),
                    rule("Grenade", Some("Normal"), Some("Command"), &["NonFlying"]),
                    rule("Rocket", Some("Normal"), Some("Command"), &[]),
                ],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        GameplayCatalog::from_tactics(&database, [("marine".to_owned(), tactics)])
    }

    fn rule(
        action: &str,
        mode: Option<&str>,
        ability: Option<&str>,
        target_types: &[&str],
    ) -> TargetRule {
        TargetRule {
            relation: Some("Enemy".to_owned()),
            squad_mode: mode.map(str::to_owned),
            action: Some(action.to_owned()),
            ability: ability.map(str::to_owned),
            target_types: target_types.iter().map(|kind| (*kind).to_owned()).collect(),
            ..TargetRule::default()
        }
    }

    fn selected<'catalog>(
        catalog: &'catalog GameplayCatalog,
        query: &AttackQuery<'_>,
        overrides: &BTreeMap<&str, bool>,
    ) -> Option<&'catalog str> {
        catalog
            .select_ranged_action("marine", query, |action| {
                overrides
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(&action.name))
                    .map_or(action.start_disabled != Some(true), |entry| *entry.1)
            })
            .map(|action| action.action.name.as_str())
    }

    #[test]
    fn marine_rules_switch_by_mode_ability_target_and_live_enablement() {
        let catalog = marine_catalog();
        let mut query = AttackQuery {
            target_proto_object_name: Some("ground_target"),
            ..AttackQuery::default()
        };
        let mut overrides = BTreeMap::new();

        assert_eq!(selected(&catalog, &query, &overrides), Some("Rifle"));

        query.ability_id = Some(0);
        assert_eq!(selected(&catalog, &query, &overrides), Some("Grenade"));

        overrides.insert("Grenade", false);
        overrides.insert("Rocket", true);
        assert_eq!(selected(&catalog, &query, &overrides), Some("Rocket"));

        query.squad_mode = SquadMode::Cover;
        query.ability_id = None;
        assert_eq!(selected(&catalog, &query, &overrides), Some("CoverGrenade"));

        query.target_proto_object_name = Some("air_target");
        assert_eq!(selected(&catalog, &query, &overrides), Some("CoverRifle"));
    }

    #[test]
    fn ability_request_falls_back_to_first_matching_non_ability_rule() {
        let catalog = marine_catalog();
        let query = AttackQuery {
            ability_id: Some(0),
            target_proto_object_name: Some("ground_target"),
            ..AttackQuery::default()
        };
        let overrides = BTreeMap::from([("Grenade", false), ("Rocket", false)]);

        assert_eq!(selected(&catalog, &query, &overrides), Some("Rifle"));
    }

    #[test]
    fn damage_object_and_state_predicates_follow_retail_or_and_order_rules() {
        let mut database = Database::new();
        database.objects.extend([
            ProtoObject {
                name: "attacker".to_owned(),
                tactics: Some("attacker.tactics".to_owned()),
                ..ProtoObject::default()
            },
            ProtoObject {
                name: "heavy_vehicle".to_owned(),
                object_class: Some("Unit".to_owned()),
                object_types: vec!["Vehicle".to_owned()],
                damage_type: Some("Heavy".to_owned()),
                ..ProtoObject::default()
            },
        ]);
        let tactics = TacticData {
            weapons: vec![weapon("Damaged", 10.0), weapon("Special", 20.0)],
            actions: vec![
                action("Damaged", "Damaged", false),
                action("Special", "Special", false),
            ],
            tactic: Some(TacticRules {
                target_rules: vec![
                    TargetRule {
                        action: Some("Damaged".to_owned()),
                        target_states: vec!["Damaged".to_owned()],
                        ..TargetRule::default()
                    },
                    TargetRule {
                        action: Some("Special".to_owned()),
                        damage_types: vec!["Light".to_owned()],
                        target_types: vec!["Vehicle".to_owned()],
                        ..TargetRule::default()
                    },
                ],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let catalog = GameplayCatalog::from_tactics(&database, [("attacker".to_owned(), tactics)]);
        let query = AttackQuery {
            target_proto_object_name: Some("heavy_vehicle"),
            ..AttackQuery::default()
        };
        assert_eq!(
            catalog
                .select_ranged_action("attacker", &query, authored_enabled)
                .map(|action| action.action.name.as_str()),
            Some("Special")
        );
        assert_eq!(
            catalog
                .select_ranged_action(
                    "attacker",
                    &{
                        let mut damaged = query;
                        damaged.flags.insert(AttackQueryFlags::TARGET_DAMAGED);
                        damaged
                    },
                    authored_enabled,
                )
                .map(|action| action.action.name.as_str()),
            Some("Damaged")
        );
    }

    #[test]
    fn auto_target_squad_mode_is_only_a_gate_for_automatic_selection() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "unit".to_owned(),
            tactics: Some("unit.tactics".to_owned()),
            ..ProtoObject::default()
        });
        let tactics = TacticData {
            weapons: vec![weapon("Mode", 10.0), weapon("Normal", 10.0)],
            actions: vec![
                action("Mode", "Mode", false),
                action("Normal", "Normal", false),
            ],
            tactic: Some(TacticRules {
                target_rules: vec![
                    TargetRule {
                        auto_target_squad_mode: Some("Cover".to_owned()),
                        action: Some("Mode".to_owned()),
                        ..TargetRule::default()
                    },
                    TargetRule {
                        action: Some("Normal".to_owned()),
                        ..TargetRule::default()
                    },
                ],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let catalog = GameplayCatalog::from_tactics(&database, [("unit".to_owned(), tactics)]);
        let manual = AttackQuery::default();
        let mut automatic = AttackQuery::default();
        automatic.flags.insert(AttackQueryFlags::AUTO_TARGET);

        assert_eq!(
            catalog
                .select_ranged_action("unit", &manual, authored_enabled)
                .map(|action| action.action.name.as_str()),
            Some("Mode")
        );
        assert_eq!(
            catalog
                .select_ranged_action("unit", &automatic, authored_enabled)
                .map(|action| action.action.name.as_str()),
            Some("Normal")
        );
    }
}
