//! Shared tactic-backed projectile launch data for native player powers.

use super::PowerExecutionId;
use crate::EntityId;
use crate::entities::Projectile;
use crate::entities::projectiles::ProjectileLaunch;
use crate::gameplay::{AreaDamageProfile, GameplayCatalog, ProjectileProfile};
use crate::player::PlayerId;
use crate::world::World;
use glam::Vec3;
use pipeline::database::hw1::tactics::Weapon;

pub(super) struct PowerProjectileAttack<'gameplay> {
    pub weapon: &'gameplay Weapon,
    pub projectile: &'gameplay ProjectileProfile,
}

#[derive(Clone, Copy)]
pub(super) struct PowerProjectileLaunch<'name> {
    pub execution_id: PowerExecutionId,
    pub player_id: PlayerId,
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub tactics_prototype: &'name str,
    pub source: Vec3,
    pub target: Vec3,
    pub target_entity_position: Vec3,
    pub target_offset: Vec3,
    pub damage_bonus: f32,
    pub collides_with_all_units: bool,
}

pub(super) fn first_power_attack<'gameplay>(
    gameplay: Option<&'gameplay GameplayCatalog>,
    tactics_prototype: &str,
) -> Option<PowerProjectileAttack<'gameplay>> {
    let gameplay = gameplay?;
    let tactics = gameplay.object(tactics_prototype)?.tactics();
    let weapon = tactics
        .actions
        .iter()
        .find_map(|action| {
            let name = action.weapon.as_deref()?;
            tactics
                .weapons
                .iter()
                .find(|weapon| weapon.name.eq_ignore_ascii_case(name))
        })
        .or_else(|| tactics.weapons.first())?;
    let projectile = gameplay.projectile(weapon.projectile.as_deref()?)?;
    Some(PowerProjectileAttack { weapon, projectile })
}

pub(super) fn launch_power_projectile(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    request: PowerProjectileLaunch<'_>,
) -> EntityId {
    let Some(attack) = first_power_attack(gameplay, request.tactics_prototype) else {
        return EntityId::INVALID;
    };
    let launch = ProjectileLaunch {
        source_id: request.source_id,
        target_id: request.target_id,
        source_position: request.source,
        target_position: request.target,
        target_entity_position: request.target_entity_position,
        target_offset: request.target_offset,
        target_radius: 0.0,
        max_range: finite_positive(attack.weapon.max_range)
            .unwrap_or_else(|| request.source.distance(request.target).max(1.0)),
        damage: effective_damage(
            world,
            request.player_id,
            request.tactics_prototype,
            attack.weapon,
        ) + finite_nonnegative(Some(request.damage_bonus)),
        weapon_type: attack.weapon.weapon_type.clone(),
        area_damage: area_damage_profile(attack.weapon),
        impact_effect: crate::gameplay::ImpactEffectProfile::from_weapon(attack.weapon),
        friendly_fire: attack.weapon.allow_friendly_fire == Some(true),
        collides_with_all_units: request.collides_with_all_units,
    };
    let projectile_id = world.projectiles.allocate_id();
    let mut projectile =
        Projectile::new(projectile_id, request.player_id, launch, attack.projectile);
    projectile.set_owning_power_execution_id(request.execution_id.get());
    world.projectiles.insert(projectile_id, projectile);
    projectile_id
}

fn effective_damage(
    world: &World,
    player_id: PlayerId,
    tactics_prototype: &str,
    weapon: &Weapon,
) -> f32 {
    let authored = finite_nonnegative(weapon.damage_per_second);
    world.get_player(player_id).map_or(authored, |player| {
        finite_nonnegative(Some(player.technologies.weapon_damage(
            tactics_prototype,
            &weapon.name,
            authored,
        )))
    })
}

fn area_damage_profile(weapon: &Weapon) -> Option<AreaDamageProfile> {
    let radius = finite_nonnegative(weapon.aoe_radius);
    (radius > 0.0).then(|| AreaDamageProfile {
        radius,
        primary_target_factor: finite_or_zero(weapon.aoe_primary_target_factor),
        distance_factor: finite_or_zero(weapon.aoe_distance_factor),
        damage_factor: finite_or_zero(weapon.aoe_damage_factor),
        linear_damage: weapon.aoe_linear_damage == Some(true),
        ignores_y_axis: weapon.aoe_ignores_y_axis == Some(true),
        friendly_fire: weapon.allow_friendly_fire == Some(true),
    })
}

fn finite_positive(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}
