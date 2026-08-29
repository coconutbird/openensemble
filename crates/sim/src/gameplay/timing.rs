//! Retail attack timing derived from visual animation tags and UAX durations.

use super::{RangedAction, unit_attacks::uses_ranged_attack_executor};
use num_traits::ToPrimitive;
use pipeline::database::hw1::tactics::{Action, TacticData, Weapon};
use pipeline::database::hw1::visual::{Anim, Model, Visual};
use pipeline::database::hw1::{ProtoObject, visual};
use pipeline::source::{AssetSource, StdFileProvider};
use std::collections::BTreeMap;

/// One weighted animation variant used by the shared unit attack executor.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackAnimation {
    /// Canonical UAX path in the layered asset source.
    pub asset_path: String,
    /// Authored random-selection weight; visual assets default to one.
    pub weight: i32,
    /// UAX duration in seconds.
    pub duration: f32,
    /// Normalized Attack-tag positions in authored order.
    pub attack_positions: Vec<f32>,
}

/// Immutable area-damage values authored on one weapon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AreaDamageProfile {
    /// Maximum distance from ground zero to a target's simulation bounds.
    pub radius: f32,
    /// Fraction of base damage dealt directly to the primary target.
    pub primary_target_factor: f32,
    /// Fraction of the radius occupied by the inner falloff interval.
    pub distance_factor: f32,
    /// Damage fraction at the inner falloff interval's outer edge.
    pub damage_factor: f32,
    /// Whether the shared damage pool is consumed nearest-target-first.
    pub linear_damage: bool,
    /// Whether the area query ignores vertical separation.
    pub ignores_y_axis: bool,
    /// Whether splash may damage the attacker's player and allies.
    pub friendly_fire: bool,
}

/// Authored projectile hit chance and miss-distribution values for one weapon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackAccuracyProfile {
    /// Chance to fire without deviation while stationary.
    pub accuracy: f32,
    /// Chance to fire without deviation while moving at full speed.
    pub moving_accuracy: f32,
    /// Maximum stationary deviation at maximum range.
    pub max_deviation: f32,
    /// Maximum full-speed moving deviation at maximum range.
    pub moving_max_deviation: f32,
    /// Roll position separating the two miss-distribution intervals.
    pub distance_factor: f32,
    /// Deviation fraction at the interval boundary.
    pub deviation_factor: f32,
}

impl Default for AttackAccuracyProfile {
    fn default() -> Self {
        Self {
            accuracy: 1.0,
            moving_accuracy: 1.0,
            max_deviation: 0.0,
            moving_max_deviation: 0.0,
            distance_factor: 0.5,
            deviation_factor: 0.5,
        }
    }
}

/// Retail ammunition use and depleted-action result for one unit attack.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AttackAmmunition {
    /// The weapon does not consume unit ammunition.
    #[default]
    None,
    /// The action reports failure when a complete next volley cannot be paid.
    FailWhenDepleted,
    /// The action reports completion when a complete next volley cannot be paid.
    CompleteWhenDepleted,
}

