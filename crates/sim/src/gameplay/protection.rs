//! Scenario-layered protection definitions used by persistent shield actions.

use super::{GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
use pipeline::xmb::{Document, Node};
use std::collections::BTreeMap;

/// Immutable data needed to run one authored `PlasmaShieldGen` action.
#[derive(Debug, Clone)]
pub struct PlasmaShieldGeneratorProfile {
    generator_proto_object_name: String,
    shield_proto_object_name: String,
    shield_proto_object_index: usize,
    shield_proto_object: ProtoObject,
    rebuild_time: f32,
    under_attack_wait: f32,
    deflect_timeout: f32,
    recharge_text_id: Option<i32>,
}

/// Immutable shield object selected by a protected building's `ShieldType`.
#[derive(Debug, Clone)]
pub(crate) struct PlasmaSubshieldProfile {
    name: String,
    index: usize,
    prototype: ProtoObject,
}

/// Persistent `BubbleShield` behavior paired with an object's Follow join.
#[derive(Debug, Clone)]
pub struct BubbleShieldActionProfile {
    owner_proto_object_name: String,
    join_action_name: String,
    work_range: f32,
    merge_type: Option<String>,
}

/// Cached member data needed to create a scenario-layered bubble squad.
#[derive(Debug, Clone)]
pub(crate) struct BubbleShieldMemberProfile {
    proto_object_name: String,
    proto_object_index: usize,
    proto_object: ProtoObject,
    count: u32,
}

/// Immutable squad selected by the layered `ShieldBubbleTypes` table.
#[derive(Debug, Clone)]
pub struct BubbleShieldSquadProfile {
    proto_squad_name: String,
    proto_squad_id: i32,
    members: Vec<BubbleShieldMemberProfile>,
}

/// Layered default and per-proto-squad bubble-shield selections.
#[derive(Debug, Clone, Default)]
pub(super) struct ShieldBubbleTypes {
    default_squad: Option<BubbleShieldSquadProfile>,
    squad_overrides: BTreeMap<String, BubbleShieldSquadProfile>,
}

impl PlasmaShieldGeneratorProfile {
    /// Return the generator proto object that owns the persistent action.
    #[must_use]
    pub fn generator_proto_object_name(&self) -> &str {
        &self.generator_proto_object_name
    }

    /// Return the shield proto object created by the action.
    #[must_use]
    pub fn shield_proto_object_name(&self) -> &str {
        &self.shield_proto_object_name
    }

    /// Return the authored shield rebuild time in seconds.
    #[must_use]
    pub const fn rebuild_time(&self) -> f32 {
        self.rebuild_time
    }

    /// Return the authored post-attack wait in seconds.
    #[must_use]
    pub const fn under_attack_wait(&self) -> f32 {
        self.under_attack_wait
    }

    /// Return the authored deflection timeout in seconds.
    #[must_use]
    pub const fn deflect_timeout(&self) -> f32 {
        self.deflect_timeout
    }

    /// Return the localized recharge-description identifier, when authored.
    #[must_use]
    pub const fn recharge_text_id(&self) -> Option<i32> {
        self.recharge_text_id
    }

    pub(crate) const fn shield_proto_object_index(&self) -> usize {
        self.shield_proto_object_index
    }

    pub(crate) const fn shield_proto_object(&self) -> &ProtoObject {
        &self.shield_proto_object
    }
}

impl PlasmaSubshieldProfile {
    pub(crate) fn shield_proto_object_name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn shield_proto_object_index(&self) -> usize {
        self.index
    }

    pub(crate) const fn shield_proto_object(&self) -> &ProtoObject {
        &self.prototype
    }
}

impl BubbleShieldActionProfile {
    /// Return the object prototype that owns the persistent shield action.
    #[must_use]
    pub fn owner_proto_object_name(&self) -> &str {
        &self.owner_proto_object_name
    }

    /// Return the authored Follow action selected by the command target rule.
    #[must_use]
    pub fn join_action_name(&self) -> &str {
        &self.join_action_name
    }

    /// Return the horizontal range at which the Follow join connects.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Return the authored merge channel, such as `Air`.
    #[must_use]
    pub fn merge_type(&self) -> Option<&str> {
        self.merge_type.as_deref()
    }
}

impl BubbleShieldSquadProfile {
    /// Return the scenario-layered proto-squad name.
    #[must_use]
    pub fn proto_squad_name(&self) -> &str {
        &self.proto_squad_name
    }

    /// Return the proto-squad's wire/database identifier.
    #[must_use]
    pub const fn proto_squad_id(&self) -> i32 {
        self.proto_squad_id
    }

    /// Return the number of concrete member objects created for this squad.
    #[must_use]
    pub fn member_count(&self) -> u32 {
        self.members.iter().map(|member| member.count).sum()
    }

    pub(crate) fn members(&self) -> &[BubbleShieldMemberProfile] {
        &self.members
    }
}

impl BubbleShieldMemberProfile {
    pub(crate) fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }

    pub(crate) const fn proto_object_index(&self) -> usize {
        self.proto_object_index
    }

    pub(crate) const fn proto_object(&self) -> &ProtoObject {
        &self.proto_object
    }

    pub(crate) const fn count(&self) -> u32 {
        self.count
    }
}

