//! Scenario-layered gameplay definitions used by the deterministic simulation.
//!
//! The typed database contains references to per-object tactic files, but the
//! tactic files themselves live beside the database tables in the ERA stack.
//! This module resolves those files after the scenario archive has been added,
//! preserving the same last-loaded-wins behavior as the retail asset manager.

use crate::entities::{ShieldCoverage, SquadMode};
use crate::player::PlayerTechState;
use glam::Vec3;
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::{Database, ProtoObject};
use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::BTreeMap;

mod abilities;
mod analysis;
mod damage_types;
mod join;
pub(crate) mod projectiles;
mod protection;
mod revival;
mod scripted_animations;
mod selection;
mod timing;

pub(crate) use abilities::resolve_database_ability;
pub use abilities::{AbilityGameplay, AbilityRecoveryStart};
pub use join::{JoinActionProfile, JoinKind, JoinMergeType, MergedSquadProfile};
pub use projectiles::{
    ProjectileInitialPerturbance, ProjectilePerturbanceProfile, ProjectileProfile,
};
pub(crate) use protection::PlasmaSubshieldProfile;
pub use protection::{
    BubbleShieldActionProfile, BubbleShieldSquadProfile, PlasmaShieldGeneratorProfile,
};
pub use revival::{HeroRevivalProfile, ReviveActionProfile, UnitRevivalProfile};
pub(crate) use selection::ProjectileCollisionTraits;
pub use selection::{AttackQuery, AttackQueryFlags, TacticRelation};
pub use timing::{AreaDamageProfile, AttackAccuracyProfile, AttackAnimation, AttackProfile};

/// Gameplay definitions for every proto object with a resolvable tactic file.
#[derive(Debug, Clone, Default)]
pub struct GameplayCatalog {
    objects: BTreeMap<String, ObjectGameplay>,
    issues: Vec<GameplayLoadIssue>,
    timing_issues: Vec<GameplayTimingIssue>,
    weapon_damage_modifiers: BTreeMap<String, BTreeMap<String, WeaponDamageModifier>>,
    damage_type_profiles: damage_types::DamageTypeProfiles,
    damage_types: BTreeMap<String, String>,
    damage_type_exemplars: BTreeMap<String, String>,
    ability_names: Vec<String>,
    abilities: Vec<AbilityGameplay>,
    command_ability_id: Option<u8>,
    object_ability_commands: BTreeMap<String, u8>,
    target_traits: BTreeMap<String, selection::ObjectTargetTraits>,
    projectile_profiles: BTreeMap<String, ProjectileProfile>,
    scripted_animation_clips:
        BTreeMap<(String, String), scripted_animations::ScriptedAnimationClip>,
    projectile_gravity: f32,
    track_intercept_distance: f32,
    height_bonus_damage: f32,
    shield_regen_delay: f32,
    shield_regen_time: f32,
    hero_revival: HeroRevivalProfile,
    plasma_shield_generators: BTreeMap<String, PlasmaShieldGeneratorProfile>,
    plasma_subshields: BTreeMap<String, protection::PlasmaSubshieldProfile>,
    bubble_shield_actions: BTreeMap<String, BubbleShieldActionProfile>,
    shield_bubble_types: protection::ShieldBubbleTypes,
    merged_squads: join::MergedSquadProfiles,
    join_database: join::JoinDatabaseProfiles,
    referenced_tactic_count: usize,
}

/// One proto object's immutable combat definition.
#[derive(Debug, Clone)]
pub struct ObjectGameplay {
    proto_object_name: String,
    damage_type: Option<String>,
    tactics_path: String,
    tactics: TacticData,
    attack_profiles: BTreeMap<String, AttackProfile>,
    hero_death: bool,
}

/// A tactic action joined to the weapon it references.
#[derive(Debug, Clone, Copy)]
pub struct RangedAction<'a> {
    /// The authored action definition.
    pub action: &'a Action,
    /// The authored weapon definition referenced by the action.
    pub weapon: &'a Weapon,
}

