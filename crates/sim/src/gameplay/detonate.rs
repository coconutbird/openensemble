//! Authored `Detonate` actions used by squad and unit suicide execution.

use super::{AreaDamageProfile, AttackQuery, GameplayCatalog, ObjectGameplay};
use pipeline::database::hw1::tactics::{Action, Weapon};

const DEFAULT_WORK_RANGE: f32 = 0.1;
const DEFAULT_GLOWY_RANGE: f32 = 0.0;
const DEFAULT_VELOCITY_SCALAR: f32 = 1.0;

/// Immutable values used by retail's squad and unit Detonate actions.
#[derive(Debug, Clone, PartialEq)]
pub struct DetonateActionProfile {
    action_name: String,
    starts_disabled: bool,
    weapon_name: String,
    weapon_type: Option<String>,
    projectile: Option<String>,
    work_range: f32,
    glowy_range: f32,
    velocity_scalar: f32,
    damage_per_second: f32,
    area_damage: Option<AreaDamageProfile>,
    duration: Option<DetonateDurationProfile>,
    physics_trigger_threshold: Option<f32>,
    proximity_trigger: bool,
    proximity_radius: f32,
    death_trigger: bool,
    detonate_throw: DetonateThrowProfile,
}

/// Authored countdown sampled when a unit `Detonate` action is created.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetonateDurationProfile {
    seconds: f32,
    spread: f32,
}

/// Random impulse applied after a physical death replacement explodes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DetonateThrowProfile {
    horizontal_max: f32,
    vertical_max: f32,
}

impl DetonateThrowProfile {
    /// Return the maximum lateral impulse component.
    #[must_use]
    pub const fn horizontal_max(self) -> f32 {
        self.horizontal_max
    }

    /// Return the maximum upward impulse component.
    #[must_use]
    pub const fn vertical_max(self) -> f32 {
        self.vertical_max
    }
}

impl DetonateDurationProfile {
    /// Return the base countdown in seconds.
    #[must_use]
    pub const fn seconds(self) -> f32 {
        self.seconds
    }

    /// Return the symmetric `+/-` countdown variance in seconds.
    #[must_use]
    pub const fn spread(self) -> f32 {
        self.spread
    }
}

impl DetonateActionProfile {
    /// Return the selected tactic action name.
    #[must_use]
    pub fn action_name(&self) -> &str {
        &self.action_name
    }

    /// Return whether this action waits for technology or live enablement.
    #[must_use]
    pub const fn starts_disabled(&self) -> bool {
        self.starts_disabled
    }

    /// Return the authored weapon name used by technology modifiers.
    #[must_use]
    pub fn weapon_name(&self) -> &str {
        &self.weapon_name
    }

    /// Return the weapon type used by target armor modifiers.
    #[must_use]
    pub fn weapon_type(&self) -> Option<&str> {
        self.weapon_type.as_deref()
    }

    /// Return the projectile attached while the squad is glowing.
    #[must_use]
    pub fn projectile(&self) -> Option<&str> {
        self.projectile.as_deref()
    }

    /// Return the center-to-obstruction threshold used to enter attack mode.
    #[must_use]
    pub const fn work_range(&self) -> f32 {
        self.work_range
    }

    /// Return the edge distance at which retail starts the glowing run.
    #[must_use]
    pub const fn glowy_range(&self) -> f32 {
        self.glowy_range
    }

    /// Return the temporary movement scalar applied during the glowing run.
    #[must_use]
    pub const fn velocity_scalar(&self) -> f32 {
        self.velocity_scalar
    }

    /// Return the direct DPS value used as one Detonate explosion's damage.
    #[must_use]
    pub const fn damage_per_second(&self) -> f32 {
        self.damage_per_second
    }

    /// Return the authored area-damage contract for this explosion.
    #[must_use]
    pub const fn area_damage(&self) -> Option<AreaDamageProfile> {
        self.area_damage
    }

    /// Return the authored countdown, when positive.
    #[must_use]
    pub const fn duration(&self) -> Option<DetonateDurationProfile> {
        self.duration
    }

    /// Return the collision speed required to activate the physics trigger.
    #[must_use]
    pub const fn physics_trigger_threshold(&self) -> Option<f32> {
        self.physics_trigger_threshold
    }

    /// Return whether any eligible enemy entering range triggers detonation.
    #[must_use]
    pub const fn has_proximity_trigger(&self) -> bool {
        self.proximity_trigger
    }