impl ShieldBubbleTypes {
    pub(super) fn from_document(database: &Database, document: &Document) -> Self {
        let Some(types) = document.root().and_then(find_bubble_types) else {
            return Self::default();
        };
        let default_squad = bubble_squad_profile(database, &types.text_string());
        let squad_overrides = types
            .children
            .iter()
            .filter(|child| child.name.eq_ignore_ascii_case("ShieldBubble"))
            .filter_map(|child| {
                let target = attribute(child, "target")?;
                let target = known_squad_name(database, &target)?;
                let shield = bubble_squad_profile(database, &child.text_string())?;
                Some((target.to_ascii_lowercase(), shield))
            })
            .collect();
        Self {
            default_squad,
            squad_overrides,
        }
    }

    pub(super) fn resolve(&self, target_proto_squad: &str) -> Option<&BubbleShieldSquadProfile> {
        self.squad_overrides
            .get(&target_proto_squad.to_ascii_lowercase())
            .or(self.default_squad.as_ref())
    }

    pub(super) fn default_squad(&self) -> Option<&BubbleShieldSquadProfile> {
        self.default_squad.as_ref()
    }
}

impl GameplayCatalog {
    /// Resolve the persistent base-shield generator profile for an object.
    #[must_use]
    pub fn plasma_shield_generator(
        &self,
        proto_object_name: &str,
    ) -> Option<&PlasmaShieldGeneratorProfile> {
        self.plasma_shield_generators
            .get(&proto_object_name.to_ascii_lowercase())
    }

    pub(crate) fn plasma_subshield(
        &self,
        protected_proto_object_name: &str,
    ) -> Option<&PlasmaSubshieldProfile> {
        self.plasma_subshields
            .get(&protected_proto_object_name.to_ascii_lowercase())
    }

    /// Resolve an object's persistent Follow/BubbleShield action pair.
    #[must_use]
    pub fn bubble_shield_action(
        &self,
        proto_object_name: &str,
    ) -> Option<&BubbleShieldActionProfile> {
        self.bubble_shield_actions
            .get(&proto_object_name.to_ascii_lowercase())
    }

    /// Resolve the layered bubble-shield proto squad for a target proto squad.
    #[must_use]
    pub fn shield_bubble_squad(&self, target_proto_squad: &str) -> Option<&str> {
        self.bubble_shield_profile(target_proto_squad)
            .map(BubbleShieldSquadProfile::proto_squad_name)
    }

    /// Return the layered default bubble-shield proto squad.
    #[must_use]
    pub fn default_shield_bubble_squad(&self) -> Option<&str> {
        self.shield_bubble_types
            .default_squad()
            .map(BubbleShieldSquadProfile::proto_squad_name)
    }

    pub(crate) fn bubble_shield_profile(
        &self,
        target_proto_squad: &str,
    ) -> Option<&BubbleShieldSquadProfile> {
        self.shield_bubble_types.resolve(target_proto_squad)
    }

    #[cfg(test)]
    pub(crate) fn load_test_shield_bubble_document(
        &mut self,
        database: &Database,
        document: &Document,
    ) {
        self.shield_bubble_types = ShieldBubbleTypes::from_document(database, document);
    }
}

pub(super) fn collect_plasma_shield_generators(
    database: &Database,
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, PlasmaShieldGeneratorProfile> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            let action = persistent_plasma_action(gameplay)?;
            let reference = action.proto_object.as_ref()?;
            if reference.squad.is_some() {
                return None;
            }
            let shield_name = reference.name.trim();
            let (shield_proto_object_index, shield_proto_object) = database
                .objects
                .iter()
                .enumerate()
                .find(|(_, object)| object.name.eq_ignore_ascii_case(shield_name))?;
            Some((
                key.clone(),
                PlasmaShieldGeneratorProfile {
                    generator_proto_object_name: gameplay.proto_object_name.clone(),
                    shield_proto_object_name: shield_proto_object.name.clone(),
                    shield_proto_object_index,
                    shield_proto_object: shield_proto_object.clone(),
                    rebuild_time: nonnegative(shield_proto_object.build_points),
                    under_attack_wait: action
                        .duration
                        .as_ref()
                        .map_or(0.0, |duration| finite_nonnegative(duration.seconds)),
                    deflect_timeout: nonnegative(action.deflect_timeout),
                    recharge_text_id: action.count,
                },
            ))
        })
        .collect()
}

pub(super) fn collect_bubble_shield_actions(
    objects: &BTreeMap<String, ObjectGameplay>,
) -> BTreeMap<String, BubbleShieldActionProfile> {
    objects
        .iter()
        .filter_map(|(key, gameplay)| {
            persistent_action(gameplay, "BubbleShield")?;
            let join = command_follow_join(gameplay)?;
            Some((
                key.clone(),
                BubbleShieldActionProfile {
                    owner_proto_object_name: gameplay.proto_object_name.clone(),
                    join_action_name: join.name.clone(),
                    work_range: nonnegative(join.work_range),
                    merge_type: join.merge_type.clone(),
                },
            ))
        })
        .collect()
}