#[derive(Debug, Clone, Copy)]
struct WeaponDamageModifier {
    damage: f32,
    rating: f32,
}

/// A tactic file that could not participate in the gameplay catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameplayLoadIssue {
    proto_object_name: String,
    tactics_path: String,
    reason: String,
}

/// An authored ranged action whose visual attack timing could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameplayTimingIssue {
    proto_object_name: String,
    action_name: String,
    reason: String,
}

impl GameplayCatalog {
    /// Resolve every tactic referenced by the active layered database.
    ///
    /// The caller must add the scenario ERA before invoking this method. A
    /// scenario-local tactic then wins over an identically named base tactic.
    #[must_use]
    pub fn load_from_source(
        database: &Database,
        source: &mut AssetSource<StdFileProvider>,
    ) -> Self {
        let referenced_tactic_count = database
            .objects
            .iter()
            .filter(|object| object.tactics.is_some())
            .count();
        let damage_type_profiles = load_damage_type_profiles(database, source);
        let damage_types = collect_damage_types(&damage_type_profiles);
        let damage_type_exemplars =
            analysis::collect_damage_type_exemplars(database, &damage_type_profiles);
        let target_traits = selection::collect_target_traits(database, &damage_type_profiles);
        let (shield_bubble_types, merged_squads) = load_squad_gameplay(database, source);
        let mut catalog = Self {
            referenced_tactic_count,
            weapon_damage_modifiers: collect_weapon_damage_modifiers(database),
            damage_type_profiles,
            damage_types,
            damage_type_exemplars,
            ability_names: selection::collect_ability_names(database),
            abilities: abilities::collect_abilities(database),
            command_ability_id: abilities::command_ability_id(database),
            object_ability_commands: abilities::collect_object_ability_commands(database),
            target_traits,
            projectile_profiles: projectiles::collect_projectile_profiles(database),
            projectile_gravity: database
                .game_data
                .as_ref()
                .and_then(|data| data.projectile_gravity)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or_default(),
            track_intercept_distance: game_data_nonnegative(database, |data| {
                data.track_intercept_distance
            }),
            height_bonus_damage: database
                .game_data
                .as_ref()
                .and_then(|data| data.height_bonus_damage)
                .filter(|value| value.is_finite())
                .unwrap_or_default(),
            shield_regen_delay: game_data_nonnegative(database, |data| data.shield_regen_delay),
            shield_regen_time: game_data_nonnegative(database, |data| data.shield_regen_time),
            hero_revival: revival::hero_profile(database),
            plasma_subshields: protection::collect_plasma_subshields(database),
            shield_bubble_types,
            merged_squads,
            join_database: join::JoinDatabaseProfiles::from_database(database),
            ..Self::default()
        };
        let mut cache = BTreeMap::<String, Result<TacticData, String>>::new();
        let mut timing_cache = timing::TimingAssetCache::default();
        for object in &database.objects {
            let Some(tactics_ref) = object.tactics.as_deref() else {
                continue;
            };
            let tactics_path = canonical_tactics_path(tactics_ref);
            let cache_key = tactics_path.to_ascii_lowercase();
            if !cache.contains_key(&cache_key) {
                cache.insert(cache_key.clone(), load_tactics(source, &tactics_path));
            }
            let Some(parsed_tactics) = cache.get(&cache_key).cloned() else {
                continue;
            };
            match parsed_tactics {
                Ok(tactics) => {
                    let timing =
                        timing::load_attack_profiles(object, &tactics, source, &mut timing_cache);
                    catalog.timing_issues.extend(timing.issues.into_iter().map(
                        |(action_name, reason)| GameplayTimingIssue {
                            proto_object_name: object.name.clone(),
                            action_name,
                            reason,
                        },
                    ));
                    catalog.insert_object(
                        object,
                        tactics_path,
                        tactics,
                        timing.profiles,
                        revival::is_hero_death_object(database, object),
                    );
                }
                Err(reason) => catalog.issues.push(GameplayLoadIssue {
                    proto_object_name: object.name.clone(),
                    tactics_path,
                    reason: reason.clone(),
                }),
            }
        }
        catalog.plasma_shield_generators =
            protection::collect_plasma_shield_generators(database, &catalog.objects);
        catalog.bubble_shield_actions = protection::collect_bubble_shield_actions(&catalog.objects);

        catalog
    }