    /// Return the weapon max range used by the proximity trigger.
    #[must_use]
    pub const fn proximity_radius(&self) -> f32 {
        self.proximity_radius
    }

    /// Return whether damage or stop notifications at zero HP detonate.
    #[must_use]
    pub const fn has_death_trigger(&self) -> bool {
        self.death_trigger
    }

    /// Return the authored post-detonation physics impulse bounds.
    #[must_use]
    pub const fn detonate_throw(&self) -> DetonateThrowProfile {
        self.detonate_throw
    }
}

impl GameplayCatalog {
    /// Select the first enabled `Detonate` action in authored order.
    ///
    /// Retail's suicide tactics do not contain target rules, so this lookup
    /// deliberately scans the action table instead of the generic work-rule
    /// path. An active tactic state still restricts action membership.
    #[must_use]
    pub fn select_detonate_action(
        &self,
        proto_object_name: &str,
        query: &AttackQuery<'_>,
        mut action_is_enabled: impl FnMut(&Action) -> bool,
    ) -> Option<DetonateActionProfile> {
        let object = self.object(proto_object_name)?;
        object
            .tactics
            .actions
            .iter()
            .filter(|action| is_detonate(action))
            .filter(|action| object.action_available_in_tactic_state(query.tactic_state, action))
            .filter(|action| action_is_enabled(action))
            .find_map(|action| object.detonate_profile(action))
    }

    pub(crate) fn detonate_action(
        &self,
        proto_object_name: &str,
        action_name: &str,
    ) -> Option<DetonateActionProfile> {
        let object = self.object(proto_object_name)?;
        object
            .tactics
            .actions
            .iter()
            .find(|action| action.name.eq_ignore_ascii_case(action_name) && is_detonate(action))
            .and_then(|action| object.detonate_profile(action))
    }

    /// Select the first authored `Detonate` action without opportunity filters.
    ///
    /// Retail uses this path when constructing a physical death replacement.
    #[must_use]
    pub fn first_detonate_action(&self, proto_object_name: &str) -> Option<DetonateActionProfile> {
        let object = self.object(proto_object_name)?;
        object
            .tactics
            .actions
            .iter()
            .find(|action| is_detonate(action))
            .and_then(|action| object.detonate_profile(action))
    }

    /// Select the first persistent `Detonate` action in compiled authored order.
    #[must_use]
    pub(crate) fn first_persistent_detonate_action(
        &self,
        proto_object_name: &str,
    ) -> Option<DetonateActionProfile> {
        let object = self.object(proto_object_name)?;
        let rules = object.tactics.tactic.as_ref()?;
        rules.persistent_actions.iter().find_map(|name| {
            object
                .tactics
                .actions
                .iter()
                .find(|action| action.name.eq_ignore_ascii_case(name) && is_detonate(action))
                .and_then(|action| object.detonate_profile(action))
        })
    }
}

impl ObjectGameplay {
    fn detonate_profile(&self, action: &Action) -> Option<DetonateActionProfile> {
        let weapon_name = action.weapon.as_deref()?;
        let weapon = self
            .tactics
            .weapons
            .iter()
            .find(|weapon| weapon.name.eq_ignore_ascii_case(weapon_name))?;
        Some(DetonateActionProfile {
            action_name: action.name.clone(),
            starts_disabled: action.start_disabled == Some(true),
            weapon_name: weapon.name.clone(),
            weapon_type: weapon.weapon_type.clone(),
            projectile: weapon.projectile.clone(),
            // Compiled retail XMBs materialize absent action floats as zero.
            // Both constructor defaults are positive, and shipped source data
            // authors no explicit zero overrides for these fields.
            work_range: finite_positive(action.work_range, DEFAULT_WORK_RANGE),
            glowy_range: finite_or(action.dodge_chance_max, DEFAULT_GLOWY_RANGE),
            velocity_scalar: finite_positive(action.velocity_scalar, DEFAULT_VELOCITY_SCALAR),
            damage_per_second: finite_nonnegative(weapon.damage_per_second, 0.0),
            area_damage: area_damage_profile(weapon),
            duration: action.duration.as_ref().and_then(|duration| {
                (duration.seconds.is_finite() && duration.seconds > 0.0).then_some(
                    DetonateDurationProfile {
                        seconds: duration.seconds,
                        spread: finite_nonnegative(duration.spread, 0.0),
                    },
                )
            }),
            physics_trigger_threshold: action
                .detonate_from_physics
                .as_ref()
                .map(|trigger| finite_nonnegative(trigger.threshold, 0.0)),
            proximity_trigger: action.detonate_when_in_range == Some(true),
            proximity_radius: finite_nonnegative(weapon.max_range, 0.0),
            death_trigger: action.detonate_on_death == Some(true),
            detonate_throw: action.detonate_throw.as_ref().map_or_else(
                DetonateThrowProfile::default,
                |throw| DetonateThrowProfile {
                    horizontal_max: finite_nonnegative(throw.horizontal_max, 0.0),
                    vertical_max: finite_nonnegative(throw.vertical_max, 0.0),
                },
            ),
        })
    }
}

