//! Wave lightning selection, damage-bank growth, and terminal explosion.

use super::super::common::create_power_visual;
use super::super::projectile::{
    PowerProjectileLaunch, first_power_attack, launch_power_projectile,
};
use super::{WaveGravityBallState, WavePowerExecution, ball_position, owner_leader_id};
use crate::EntityId;
use crate::entities::SquadMode;
use crate::entity::Entity;
use crate::gameplay::GameplayCatalog;
use crate::physics::PhysicsBody;
use crate::player::GAIA_PLAYER;
use crate::world::World;
use glam::{Quat, Vec3};
use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;

const LIGHTNING_DISTANCE_WEIGHT_SCALAR: f32 = 3.0;
const NUDGE_SCALAR_AIR: f32 = 0.25;
const DEBRIS_THROW_DISTANCE: f32 = 65.0;

#[derive(Debug, Clone, Copy)]
struct WeightedSquad {
    squad_id: EntityId,
    weight: f32,
}

#[derive(Debug, Clone)]
struct CapturedDebrisSource {
    position: Vec3,
    center_offset: Vec3,
    velocity: Vec3,
    forward: Vec3,
    proto_object_name: String,
    visual_variation_index: Option<usize>,
    visual_player_id: crate::player::PlayerId,
}

#[derive(Debug, Clone, Copy)]
struct WaveThrowProfile {
    radius: f32,
    velocity: f32,
    offset_radians: f32,
}

pub(super) fn update_lightning(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    units_to_target: &[EntityId],
) -> bool {
    let mut squads = lightning_candidates(world, gameplay, execution, units_to_target);
    if squads.is_empty() || execution.profile.lightning_per_tick == 0 {
        return false;
    }
    squads.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then_with(|| left.squad_id.cmp(&right.squad_id))
    });
    let total_weight = squads.iter().map(|candidate| candidate.weight).sum();
    let mut did_damage = false;
    for _ in 0..execution.profile.lightning_per_tick {
        let random_weight = world.trigger_random_float(0.0, total_weight);
        let squad_id = select_weighted_squad(&squads, total_weight, random_weight);
        did_damage |= hit_lightning(world, database, gameplay, execution, squad_id);
    }
    did_damage
}