    /// Build a catalog from already parsed per-object tactics.
    ///
    /// This is useful for deterministic simulation tests that do not require an
    /// installed copy of the game.
    #[must_use]
    pub fn from_tactics(
        database: &Database,
        tactics: impl IntoIterator<Item = (String, TacticData)>,
    ) -> Self {
        let parsed = tactics
            .into_iter()
            .map(|(name, tactic)| (name.to_ascii_lowercase(), tactic))
            .collect::<BTreeMap<_, _>>();
        let referenced_tactic_count = database
            .objects
            .iter()
            .filter(|object| object.tactics.is_some())
            .count();
        let damage_type_profiles = damage_types::DamageTypeProfiles::from_database(database);
        let damage_types = collect_damage_types(&damage_type_profiles);
        let damage_type_exemplars =
            analysis::collect_damage_type_exemplars(database, &damage_type_profiles);
        let target_traits = selection::collect_target_traits(database, &damage_type_profiles);
        let mut catalog = Self {
            referenced_tactic_count,
            weapon_damage_modifiers: collect_weapon_damage_modifiers(database),
            damage_type_profiles,
            damage_types,
            damage_type_exemplars,
            ability_names: selection::collect_ability_names(database),
            abilities: abilities::collect_abilities(database),
            command_ability_id: abilities::command_ability_id(database),
            object_ability_commands: abilities::collect_object_ability_commands(database),
            target_traits,
            projectile_profiles: projectiles::collect_projectile_profiles(database),
            projectile_gravity: database
                .game_data
                .as_ref()
                .and_then(|data| data.projectile_gravity)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or_default(),
            track_intercept_distance: game_data_nonnegative(database, |data| {
                data.track_intercept_distance
            }),
            height_bonus_damage: database
                .game_data
                .as_ref()
                .and_then(|data| data.height_bonus_damage)
                .filter(|value| value.is_finite())
                .unwrap_or_default(),
            shield_regen_delay: game_data_nonnegative(database, |data| data.shield_regen_delay),
            shield_regen_time: game_data_nonnegative(database, |data| data.shield_regen_time),
            hero_revival: revival::hero_profile(database),
            plasma_subshields: protection::collect_plasma_subshields(database),
            join_database: join::JoinDatabaseProfiles::from_database(database),
            ..Self::default()
        };
        for object in &database.objects {
            let Some(tactic) = parsed.get(&object.name.to_ascii_lowercase()) else {
                continue;
            };
            let tactics_path = object
                .tactics
                .as_deref()
                .map_or_else(String::new, canonical_tactics_path);
            catalog.insert_object(
                object,
                tactics_path,
                tactic.clone(),
                BTreeMap::new(),
                revival::is_hero_death_object(database, object),
            );
        }
        catalog.plasma_shield_generators =
            protection::collect_plasma_shield_generators(database, &catalog.objects);
        catalog.bubble_shield_actions = protection::collect_bubble_shield_actions(&catalog.objects);
        catalog
    }

    #[cfg(test)]
    pub(crate) fn from_test_profiles(
        database: &Database,
        tactics: impl IntoIterator<Item = (String, TacticData)>,
        profiles: impl IntoIterator<Item = (String, AttackProfile)>,
    ) -> Self {
        let mut catalog = Self::from_tactics(database, tactics);
        for (proto_object_name, profile) in profiles {
            if let Some(object) = catalog
                .objects
                .get_mut(&proto_object_name.to_ascii_lowercase())
            {
                object
                    .attack_profiles
                    .insert(profile.action_name.to_ascii_lowercase(), profile);
            }
        }
        catalog
    }