pub(super) fn collect_plasma_subshields(
    database: &Database,
) -> BTreeMap<String, PlasmaSubshieldProfile> {
    database
        .objects
        .iter()
        .filter(|object| !is_turret(object))
        .filter_map(|protected| {
            let shield_type = protected.shield_type.as_deref()?.trim();
            let (shield_proto_object_index, shield_proto_object) = database
                .objects
                .iter()
                .enumerate()
                .find(|(_, object)| object.name.eq_ignore_ascii_case(shield_type))?;
            Some((
                protected.name.to_ascii_lowercase(),
                PlasmaSubshieldProfile {
                    name: shield_proto_object.name.clone(),
                    index: shield_proto_object_index,
                    prototype: shield_proto_object.clone(),
                },
            ))
        })
        .collect()
}

fn persistent_plasma_action(
    gameplay: &ObjectGameplay,
) -> Option<&pipeline::database::hw1::tactics::Action> {
    persistent_action(gameplay, "PlasmaShieldGen")
}

fn persistent_action<'a>(
    gameplay: &'a ObjectGameplay,
    action_type: &str,
) -> Option<&'a pipeline::database::hw1::tactics::Action> {
    let persistent = &gameplay.tactics.tactic.as_ref()?.persistent_actions;
    persistent.iter().find_map(|name| {
        gameplay.tactics.actions.iter().find(|action| {
            action.name.eq_ignore_ascii_case(name)
                && action
                    .action_type
                    .as_deref()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case(action_type))
        })
    })
}

fn command_follow_join(
    gameplay: &ObjectGameplay,
) -> Option<&pipeline::database::hw1::tactics::Action> {
    let tactics = &gameplay.tactics;
    let command_action = tactics.tactic.as_ref().and_then(|rules| {
        rules.target_rules.iter().find_map(|rule| {
            rule.ability
                .as_deref()
                .is_some_and(|ability| ability.eq_ignore_ascii_case("Command"))
                .then_some(rule.action.as_deref())
                .flatten()
        })
    });
    tactics.actions.iter().find(|action| {
        command_action.is_none_or(|name| action.name.eq_ignore_ascii_case(name))
            && action
                .action_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("Join"))
            && action
                .join_type
                .as_ref()
                .is_some_and(|join| join.kind.eq_ignore_ascii_case("Follow"))
    })
}

fn find_bubble_types(root: &Node) -> Option<&Node> {
    root.children
        .iter()
        .find(|node| node.name.eq_ignore_ascii_case("ShieldBubbleTypes"))
}

fn known_squad_name(database: &Database, value: &str) -> Option<String> {
    let value = value.trim();
    database
        .squads
        .iter()
        .find(|squad| squad.name.eq_ignore_ascii_case(value))
        .map(|squad| squad.name.clone())
}

fn bubble_squad_profile(database: &Database, value: &str) -> Option<BubbleShieldSquadProfile> {
    let value = value.trim();
    let (squad_index, squad) = database
        .squads
        .iter()
        .enumerate()
        .find(|(_, squad)| squad.name.eq_ignore_ascii_case(value))?;
    let members = bubble_members(database, squad);
    (!members.is_empty()).then(|| BubbleShieldSquadProfile {
        proto_squad_name: squad.name.clone(),
        proto_squad_id: database_id(squad.dbid, squad_index),
        members,
    })
}

fn bubble_members(database: &Database, squad: &ProtoSquad) -> Vec<BubbleShieldMemberProfile> {
    squad
        .units
        .iter()
        .flat_map(|units| &units.entries)
        .filter_map(|entry| {
            let name = entry.proto_object.trim();
            let (proto_object_index, proto_object) = database
                .objects
                .iter()
                .enumerate()
                .find(|(_, object)| object.name.eq_ignore_ascii_case(name))?;
            let count = u32::try_from(entry.count.max(0)).ok()?;
            (count > 0).then(|| BubbleShieldMemberProfile {
                proto_object_name: proto_object.name.clone(),
                proto_object_index,
                proto_object: proto_object.clone(),
                count,
            })
        })
        .collect()
}

fn database_id(explicit: Option<i32>, index: usize) -> i32 {
    explicit.unwrap_or_else(|| i32::try_from(index).unwrap_or(-1))
}

fn attribute(node: &Node, name: &str) -> Option<String> {
    node.attributes
        .iter()
        .find(|attribute| attribute.name.eq_ignore_ascii_case(name))
        .map(pipeline::xmb::Attribute::value_string)
}

fn nonnegative(value: Option<f32>) -> f32 {
    value.map_or(0.0, finite_nonnegative)
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

fn is_turret(object: &ProtoObject) -> bool {
    object.object_types.iter().any(|object_type| {
        object_type.eq_ignore_ascii_case("TurretSocket")
            || object_type.eq_ignore_ascii_case("TurretBuilding")
    })
}

#[cfg(test)]
mod tests;