fn is_detonate(action: &Action) -> bool {
    action
        .action_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Detonate"))
}

fn area_damage_profile(weapon: &Weapon) -> Option<AreaDamageProfile> {
    let radius = finite_nonnegative(weapon.aoe_radius, 0.0);
    (radius > 0.0).then(|| AreaDamageProfile {
        radius,
        primary_target_factor: finite_or(weapon.aoe_primary_target_factor, 0.0),
        distance_factor: finite_or(weapon.aoe_distance_factor, 0.0),
        damage_factor: finite_or(weapon.aoe_damage_factor, 0.0),
        linear_damage: weapon.aoe_linear_damage == Some(true),
        ignores_y_axis: weapon.aoe_ignores_y_axis == Some(true),
        friendly_fire: weapon.allow_friendly_fire == Some(true),
    })
}

fn finite_nonnegative(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

fn finite_positive(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(fallback)
}

fn finite_or(value: Option<f32>, fallback: f32) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SquadMode;
    use crate::gameplay::{AttackQueryFlags, TacticRelation, TacticStateId};
    use pipeline::database::hw1::tactics::{TacticData, TacticState};
    use pipeline::database::hw1::{Database, ProtoObject};

    #[test]
    fn authored_order_enablement_state_and_defaults_select_suicide_weapon() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "suicide".to_owned(),
            tactics: Some("suicide.tactics".to_owned()),
            ..ProtoObject::default()
        });
        let tactics = TacticData {
            weapons: vec![Weapon {
                name: "Bomb".to_owned(),
                damage_per_second: Some(1_400.0),
                weapon_type: Some("Basic".to_owned()),
                projectile: Some("plasma_grenade".to_owned()),
                aoe_radius: Some(12.0),
                ..Weapon::default()
            }],
            states: vec![TacticState {
                actions: vec!["UpgradedBomb".to_owned()],
                ..TacticState::default()
            }],
            actions: vec![detonate_action("BaseBomb"), detonate_action("UpgradedBomb")],
            ..TacticData::default()
        };
        let gameplay = GameplayCatalog::from_tactics(&database, [("suicide".to_owned(), tactics)]);
        let query = AttackQuery {
            relation: TacticRelation::Enemy,
            squad_mode: SquadMode::Normal,
            ability_id: None,
            target_proto_object_name: None,
            tactic_state: TacticStateId::from_index(0),
            flags: AttackQueryFlags::empty(),
        };

        let profile = gameplay
            .select_detonate_action("suicide", &query, |_| true)
            .expect("state-selected Detonate action");
        assert_eq!(profile.action_name(), "UpgradedBomb");
        assert_eq!(profile.weapon_name(), "Bomb");
        assert_eq!(profile.weapon_type(), Some("Basic"));
        assert_eq!(profile.projectile(), Some("plasma_grenade"));
        assert_eq!(profile.work_range().to_bits(), DEFAULT_WORK_RANGE.to_bits());
        assert_eq!(
            profile.glowy_range().to_bits(),
            DEFAULT_GLOWY_RANGE.to_bits()
        );
        assert_eq!(
            profile.velocity_scalar().to_bits(),
            DEFAULT_VELOCITY_SCALAR.to_bits()
        );
        assert_eq!(profile.damage_per_second().to_bits(), 1_400.0_f32.to_bits());
        assert_eq!(profile.area_damage().map(|area| area.radius), Some(12.0));
    }

    fn detonate_action(name: &str) -> Action {
        Action {
            name: name.to_owned(),
            action_type: Some("Detonate".to_owned()),
            weapon: Some("Bomb".to_owned()),
            ..Action::default()
        }
    }
}