    #[cfg(test)]
    pub(crate) fn load_test_damage_type_document(
        &mut self,
        database: &Database,
        document: &pipeline::xmb::Document,
    ) {
        self.damage_type_profiles =
            damage_types::DamageTypeProfiles::from_document(database, document);
        self.damage_types = collect_damage_types(&self.damage_type_profiles);
        self.damage_type_exemplars =
            analysis::collect_damage_type_exemplars(database, &self.damage_type_profiles);
        self.target_traits = selection::collect_target_traits(database, &self.damage_type_profiles);
        for object in self.objects.values_mut() {
            object.damage_type = self
                .damage_type_profiles
                .base_damage_type(&object.proto_object_name)
                .map(str::to_owned);
        }
    }

    fn insert_object(
        &mut self,
        object: &ProtoObject,
        tactics_path: String,
        tactics: TacticData,
        attack_profiles: BTreeMap<String, AttackProfile>,
        hero_death: bool,
    ) {
        let damage_type = self
            .damage_type_profiles
            .base_damage_type(&object.name)
            .map(str::to_owned)
            .or_else(|| object.damage_type.clone());
        self.objects.insert(
            object.name.to_ascii_lowercase(),
            ObjectGameplay {
                proto_object_name: object.name.clone(),
                damage_type,
                tactics_path,
                tactics,
                attack_profiles,
                hero_death,
            },
        );
    }

    /// Look up gameplay data using the case-insensitive database name contract.
    #[must_use]
    pub fn object(&self, proto_object_name: &str) -> Option<&ObjectGameplay> {
        self.objects.get(&proto_object_name.to_ascii_lowercase())
    }