impl AttackAmmunition {
    /// Return whether each Attack tag consumes authoritative unit ammunition.
    #[must_use]
    pub const fn is_used(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Return whether depletion completes rather than fails the attack action.
    #[must_use]
    pub const fn completes_when_depleted(self) -> bool {
        matches!(self, Self::CompleteWhenDepleted)
    }
}

/// Immutable attack values computed using retail's `computeAttackInfo` rules.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackProfile {
    /// Tactic action name.
    pub action_name: String,
    /// Tactic weapon name.
    pub weapon_name: String,
    /// Weapon type used for target damage modifiers.
    pub weapon_type: Option<String>,
    /// Projectile proto-object name, or `None` for an instant/melee hit.
    pub projectile: Option<String>,
    /// Authored area-damage contract, present only for a positive radius.
    pub area_damage: Option<AreaDamageProfile>,
    /// Whether direct projectile collision may hit the attacker or allied units.
    pub friendly_fire: bool,
    /// Whether the projectile aims at the target's ground point instead of its body.
    ///
    /// Retail also uses this to disable collisions with prototypes carrying the
    /// `TargetsFootOfUnit` flag for this particular launch.
    pub targets_foot_of_unit: bool,
    /// Maximum authored weapon range.
    pub max_range: f32,
    /// Maximum target velocity considered by launch-time projectile leading.
    pub max_velocity_lead: f32,
    /// Stationary and moving projectile accuracy/deviation contract.
    pub accuracy: AttackAccuracyProfile,
    /// Base damage applied for each Attack tag before live modifiers.
    pub damage_per_attack: f32,
    /// Ammunition consumption and depleted-action completion behavior.
    pub ammunition: AttackAmmunition,
    /// Weighted attack-animation variants in visual order.
    pub animations: Vec<AttackAnimation>,
    /// Inclusive pre-attack cooldown roll range in seconds.
    pub pre_attack_cooldown: [f32; 2],
    /// Inclusive post-attack cooldown roll range in seconds.
    pub post_attack_cooldown: [f32; 2],
    /// Weighted reload animation duration in seconds.
    pub reload_duration: f32,
    /// Attack tags before a visual reload; zero disables visual ammo.
    pub visual_ammo: u32,
    /// Whether height bonus damage participates in the hit calculation.
    pub uses_height_bonus_damage: bool,
}

impl AttackProfile {
    /// Return retail's maximum Attack-tag count across animation variants.
    #[must_use]
    pub fn maximum_attacks_per_animation(&self) -> u32 {
        self.animations
            .iter()
            .map(|animation| animation.attack_positions.len())
            .max()
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or_default()
    }
}

#[derive(Debug, Default)]
pub(super) struct TimingAssetCache {
    visuals: BTreeMap<String, Result<Visual, String>>,
    animation_durations: BTreeMap<String, Result<f32, String>>,
}

pub(super) struct AttackProfileLoad {
    pub profiles: BTreeMap<String, AttackProfile>,
    pub issues: Vec<(String, String)>,
}

