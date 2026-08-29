//! Rage landing impact, physics nudge, and heal-on-kill behavior.

use super::super::common::prototype_lifetime_ms;
use super::execution::{PendingRageKill, RagePowerExecution};
use crate::EntityId;
use crate::gameplay::{AreaDamageProfile, GameplayCatalog};
use crate::world::World;
use crate::world::combat::AttackDamage;
use glam::Vec3;
use pipeline::database::hw1::Database;
use pipeline::database::hw1::tactics::Weapon;

#[derive(Debug, Clone)]
struct ImpactProfile {
    damage: f32,
    weapon_type: Option<String>,
    area_damage: Option<AreaDamageProfile>,
}

pub(super) fn apply_landing_impact(
    world: &mut World,
    execution: &RagePowerExecution,
    gameplay: Option<&GameplayCatalog>,
) {
    let Some(target_squad) = world.get_squad(execution.target_squad_id) else {
        return;
    };
    let target_unit_ids = target_squad.unit_ids.clone();
    let Some(target_id) = target_unit_ids
        .iter()
        .find(|id| world.get_unit(**id).is_some())
        .copied()
    else {
        return;
    };
    if let Some((leader_id, damage_multiplier, profile)) = impact_attack(world, execution, gameplay)
    {
        let target_position = world
            .get_unit(target_id)
            .map_or(execution.teleport_destination, |unit| unit.base.position);
        let direction = target_position - execution.teleport_destination;
        let attack = AttackDamage {
            attacker_id: leader_id,
            attacker_player_id: execution.player_id,
            primary_target_id: Some(target_id),
            ground_zero: execution.teleport_destination,
            direction,
            damage: profile.damage * damage_multiplier,
            weapon_type: profile.weapon_type,
            area_damage: profile.area_damage,
        };
        let _dealt = world.apply_attack_damage(&attack, gameplay);
    }
    nudge_units(
        world,
        &target_unit_ids,
        execution.teleport_destination,
        execution.nudge_multiplier,
    );
}

fn impact_attack(
    world: &World,
    execution: &RagePowerExecution,
    gameplay: Option<&GameplayCatalog>,
) -> Option<(EntityId, f32, ImpactProfile)> {
    let leader_id = world
        .get_squad(execution.owner_squad_id)?
        .unit_ids
        .iter()
        .find(|id| world.get_unit(**id).is_some())
        .copied()?;
    let multiplier = world.get_unit(leader_id)?.effective_damage_multiplier();
    let tactics = gameplay?.object(&execution.projectile_prototype)?.tactics();
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
    Some((leader_id, multiplier, impact_profile(weapon)?))
}

fn impact_profile(weapon: &Weapon) -> Option<ImpactProfile> {
    let damage = weapon
        .damage_per_second
        .filter(|value| value.is_finite() && *value > 0.0)?;
    let radius = finite_nonnegative(weapon.aoe_radius);
    let area_damage = (radius > 0.0).then(|| AreaDamageProfile {
        radius,
        primary_target_factor: finite_or_zero(weapon.aoe_primary_target_factor),
        distance_factor: finite_or_zero(weapon.aoe_distance_factor),
        damage_factor: finite_or_zero(weapon.aoe_damage_factor),
        linear_damage: weapon.aoe_linear_damage == Some(true),
        ignores_y_axis: weapon.aoe_ignores_y_axis == Some(true),
        friendly_fire: weapon.allow_friendly_fire == Some(true),
    });
    Some(ImpactProfile {
        damage,
        weapon_type: weapon.weapon_type.clone(),
        area_damage,
    })
}

fn nudge_units(world: &mut World, ids: &[EntityId], origin: Vec3, multiplier: f32) {
    if multiplier <= 0.0 {
        return;
    }
    for id in ids {
        let Some((mass, point, direction)) = world.get_unit(*id).and_then(|unit| {
            let body = unit.physics.as_ref()?;
            let mut direction = Vec3::new(
                unit.base.position.x - origin.x,
                0.0,
                unit.base.position.z - origin.z,
            )
            .normalize_or_zero();
            if direction == Vec3::ZERO {
                direction = Vec3::Z;
            }
            direction.y = 1.0;
            Some((
                body.material().mass.max(0.0),
                unit.base.position + body.collider().center_offset,
                direction,
            ))
        }) else {
            continue;
        };
        if let Some(unit) = world.get_unit_mut(*id) {
            let _applied = unit.apply_impulse_at_point(direction * mass * multiplier, point);
        }
    }
}

pub(super) fn resolve_pending_kills(world: &mut World, database: Option<&Database>) {
    let events = std::mem::take(&mut world.power_manager.pending_rage_kills);
    let Some(database) = database else {
        return;
    };
    for event in &events {
        resolve_kill(world, database, event);
    }
}

fn resolve_kill(world: &mut World, database: &Database, event: &PendingRageKill) {
    let Some(owner_squad_id) = world
        .get_unit(event.attacker_id)
        .and_then(|unit| unit.squad_id)
    else {
        return;
    };
    let Some((heal_scale, heal_prototype_id, heal_prototype)) = world
        .power_manager
        .rage_executions
        .iter()
        .find(|execution| execution.owner_squad_id == owner_squad_id)
        .map(|execution| {
            (
                execution.heal_per_kill_combat_value,
                execution.heal_attachment_prototype_id,
                execution.heal_attachment_prototype.clone(),
            )
        })
    else {
        return;
    };
    let combat_value = database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(&event.target_prototype))
        .and_then(|prototype| prototype.combat_value)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_default();
    let healing = heal_scale * combat_value;
    if !healing.is_finite() || healing <= f32::EPSILON {
        return;
    }
    world.repair_squads_by_combat_value(database, &[owner_squad_id], healing, false, true);
    attach_heal_effect(
        world,
        database,
        owner_squad_id,
        heal_prototype_id,
        &heal_prototype,
    );
}

fn attach_heal_effect(
    world: &mut World,
    database: &Database,
    squad_id: EntityId,
    prototype_id: i32,
    prototype_name: &str,
) {
    let Some(leader_id) = world.get_squad(squad_id).and_then(|squad| {
        squad
            .unit_ids
            .iter()
            .find(|id| world.get_unit(**id).is_some())
            .copied()
    }) else {
        return;
    };
    let Some(attachment_id) =
        world.add_prototype_attachment_to_unit(database, leader_id, prototype_id)
    else {
        return;
    };
    let lifetime = database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(prototype_name))
        .and_then(prototype_lifetime_ms);
    if let Some(lifetime) = lifetime {
        world
            .power_manager
            .track_visual(attachment_id, world.game_time_ms.wrapping_add(lifetime));
    }
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}