    /// Resolve the work range of the object's enabled teleporter hot-drop action.
    #[must_use]
    pub fn teleporter_work_range(&self, proto_object_name: &str) -> Option<f32> {
        self.object(proto_object_name)?
            .tactics
            .actions
            .iter()
            .find(|action| {
                action
                    .action_type
                    .as_deref()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("HotDrop"))
                    && action.use_teleporter.unwrap_or(false)
            })
            .and_then(|action| action.work_range)
            .filter(|range| range.is_finite() && *range >= 0.0)
    }

    /// Iterate in normalized proto-object name order.
    pub fn objects(&self) -> impl Iterator<Item = &ObjectGameplay> {
        self.objects.values()
    }

    /// Return the number of proto objects with loaded tactic data.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Return whether no tactic data was loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Return the number of database objects that referenced a tactic file.
    #[must_use]
    pub const fn referenced_tactic_count(&self) -> usize {
        self.referenced_tactic_count
    }

    /// Return deterministic load diagnostics in database object order.
    #[must_use]
    pub fn issues(&self) -> &[GameplayLoadIssue] {
        &self.issues
    }

    /// Return actions that could not obtain retail visual/UAX attack timing.
    #[must_use]
    pub fn timing_issues(&self) -> &[GameplayTimingIssue] {
        &self.timing_issues
    }

    /// Resolve the baseline Normal-mode ranged action in retail rule order.
    ///
    /// Authored target rules are checked in order, matching `BTactic`. If no
    /// applicable Normal-mode rule exists, a ranged action marked `Default` is
    /// used. As a final safe fallback, a tactic with exactly one enabled ranged
    /// action is unambiguous. Multiple actions are deliberately not ranked by
    /// DPS because tech effects and other squad modes enable Marine grenades,
    /// rockets, and cover actions dynamically.
    #[must_use]
    pub fn initial_ranged_action(&self, proto_object_name: &str) -> Option<RangedAction<'_>> {
        self.initial_ranged_action_for(proto_object_name)
    }

    /// Resolve both the initial ranged action and its authored attack timing.
    #[must_use]
    pub fn initial_attack_profile(&self, proto_object_name: &str) -> Option<&AttackProfile> {
        let object = self.object(proto_object_name)?;
        let action = self.initial_ranged_action_for(proto_object_name)?;
        object.attack_profile(&action.action.name)
    }

    /// Resolve the base weapon-type modifier against a target proto object.
    ///
    /// Player tech effects can later layer on this immutable database value.
    #[must_use]
    pub fn weapon_damage_modifier(&self, weapon_type: Option<&str>, target_proto: &str) -> f32 {
        let Some(damage_type) = self.damage_types.get(&target_proto.to_ascii_lowercase()) else {
            return 1.0;
        };
        self.weapon_modifier_for_damage_type(weapon_type, damage_type, None)
    }

    pub(crate) fn directional_weapon_damage_modifier(
        &self,
        weapon_type: Option<&str>,
        target_proto: &str,
        direction: Vec3,
        forward: Vec3,
        mode: SquadMode,
        technologies: Option<&PlayerTechState>,
    ) -> f32 {
        let Some(damage_type) =
            self.damage_type_profiles
                .damage_type(target_proto, direction, forward, mode)
        else {
            return 1.0;
        };
        self.weapon_modifier_for_damage_type(weapon_type, damage_type, technologies)
    }

    fn weapon_modifier_for_damage_type(
        &self,
        weapon_type: Option<&str>,
        damage_type: &str,
        technologies: Option<&PlayerTechState>,
    ) -> f32 {
        let Some(weapon_type) = weapon_type else {
            return 1.0;
        };
        let Some(modifier) = self
            .weapon_damage_modifiers
            .get(&weapon_type.to_ascii_lowercase())
            .and_then(|modifiers| modifiers.get(&damage_type.to_ascii_lowercase()))
        else {
            return 1.0;
        };
        let effective = technologies.map_or(modifier.damage, |technologies| {
            technologies.weapon_type_damage_modifier(weapon_type, damage_type, modifier.damage)
        });
        if effective.is_finite() {
            effective.max(0.0)
        } else {
            1.0
        }
    }

    /// Return the authored shield-facing rule for one proto object.
    #[must_use]
    pub fn shield_coverage(&self, proto_object_name: &str) -> ShieldCoverage {
        self.damage_type_profiles.shield_coverage(proto_object_name)
    }

    /// Resolve the armor category selected by an incoming impact vector.
    #[must_use]
    pub fn directional_damage_type(
        &self,
        proto_object_name: &str,
        direction: Vec3,
        forward: Vec3,
        mode: SquadMode,
    ) -> Option<&str> {
        self.damage_type_profiles
            .damage_type(proto_object_name, direction, forward, mode)
    }

    pub(crate) fn shield_coverages(&self) -> impl Iterator<Item = (&str, ShieldCoverage)> + '_ {
        self.damage_type_profiles.shield_coverages()
    }

    /// Look up immutable projectile movement data by proto-object name.
    #[must_use]
    pub fn projectile(&self, proto_object_name: &str) -> Option<&ProjectileProfile> {
        self.projectile_profiles
            .get(&proto_object_name.to_ascii_lowercase())
    }

    /// Resolve a command's generic ability ID to the source object's ability.
    ///
    /// Retail maps the database `Command` ability through the proto object's
    /// `AbilityCommand`; concrete ability IDs pass through unchanged.
    #[must_use]
    pub fn resolve_order_ability(
        &self,
        proto_object_name: &str,
        requested_id: u8,
    ) -> Option<&AbilityGameplay> {
        let actual_id = if self.command_ability_id == Some(requested_id) {
            *self
                .object_ability_commands
                .get(&proto_object_name.to_ascii_lowercase())?
        } else {
            requested_id
        };
        self.abilities.get(usize::from(actual_id))
    }

    /// Database index of the generic retail `Command` ability.
    ///
    /// Trigger Work V4 writes this ID when `DoAbility` is enabled so tactic
    /// rules can choose the object's concrete command action.
    #[must_use]
    pub const fn command_ability_id(&self) -> Option<u8> {
        self.command_ability_id
    }

    /// Return global projectile gravity from the layered game-data table.
    #[must_use]
    pub const fn projectile_gravity(&self) -> f32 {
        self.projectile_gravity
    }

    /// Distance inside which tracking projectiles lead moving targets.
    #[must_use]
    pub const fn track_intercept_distance(&self) -> f32 {
        self.track_intercept_distance
    }

    /// Return the global height-bonus damage factor from layered game data.
    #[must_use]
    pub const fn height_bonus_damage(&self) -> f32 {
        self.height_bonus_damage
    }

    /// Base delay after damage before a squad may start shield recharge.
    #[must_use]
    pub const fn shield_regen_delay(&self) -> f32 {
        self.shield_regen_delay
    }

    /// Fixed lifetime of one retail unit shield-recharge action.
    #[must_use]
    pub const fn shield_regen_time(&self) -> f32 {
        self.shield_regen_time
    }

    /// Base player recharge rate as a fraction of maximum shields per second.
    #[must_use]
    pub fn shield_regen_rate(&self) -> f32 {
        if self.shield_regen_time > 0.0 {
            self.shield_regen_time.recip()
        } else {
            0.0
        }
    }

    /// Resolve the persistent revival behavior for one unit prototype.
    #[must_use]
    pub fn unit_revival_profile(&self, proto_object_name: &str) -> Option<UnitRevivalProfile> {
        let object = self.object(proto_object_name)?;
        if object.hero_death {
            return Some(UnitRevivalProfile::Hero(self.hero_revival));
        }
        revival::revive_action_profile(&object.tactics.actions).map(UnitRevivalProfile::Revive)
    }
}