pub(super) fn load_attack_profiles(
    object: &ProtoObject,
    tactics: &TacticData,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> AttackProfileLoad {
    let mut loaded = AttackProfileLoad {
        profiles: BTreeMap::new(),
        issues: Vec::new(),
    };
    let ranged_actions = tactics
        .actions
        .iter()
        .filter(|action| uses_ranged_attack_executor(action))
        .filter_map(|action| resolve_action(tactics, action))
        .collect::<Vec<_>>();
    if ranged_actions.is_empty() {
        return loaded;
    }

    let visual = match load_visual(object, source, cache) {
        Ok(visual) => visual,
        Err(reason) => {
            loaded.issues.extend(
                ranged_actions
                    .into_iter()
                    .map(|action| (action.action.name.clone(), reason.clone())),
            );
            return loaded;
        }
    };

    for ranged in ranged_actions {
        match build_attack_profile(object, &visual, ranged, source, cache) {
            Ok(profile) => {
                loaded
                    .profiles
                    .insert(profile.action_name.to_ascii_lowercase(), profile);
            }
            Err(reason) => loaded.issues.push((ranged.action.name.clone(), reason)),
        }
    }
    loaded
}

fn resolve_action<'a>(tactics: &'a TacticData, action: &'a Action) -> Option<RangedAction<'a>> {
    let weapon_name = action.weapon.as_deref()?;
    let weapon = tactics
        .weapons
        .iter()
        .find(|weapon| weapon.name.eq_ignore_ascii_case(weapon_name))?;
    Some(RangedAction { action, weapon })
}

pub(super) fn load_visual(
    object: &ProtoObject,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> Result<Visual, String> {
    let visual_ref = object
        .visual
        .as_deref()
        .ok_or_else(|| "proto object has no visual reference".to_owned())?;
    let path = canonical_visual_path(visual_ref);
    let key = path.to_ascii_lowercase();
    if !cache.visuals.contains_key(&key) {
        let parsed = source
            .read_xmb(&path)
            .ok_or_else(|| format!("visual {path} was not found or was not valid XMB"))
            .and_then(|document| {
                visual::parse(&document).map_err(|error| format!("failed to parse {path}: {error}"))
            });
        cache.visuals.insert(key.clone(), parsed);
    }
    cache
        .visuals
        .get(&key)
        .expect("visual cache entry was inserted")
        .clone()
}

fn build_attack_profile(
    object: &ProtoObject,
    visual: &Visual,
    ranged: RangedAction<'_>,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> Result<AttackProfile, String> {
    let animation_name = ranged
        .action
        .anim
        .as_ref()
        .map(|animation| animation.name.trim())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "unit attack has no attack animation".to_owned())?;
    let model = select_attack_model(object, visual, ranged.action, ranged.weapon)
        .ok_or_else(|| "visual has no usable attack model".to_owned())?;
    let animation = find_animation(model, animation_name).ok_or_else(|| {
        format!(
            "visual model {} has no {animation_name} animation",
            model.name
        )
    })?;
    let variants = load_attack_animations(animation, source, cache)?;
    if !variants
        .iter()
        .any(|variant| !variant.attack_positions.is_empty())
    {
        return Err(format!(
            "visual model {} animation {animation_name} has no Attack tags",
            model.name
        ));
    }

    let damage_per_second = ranged
        .weapon
        .damage_per_second
        .filter(|damage| damage.is_finite())
        .unwrap_or_default();
    let cooldown_average = cooldown_average(ranged.weapon);
    let weighted_duration =
        weighted_average(&variants, |variant| variant.duration + cooldown_average)?;
    let weighted_attacks = weighted_average(&variants, |variant| {
        variant.attack_positions.len().to_f32().unwrap_or(f32::MAX)
    })?;
    let denominator = weighted_attacks.max(0.1);
    let uses_dps_as_dpa = ranged.weapon.use_dps_as_dpa == Some(true);
    let mut damage_per_attack = if uses_dps_as_dpa {
        damage_per_second / denominator
    } else {
        damage_per_second * weighted_duration / denominator
    };
    let reload_duration = load_reload_duration(visual, ranged.action, source, cache);
    let visual_ammo = ranged.weapon.visual_ammo.unwrap_or_default();
    if reload_duration > 0.0 && !uses_dps_as_dpa && visual_ammo > 0 && damage_per_second > 0.0 {
        let time_per_attack = damage_per_attack / damage_per_second;
        let attack_time = visual_ammo.to_f32().unwrap_or(f32::MAX) * time_per_attack;
        if attack_time > 0.0 {
            damage_per_attack *= 1.0 + reload_duration / attack_time;
        }
    }
    if !damage_per_attack.is_finite() || damage_per_attack <= 0.0 {
        return Err("computed damage per attack is not positive and finite".to_owned());
    }

    Ok(AttackProfile {
        action_name: ranged.action.name.clone(),
        weapon_name: ranged.weapon.name.clone(),
        weapon_type: ranged.weapon.weapon_type.clone(),
        projectile: ranged.weapon.projectile.clone(),
        area_damage: area_damage_profile(ranged.weapon),
        friendly_fire: ranged.weapon.allow_friendly_fire == Some(true),
        targets_foot_of_unit: ranged.weapon.targets_foot_of_unit == Some(true),
        max_range: finite_nonnegative(ranged.weapon.max_range),
        max_velocity_lead: finite_nonnegative(ranged.weapon.max_velocity_lead),
        accuracy: attack_accuracy_profile(ranged.weapon),
        damage_per_attack,
        ammunition: if ranged.weapon.uses_ammo == Some(true) {
            if ranged.action.stop_attacking_when_ammo_depleted == Some(true) {
                AttackAmmunition::CompleteWhenDepleted
            } else {
                AttackAmmunition::FailWhenDepleted
            }
        } else {
            AttackAmmunition::None
        },
        animations: variants,
        pre_attack_cooldown: cooldown_range(
            ranged.weapon.pre_attack_cooldown_min,
            ranged.weapon.pre_attack_cooldown_max,
        ),
        post_attack_cooldown: cooldown_range(
            ranged.weapon.post_attack_cooldown_min,
            ranged.weapon.post_attack_cooldown_max,
        ),
        reload_duration,
        visual_ammo,
        uses_height_bonus_damage: ranged.weapon.enable_height_bonus_damage == Some(true),
    })
}

fn attack_accuracy_profile(weapon: &Weapon) -> AttackAccuracyProfile {
    let defaults = AttackAccuracyProfile::default();
    AttackAccuracyProfile {
        accuracy: finite_or(weapon.accuracy, defaults.accuracy),
        moving_accuracy: finite_or(weapon.moving_accuracy, defaults.moving_accuracy),
        max_deviation: finite_or(weapon.max_deviation, defaults.max_deviation),
        moving_max_deviation: finite_or(weapon.moving_max_deviation, defaults.moving_max_deviation),
        distance_factor: finite_or(weapon.accuracy_distance_factor, defaults.distance_factor),
        deviation_factor: finite_or(weapon.accuracy_deviation_factor, defaults.deviation_factor),
    }
}

fn area_damage_profile(weapon: &Weapon) -> Option<AreaDamageProfile> {
    let radius = finite_nonnegative(weapon.aoe_radius);
    (radius > 0.0).then(|| AreaDamageProfile {
        radius,
        primary_target_factor: finite_or_default(weapon.aoe_primary_target_factor),
        distance_factor: finite_or_default(weapon.aoe_distance_factor),
        damage_factor: finite_or_default(weapon.aoe_damage_factor),
        linear_damage: weapon.aoe_linear_damage == Some(true),
        ignores_y_axis: weapon.aoe_ignores_y_axis == Some(true),
        friendly_fire: weapon.allow_friendly_fire == Some(true),
    })
}

fn select_attack_model<'a>(
    object: &ProtoObject,
    visual: &'a Visual,
    action: &Action,
    weapon: &Weapon,
) -> Option<&'a Model> {
    let hardpoint = weapon.hardpoint.as_deref().and_then(|name| {
        object
            .hardpoints
            .iter()
            .find(|hardpoint| hardpoint.name.eq_ignore_ascii_case(name))
    });
    if let Some(hardpoint) = hardpoint
        && hardpoint.single_bone_ik != Some(true)
    {
        if let Some(model) = hardpoint
            .pitch_attachment
            .as_deref()
            .and_then(|name| find_model(visual, name))
        {
            return Some(model);
        }
        if let Some(model) = hardpoint
            .yaw_attachment
            .as_deref()
            .and_then(|name| find_model(visual, name))
        {
            return Some(model);
        }
    } else if let Some(model) = squad_mode_model(visual, action.squad_mode.as_deref()) {
        return Some(model);
    }

    visual
        .default_model
        .as_deref()
        .and_then(|name| find_model(visual, name))
        .or_else(|| visual.models.first())
}