fn lightning_candidates(
    world: &World,
    gameplay: Option<&GameplayCatalog>,
    execution: &WavePowerExecution,
    units_to_target: &[EntityId],
) -> Vec<WeightedSquad> {
    let ball = ball_position(world, execution);
    let mut result = Vec::<WeightedSquad>::new();
    for unit_id in units_to_target {
        let Some(unit) = world.get_unit(*unit_id) else {
            continue;
        };
        let hostile = unit.base.player_id == GAIA_PLAYER
            || world.players_are_enemies(execution.player_id, unit.base.player_id);
        if !hostile || !unit.is_attackable() {
            continue;
        }
        let distance = unit.base.position.distance(ball) - unit.obstruction_radius();
        if distance > execution.profile.pulling_range {
            continue;
        }
        let Some(squad_id) = unit.squad_id else {
            continue;
        };
        if result
            .iter()
            .any(|candidate| candidate.squad_id == squad_id)
        {
            continue;
        }
        let Some(squad) = world.get_squad(squad_id) else {
            continue;
        };
        let combat_value = gameplay
            .map(|catalog| catalog.squad_combat_value(&squad.proto_squad_name))
            .unwrap_or_default()
            .max(0.0);
        let planar_distance = Vec3::new(
            unit.base.position.x - ball.x,
            0.0,
            unit.base.position.z - ball.z,
        )
        .length();
        let distance_weight = if execution.profile.pulling_range > 0.0 {
            (planar_distance / execution.profile.pulling_range).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let multiplier = LIGHTNING_DISTANCE_WEIGHT_SCALAR
            + (1.0 - LIGHTNING_DISTANCE_WEIGHT_SCALAR) * distance_weight;
        result.push(WeightedSquad {
            squad_id,
            weight: combat_value * multiplier,
        });
    }
    result
}

fn select_weighted_squad(
    squads: &[WeightedSquad],
    total_weight: f32,
    random_weight: f32,
) -> EntityId {
    let mut selected = squads[0].squad_id;
    let mut remaining = total_weight;
    for candidate in squads {
        remaining -= candidate.weight;
        if remaining <= random_weight {
            selected = candidate.squad_id;
            break;
        }
    }
    selected
}

fn hit_lightning(
    world: &mut World,
    database: &Database,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    squad_id: EntityId,
) -> bool {
    let Some(target_id) = world.get_squad(squad_id).and_then(|squad| {
        squad
            .unit_ids
            .iter()
            .find(|unit_id| {
                world
                    .get_unit(**unit_id)
                    .is_some_and(crate::Unit::is_operational)
            })
            .copied()
    }) else {
        return false;
    };
    let Some((target_position, radius)) = world.get_unit(target_id).map(|unit| {
        (
            unit.base.position,
            unit.obstruction_half_extents
                .x
                .min(unit.obstruction_half_extents.z),
        )
    }) else {
        return false;
    };
    let strike_offset = Vec3::new(
        world.trigger_random_float(-1.0, 1.0) * radius,
        0.0,
        world.trigger_random_float(-1.0, 1.0) * radius,
    );
    let strike_position = target_position + strike_offset;
    let ball = ball_position(world, execution);
    let projectile_id = launch_power_projectile(
        world,
        gameplay,
        PowerProjectileLaunch {
            execution_id: execution.id,
            player_id: execution.player_id,
            source_id: owner_leader_id(world, execution.owner_squad_id),
            target_id,
            tactics_prototype: &execution.profile.lightning_projectile,
            source: ball,
            target: strike_position,
            target_entity_position: target_position,
            target_offset: strike_offset,
            damage_bonus: 0.0,
            collides_with_all_units: true,
        },
    );
    if !projectile_id.is_invalid() {
        execution.active_projectile_ids.push(projectile_id);
    }
    create_lightning_beam(world, database, execution, ball, strike_position);
    let random = world.trigger_random_index(32_767);
    if random % 255 < u32::from(execution.profile.nudge_chance_pulling) {
        nudge_unit(world, execution, target_id, strike_position);
    }
    !projectile_id.is_invalid()
}

fn create_lightning_beam(
    world: &mut World,
    database: &Database,
    execution: &WavePowerExecution,
    ball: Vec3,
    strike: Vec3,
) {
    let Some(prototype) = execution.profile.lightning_beam_visual.as_deref() else {
        return;
    };
    let direction = (strike - ball).normalize_or(Vec3::Y);
    let _beam = create_power_visual(
        world,
        database,
        execution.player_id,
        ball,
        direction,
        prototype,
    );
}

fn nudge_unit(
    world: &mut World,
    execution: &WavePowerExecution,
    unit_id: EntityId,
    strike_position: Vec3,
) {
    let ball = ball_position(world, execution);
    let Some((position, mass, flying)) = world.get_unit(unit_id).and_then(|unit| {
        unit.physics
            .as_ref()
            .map(|body| (unit.base.position, body.material().mass, unit.flying))
    }) else {
        return;
    };
    let mut impulse =
        (ball - position).normalize_or_zero() * execution.profile.nudge_strength * mass;
    if flying {
        impulse *= NUDGE_SCALAR_AIR;
    }
    if let Some(unit) = world.get_unit_mut(unit_id) {
        let _applied = unit.apply_impulse_at_point(impulse, strike_position);
    }
}

pub(super) fn explode(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
) -> bool {
    if execution.state == WaveGravityBallState::Exploding {
        return true;
    }
    let ball = ball_position(world, execution);
    if world.get_unit(execution.ball_object_id).is_none()
        || first_power_attack(gameplay, &execution.profile.explode_projectile).is_none()
        || first_power_attack(gameplay, &execution.profile.debris_projectile).is_none()
    {
        return false;
    }
    let maximum = execution
        .captured_objects
        .len()
        .to_f32()
        .unwrap_or(f32::MAX)
        * execution.profile.maximum_explosion_damage_bank_per_captured;
    let damage_bonus = execution.current_explosion_damage_bank.clamp(0.0, maximum);
    launch_explosion_projectile(world, gameplay, execution, ball, damage_bonus);
    launch_captured_debris(world, gameplay, execution, ball);
    throw_nearby_units(world, gameplay, execution, ball, damage_bonus);
    execution.queued_pickup_objects.clear();
    super::replace_units_to_pull(world, execution, Vec::new());
    execution.explosion_requested = false;
    execution.explode_cooldown_left = execution.profile.explode_time;
    execution.state = WaveGravityBallState::Exploding;
    true
}

fn launch_explosion_projectile(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    ball: Vec3,
    damage_bonus: f32,
) {
    let projectile_id = launch_power_projectile(
        world,
        gameplay,
        PowerProjectileLaunch {
            execution_id: execution.id,
            player_id: execution.player_id,
            source_id: owner_leader_id(world, execution.owner_squad_id),
            target_id: EntityId::INVALID,
            tactics_prototype: &execution.profile.explode_projectile,
            source: ball,
            target: ball,
            target_entity_position: ball,
            target_offset: Vec3::ZERO,
            damage_bonus,
            collides_with_all_units: true,
        },
    );
    if !projectile_id.is_invalid() {
        execution.active_projectile_ids.push(projectile_id);
    }
}

fn launch_captured_debris(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    ball: Vec3,
) {
    let captured = std::mem::take(&mut execution.captured_objects);
    for captured in captured {
        let Some(source) =
            captured_debris_source(world, captured.unit_id, captured.color_player_id)
        else {
            let _attachment = world.remove_object(captured.pickup_attachment_id);
            continue;
        };
        let direction = (source.position - ball).normalize_or(Vec3::X);
        let target = source.position + direction * DEBRIS_THROW_DISTANCE;
        let projectile_id = launch_power_projectile(
            world,
            gameplay,
            PowerProjectileLaunch {
                execution_id: execution.id,
                player_id: execution.player_id,
                source_id: owner_leader_id(world, execution.owner_squad_id),
                target_id: EntityId::INVALID,
                tactics_prototype: &execution.profile.debris_projectile,
                source: source.position,
                target,
                target_entity_position: target,
                target_offset: Vec3::ZERO,
                damage_bonus: 0.0,
                collides_with_all_units: true,
            },
        );
        finish_debris_launch(
            world,
            gameplay,
            execution,
            captured.pickup_attachment_id,
            projectile_id,
            &source,
            direction,
        );
        let _removed = world.remove_unit(captured.unit_id);
    }
}

fn captured_debris_source(
    world: &World,
    unit_id: EntityId,
    visual_player_id: crate::player::PlayerId,
) -> Option<CapturedDebrisSource> {
    let unit = world.get_unit(unit_id)?;
    let center_offset = unit
        .physics
        .as_ref()
        .map_or(Vec3::ZERO, |body| body.collider().center_offset);
    Some(CapturedDebrisSource {
        position: unit.base.position + center_offset,
        center_offset,
        velocity: unit.base.velocity,
        forward: unit.base.forward,
        proto_object_name: unit.proto_object_name.clone(),
        visual_variation_index: unit.object_state.visual_variation_index(),
        visual_player_id,
    })
}

fn finish_debris_launch(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    pickup_attachment_id: EntityId,
    projectile_id: EntityId,
    source: &CapturedDebrisSource,
    direction: Vec3,
) {
    if projectile_id.is_invalid() {
        let _attachment = world.remove_object(pickup_attachment_id);
        return;
    }
    if let Some(projectile) = world.get_projectile_mut(projectile_id) {
        projectile.inherit_source_visual(
            &source.proto_object_name,
            source.visual_variation_index,
            source.visual_player_id,
            source.center_offset,
        );
        let velocity = source.velocity + direction * execution.profile.explosion_force_on_debris;
        let gravity = gameplay.map_or(0.0, GameplayCatalog::projectile_gravity);
        projectile.inherit_source_motion(velocity, source.forward, gravity);
    }
    if !world.reparent_attachment(pickup_attachment_id, projectile_id) {
        let _attachment = world.remove_object(pickup_attachment_id);
    }
    execution.active_projectile_ids.push(projectile_id);
}

fn throw_nearby_units(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &WavePowerExecution,
    ball: Vec3,
    damage_bank: f32,
) {
    if !execution.profile.throw_units_on_explosion
        || execution.maximum_possible_explosion_damage_bank <= 0.0
    {
        return;
    }
    let scalar = damage_bank / execution.maximum_possible_explosion_damage_bank;
    if scalar <= execution.profile.minimum_damage_bank_percent_to_throw {
        return;
    }
    let Some(profile) = wave_throw_profile(gameplay, &execution.profile.explode_projectile) else {
        return;
    };
    let launch_speed = profile.velocity * scalar;
    let leader_id = owner_leader_id(world, execution.owner_squad_id);
    let units = world
        .find_live_squads(None, None, None, Some((ball, profile.radius)))
        .into_iter()
        .flat_map(|squad_id| {
            world
                .get_squad(squad_id)
                .map_or_else(Vec::new, |squad| squad.unit_ids.clone())
        })
        .collect::<Vec<_>>();
    for unit_id in units {
        if unit_id == leader_id || !can_be_thrown(world, unit_id) {
            continue;
        }
        let Some(position) = world.get_unit(unit_id).map(|unit| unit.base.position) else {
            continue;
        };
        let velocity = thrown_velocity(ball, position, profile.offset_radians, launch_speed);
        let Some(release_physics) = ensure_throw_physics(world, gameplay, unit_id) else {
            continue;
        };
        if let Some(unit) = world.get_unit_mut(unit_id) {
            let _started = unit.begin_throw(execution.ball_object_id, velocity, release_physics);
        }
    }
}

fn ensure_throw_physics(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    unit_id: EntityId,
) -> Option<bool> {
    let unit = world.get_unit(unit_id)?;
    if unit.physics.is_some() {
        return Some(false);
    }
    let position = unit.base.position;
    let profile = gameplay?.physics_replacement(&unit.proto_object_name)?;
    let material = profile.material();
    let collider = profile.collider();
    let ground_height = world.terrain_height(position, true).unwrap_or(position.y);
    world.get_unit_mut(unit_id)?.physics = Some(PhysicsBody::dynamic_replacement(
        material,
        collider,
        ground_height,
        position.y,
    ));
    Some(true)
}

fn wave_throw_profile(
    gameplay: Option<&GameplayCatalog>,
    tactics_prototype: &str,
) -> Option<WaveThrowProfile> {
    let weapon = first_power_attack(gameplay, tactics_prototype)?.weapon;
    Some(WaveThrowProfile {
        radius: finite_nonnegative(weapon.aoe_radius),
        velocity: finite_nonnegative(weapon.throw_velocity),
        offset_radians: finite_or_zero(weapon.throw_offset_angle).to_radians(),
    })
}

fn can_be_thrown(world: &World, unit_id: EntityId) -> bool {
    let Some(unit) = world.get_unit(unit_id).filter(|unit| unit.is_alive()) else {
        return false;
    };
    unit.squad_id
        .and_then(|squad_id| world.get_squad(squad_id))
        .is_some_and(|squad| {
            !matches!(
                squad.mode,
                SquadMode::Lockdown | SquadMode::Power | SquadMode::Cover
            )
        })
}

fn thrown_velocity(ball: Vec3, unit: Vec3, offset_radians: f32, speed: f32) -> Vec3 {
    let planar = Vec3::new(unit.x - ball.x, 0.0, unit.z - ball.z).normalize_or(Vec3::X);
    let planar = Quat::from_rotation_y(offset_radians) * planar;
    let launch_angle = 30.0_f32.to_radians();
    Vec3::new(
        planar.x * launch_angle.cos(),
        launch_angle.sin(),
        planar.z * launch_angle.cos(),
    ) * speed
}

fn finite_nonnegative(value: Option<f32>) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_default()
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}