fn load_squad_gameplay(
    database: &Database,
    source: &mut AssetSource<StdFileProvider>,
) -> (protection::ShieldBubbleTypes, join::MergedSquadProfiles) {
    let Some(document) = source.read_xmb("data\\squads.xml") else {
        return Default::default();
    };
    (
        protection::ShieldBubbleTypes::from_document(database, &document),
        join::MergedSquadProfiles::from_document(database, &document),
    )
}

impl ObjectGameplay {
    /// Return the database proto-object name with its original spelling.
    #[must_use]
    pub fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }

    /// Return the object's authored damage type.
    #[must_use]
    pub fn damage_type(&self) -> Option<&str> {
        self.damage_type.as_deref()
    }

    /// Return the canonical layered asset path used for this tactic.
    #[must_use]
    pub fn tactics_path(&self) -> &str {
        &self.tactics_path
    }

    /// Return the complete parsed tactic definition.
    #[must_use]
    pub const fn tactics(&self) -> &TacticData {
        &self.tactics
    }

    /// Look up a computed attack profile by authored action name.
    #[must_use]
    pub fn attack_profile(&self, action_name: &str) -> Option<&AttackProfile> {
        self.attack_profiles.get(&action_name.to_ascii_lowercase())
    }

    /// Iterate over attack profiles in normalized action-name order.
    pub fn attack_profiles(&self) -> impl Iterator<Item = &AttackProfile> {
        self.attack_profiles.values()
    }

    /// Iterate over all valid ranged action/weapon pairs in authored order.
    pub fn ranged_actions(&self) -> impl Iterator<Item = RangedAction<'_>> {
        self.tactics
            .actions
            .iter()
            .filter_map(|action| is_ranged_attack(action).then(|| self.resolve_action(action))?)
    }

    pub(super) fn resolve_action<'a>(&'a self, action: &'a Action) -> Option<RangedAction<'a>> {
        let weapon_name = action.weapon.as_deref()?;
        let weapon = self
            .tactics
            .weapons
            .iter()
            .find(|weapon| weapon.name.eq_ignore_ascii_case(weapon_name))?;
        Some(RangedAction { action, weapon })
    }
}