fn squad_mode_model<'a>(visual: &'a Visual, squad_mode: Option<&str>) -> Option<&'a Model> {
    let squad_mode = squad_mode?;
    let logic = visual.logic.as_ref()?;
    if !logic.logic_type.eq_ignore_ascii_case("SquadMode") {
        return None;
    }
    let model_name = logic
        .entries
        .iter()
        .find(|entry| entry.value.eq_ignore_ascii_case(squad_mode))?
        .model_ref
        .as_deref()?;
    find_model(visual, model_name)
}

fn find_model<'a>(visual: &'a Visual, name: &str) -> Option<&'a Model> {
    visual
        .models
        .iter()
        .find(|model| model.name.eq_ignore_ascii_case(name))
}

pub(super) fn find_animation<'a>(model: &'a Model, name: &str) -> Option<&'a Anim> {
    model
        .anims
        .iter()
        .find(|animation| animation.anim_type.eq_ignore_ascii_case(name))
}

fn load_attack_animations(
    animation: &Anim,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> Result<Vec<AttackAnimation>, String> {
    let mut variants = Vec::new();
    let mut failures = Vec::new();
    for asset in animation
        .assets
        .iter()
        .filter(|asset| asset.asset_type.eq_ignore_ascii_case("Anim"))
    {
        let Some(file) = asset.file.as_deref() else {
            failures.push("animation asset has no file".to_owned());
            continue;
        };
        let path = canonical_animation_path(file);
        match load_animation_duration(&path, source, cache) {
            Ok(duration) => {
                let mut attack_positions = asset
                    .tags
                    .iter()
                    .filter(|tag| tag.tag_type.eq_ignore_ascii_case("Attack"))
                    .map(|tag| tag.position.unwrap_or_default().clamp(0.0, 1.0))
                    .collect::<Vec<_>>();
                attack_positions.sort_by(f32::total_cmp);
                variants.push(AttackAnimation {
                    asset_path: path,
                    weight: asset.weight.unwrap_or(1),
                    duration,
                    attack_positions,
                });
            }
            Err(reason) => failures.push(reason),
        }
    }
    if variants.is_empty() {
        let detail = failures.first().map_or("no Anim assets", String::as_str);
        return Err(format!(
            "attack animation has no loadable UAX assets: {detail}"
        ));
    }
    Ok(variants)
}

pub(super) fn load_animation_duration(
    path: &str,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> Result<f32, String> {
    let key = path.to_ascii_lowercase();
    if !cache.animation_durations.contains_key(&key) {
        let duration = source
            .resolve_exact(path)
            .ok_or_else(|| format!("animation {path} was not found"))
            .and_then(|bytes| {
                pipeline::uax::UaxFile::from_bytes(&bytes)
                    .map_err(|error| format!("failed to parse {path}: {error}"))
            })
            .and_then(|animation| {
                animation
                    .duration()
                    .map_err(|error| format!("failed to read duration from {path}: {error}"))
            })
            .and_then(|duration| {
                (duration.is_finite() && duration > 0.0)
                    .then_some(duration)
                    .ok_or_else(|| format!("animation {path} has invalid duration {duration}"))
            });
        cache.animation_durations.insert(key.clone(), duration);
    }
    cache
        .animation_durations
        .get(&key)
        .expect("animation cache entry was inserted")
        .clone()
}

fn load_reload_duration(
    visual: &Visual,
    action: &Action,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> f32 {
    let Some(name) = action
        .reload_anim
        .as_deref()
        .filter(|name| !name.is_empty())
    else {
        return 0.0;
    };
    visual
        .models
        .iter()
        .find_map(|model| find_animation(model, name))
        .and_then(|animation| load_weighted_duration(animation, source, cache).ok())
        .unwrap_or_default()
}

fn load_weighted_duration(
    animation: &Anim,
    source: &mut AssetSource<StdFileProvider>,
    cache: &mut TimingAssetCache,
) -> Result<f32, String> {
    let variants = load_attack_animations(animation, source, cache)?;
    weighted_average(&variants, |variant| variant.duration)
}

fn weighted_average(
    variants: &[AttackAnimation],
    value: impl Fn(&AttackAnimation) -> f32,
) -> Result<f32, String> {
    let weight_total = variants
        .iter()
        .map(|variant| variant.weight.max(0).to_f32().unwrap_or_default())
        .sum::<f32>();
    if !weight_total.is_finite() || weight_total <= 0.0 {
        return Err("animation weights do not have a positive total".to_owned());
    }
    Ok(variants
        .iter()
        .map(|variant| {
            value(variant) * variant.weight.max(0).to_f32().unwrap_or_default() / weight_total
        })
        .sum())
}

fn cooldown_average(weapon: &Weapon) -> f32 {
    let pre = cooldown_range(
        weapon.pre_attack_cooldown_min,
        weapon.pre_attack_cooldown_max,
    );
    let post = cooldown_range(
        weapon.post_attack_cooldown_min,
        weapon.post_attack_cooldown_max,
    );
    (pre[0] + pre[1] + post[0] + post[1]) * 0.5
}

fn cooldown_range(minimum: Option<f32>, maximum: Option<f32>) -> [f32; 2] {
    let maximum = finite_nonnegative(maximum);
    if maximum == 0.0 {
        return [0.0, 0.0];
    }
    let minimum = finite_nonnegative(minimum).min(maximum);
    [minimum, maximum]
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn finite_or_default(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

fn finite_or(value: Option<f32>, fallback: f32) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or(fallback)
}

fn canonical_visual_path(visual_ref: &str) -> String {
    let visual_ref = visual_ref.trim().replace('/', "\\");
    let visual_ref = visual_ref.trim_start_matches('\\');
    if visual_ref.to_ascii_lowercase().starts_with("art\\") {
        visual_ref.to_owned()
    } else {
        format!("art\\{visual_ref}")
    }
}

pub(super) fn canonical_animation_path(animation_ref: &str) -> String {
    let animation_ref = animation_ref.trim().replace('/', "\\");
    let animation_ref = animation_ref.trim_start_matches('\\');
    let base = if animation_ref.to_ascii_lowercase().starts_with("art\\") {
        animation_ref.to_owned()
    } else {
        format!("art\\{animation_ref}")
    };
    if base.to_ascii_lowercase().ends_with(".uax") {
        base
    } else {
        format!("{base}.uax")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn animation(weight: i32, duration: f32, attacks: usize) -> AttackAnimation {
        AttackAnimation {
            asset_path: format!("attack_{duration}.uax"),
            weight,
            duration,
            attack_positions: vec![0.5; attacks],
        }
    }

    #[test]
    fn weighted_values_match_retail_attack_info_math() {
        let variants = [animation(1, 1.0, 1), animation(3, 2.0, 2)];
        assert!(
            (weighted_average(&variants, |variant| variant.duration).unwrap() - 1.75).abs() < 0.001
        );
        assert!(
            (weighted_average(&variants, |variant| {
                variant.attack_positions.len().to_f32().unwrap_or(f32::MAX)
            })
            .unwrap()
                - 1.75)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn canonical_asset_paths_preserve_existing_prefixes_and_extensions() {
        assert_eq!(
            canonical_visual_path("unsc/marine.vis"),
            "art\\unsc\\marine.vis"
        );
        assert_eq!(
            canonical_visual_path("art\\unsc\\marine.vis"),
            "art\\unsc\\marine.vis"
        );
        assert_eq!(
            canonical_animation_path("unsc/marine_attack"),
            "art\\unsc\\marine_attack.uax"
        );
        assert_eq!(
            canonical_animation_path("art\\unsc\\marine_attack.uax"),
            "art\\unsc\\marine_attack.uax"
        );
    }

    #[test]
    fn area_damage_profile_preserves_authored_weapon_contract() {
        let weapon = Weapon {
            aoe_radius: Some(4.0),
            aoe_primary_target_factor: Some(0.25),
            aoe_distance_factor: Some(0.5),
            aoe_damage_factor: Some(0.2),
            aoe_linear_damage: Some(true),
            aoe_ignores_y_axis: Some(true),
            allow_friendly_fire: Some(true),
            ..Weapon::default()
        };
        assert_eq!(
            area_damage_profile(&weapon),
            Some(AreaDamageProfile {
                radius: 4.0,
                primary_target_factor: 0.25,
                distance_factor: 0.5,
                damage_factor: 0.2,
                linear_damage: true,
                ignores_y_axis: true,
                friendly_fire: true,
            })
        );
        assert_eq!(area_damage_profile(&Weapon::default()), None);
    }
}