impl GameplayLoadIssue {
    /// Return the proto object whose tactic failed to load.
    #[must_use]
    pub fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }

    /// Return the canonical attempted tactic path.
    #[must_use]
    pub fn tactics_path(&self) -> &str {
        &self.tactics_path
    }

    /// Return the parse or resolution failure.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl GameplayTimingIssue {
    /// Return the proto object whose action timing failed to resolve.
    #[must_use]
    pub fn proto_object_name(&self) -> &str {
        &self.proto_object_name
    }

    /// Return the authored ranged action name.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Return the visual/UAX resolution failure.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

fn collect_weapon_damage_modifiers(
    database: &Database,
) -> BTreeMap<String, BTreeMap<String, WeaponDamageModifier>> {
    database
        .weapon_types
        .iter()
        .map(|weapon_type| {
            let modifiers = weapon_type
                .damage_modifiers
                .iter()
                .map(|modifier| {
                    (
                        modifier.damage_type.to_ascii_lowercase(),
                        WeaponDamageModifier {
                            damage: modifier.modifier,
                            rating: modifier.rating.unwrap_or(1.0),
                        },
                    )
                })
                .collect();
            (weapon_type.name.to_ascii_lowercase(), modifiers)
        })
        .collect()
}

fn load_damage_type_profiles(
    database: &Database,
    source: &mut AssetSource<StdFileProvider>,
) -> damage_types::DamageTypeProfiles {
    source.read_xmb("data\\objects.xml").map_or_else(
        || damage_types::DamageTypeProfiles::from_database(database),
        |document| damage_types::DamageTypeProfiles::from_document(database, &document),
    )
}

fn collect_damage_types(profiles: &damage_types::DamageTypeProfiles) -> BTreeMap<String, String> {
    profiles
        .base_damage_types()
        .map(|(object, damage_type)| (object.to_owned(), damage_type.to_ascii_lowercase()))
        .collect()
}

fn game_data_nonnegative(
    database: &Database,
    field: impl FnOnce(&pipeline::database::hw1::GameData) -> Option<f32>,
) -> f32 {
    database
        .game_data
        .as_ref()
        .and_then(field)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn canonical_tactics_path(tactics_ref: &str) -> String {
    let tactics_ref = tactics_ref.trim().replace('/', "\\");
    let tactics_ref = tactics_ref.trim_start_matches('\\');
    if tactics_ref
        .to_ascii_lowercase()
        .starts_with("data\\tactics\\")
    {
        tactics_ref.to_owned()
    } else {
        format!("data\\tactics\\{tactics_ref}")
    }
}

fn load_tactics(
    source: &mut AssetSource<StdFileProvider>,
    path: &str,
) -> Result<TacticData, String> {
    let document = source
        .read_xmb(path)
        .ok_or_else(|| "asset was not found or was not a valid XMB document".to_owned())?;
    pipeline::database::hw1::tactics::parse(&document)
        .map_err(|error| format!("failed to parse tactic data: {error}"))
}

fn is_ranged_attack(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("RangedAttack"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::tactics::{TacticRules, TargetRule};

    fn database() -> Database {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "test_unit".to_owned(),
            tactics: Some("test_unit.tactics".to_owned()),
            damage_type: Some("Light".to_owned()),
            ..ProtoObject::default()
        });
        database
    }

    fn action(name: &str, weapon: &str) -> Action {
        Action {
            name: name.to_owned(),
            action_type: Some("RangedAttack".to_owned()),
            weapon: Some(weapon.to_owned()),
            ..Action::default()
        }
    }

    fn weapon(name: &str, range: f32) -> Weapon {
        Weapon {
            name: name.to_owned(),
            damage_per_second: Some(10.0),
            max_range: Some(range),
            ..Weapon::default()
        }
    }

    #[test]
    fn catalog_lookup_is_case_insensitive_and_preserves_raw_tactics() {
        let tactics = TacticData {
            weapons: vec![weapon("Rifle", 25.0)],
            actions: vec![action("RifleAttack", "Rifle")],
            ..TacticData::default()
        };
        let catalog =
            GameplayCatalog::from_tactics(&database(), [("TEST_UNIT".to_owned(), tactics)]);

        let object = catalog.object("Test_Unit").expect("loaded gameplay");
        assert_eq!(object.proto_object_name(), "test_unit");
        assert_eq!(object.damage_type(), Some("Light"));
        assert_eq!(object.tactics().actions.len(), 1);
        assert_eq!(
            catalog
                .initial_ranged_action("TEST_UNIT")
                .and_then(|resolved| resolved.weapon.max_range),
            Some(25.0)
        );
    }

    #[test]
    fn authored_enemy_rule_wins_in_retail_order() {
        let tactics = TacticData {
            weapons: vec![weapon("First", 10.0), weapon("Second", 20.0)],
            actions: vec![
                action("FirstAttack", "First"),
                action("SecondAttack", "Second"),
            ],
            tactic: Some(TacticRules {
                target_rules: vec![TargetRule {
                    relation: Some("Enemy".to_owned()),
                    action: Some("SecondAttack".to_owned()),
                    ..TargetRule::default()
                }],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let catalog =
            GameplayCatalog::from_tactics(&database(), [("test_unit".to_owned(), tactics)]);

        let selected = catalog
            .initial_ranged_action("test_unit")
            .expect("rule-selected action");
        assert_eq!(selected.action.name, "SecondAttack");
        assert_eq!(selected.weapon.max_range, Some(20.0));
    }

    #[test]
    fn ambiguous_enabled_attacks_are_not_guessed() {
        let tactics = TacticData {
            weapons: vec![weapon("Rifle", 25.0), weapon("Rocket", 50.0)],
            actions: vec![
                action("RifleAttack", "Rifle"),
                action("RocketAttack", "Rocket"),
            ],
            ..TacticData::default()
        };
        let catalog =
            GameplayCatalog::from_tactics(&database(), [("test_unit".to_owned(), tactics)]);

        assert!(catalog.initial_ranged_action("test_unit").is_none());
        assert_eq!(
            catalog
                .object("test_unit")
                .unwrap()
                .ranged_actions()
                .count(),
            2
        );
    }

    #[test]
    fn baseline_selection_uses_normal_mode_and_ignores_cover_and_abilities() {
        let tactics = TacticData {
            weapons: vec![
                weapon("CoverRifle", 60.0),
                weapon("Grenade", 35.0),
                weapon("Rifle", 25.0),
            ],
            actions: vec![
                action("CoverAttack", "CoverRifle"),
                action("GrenadeAttack", "Grenade"),
                action("RifleAttack", "Rifle"),
            ],
            tactic: Some(TacticRules {
                target_rules: vec![
                    TargetRule {
                        relation: Some("Enemy".to_owned()),
                        squad_mode: Some("Cover".to_owned()),
                        action: Some("CoverAttack".to_owned()),
                        ..TargetRule::default()
                    },
                    TargetRule {
                        squad_mode: Some("Normal".to_owned()),
                        ability: Some("Command".to_owned()),
                        action: Some("GrenadeAttack".to_owned()),
                        ..TargetRule::default()
                    },
                    TargetRule {
                        relation: Some("Enemy".to_owned()),
                        squad_mode: Some("Normal".to_owned()),
                        action: Some("RifleAttack".to_owned()),
                        ..TargetRule::default()
                    },
                ],
                ..TacticRules::default()
            }),
            ..TacticData::default()
        };
        let catalog =
            GameplayCatalog::from_tactics(&database(), [("test_unit".to_owned(), tactics)]);

        assert_eq!(
            catalog
                .initial_ranged_action("test_unit")
                .map(|action| action.action.name.as_str()),
            Some("RifleAttack")
        );
    }
}
