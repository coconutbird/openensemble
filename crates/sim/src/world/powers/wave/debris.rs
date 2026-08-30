//! Damage-part ripping and short-lived gravity-ball spring ownership.

use super::pulling::{PullBody, PullSpring, gravity_ball_impulse};
use super::{WavePowerExecution, ball_position};
use crate::EntityId;
use crate::entities::squads::formation_offset_to_world;
use crate::gameplay::{GameplayCatalog, ThrownDamagePart};
use crate::sync::SyncChecksum;
use crate::world::World;
use glam::Vec3;
use num_traits::ToPrimitive;

const FAKE_OBJECT_LIFESPAN: f32 = 0.3;
const FAKE_OBJECT_ALPHA_FADE: f32 = 0.45;
const THROW_FORCE: f32 = 1_000.0;
const SOURCE_VELOCITY_FACTOR: f32 = 150.0;
const IMPULSE_SCALE_MINIMUM: f32 = 0.6;
const IMPULSE_SCALE_MAXIMUM: f32 = 1.4;
const MAXIMUM_POINT_OFFSET: f32 = 0.1;

/// A Wave-ripped part pulled briefly by a spring without becoming captured debris.
#[derive(Debug, Clone, Copy)]
pub struct WaveFakeObject {
    unit_id: EntityId,
    expires_at_seconds: f32,
    rest_length: f32,
}

impl WaveFakeObject {
    /// Return the detached damage-part entity.
    #[must_use]
    pub const fn unit_id(&self) -> EntityId {
        self.unit_id
    }

    /// Return the execution-local expiry timestamp.
    #[must_use]
    pub const fn expires_at_seconds(&self) -> f32 {
        self.expires_at_seconds
    }
}

pub(super) fn rip_part_off_unit(
    world: &mut World,
    gameplay: Option<&GameplayCatalog>,
    execution: &mut WavePowerExecution,
    source_id: EntityId,
    dt: f32,
) -> Option<EntityId> {
    let source_name = world.get_unit(source_id)?.proto_object_name.clone();
    let damage_parts = gameplay?.damage_parts(&source_name)?;
    let impact_count = damage_parts.impact_point_count();
    if impact_count == 0 {
        return None;
    }
    let impact_count = u32::try_from(impact_count).ok()?;
    let impact_roll = world.trigger_random_index(32_767);
    let part = damage_parts
        .impact_point((impact_roll % impact_count) as usize)?
        .clone();
    spawn_fake_part(world, execution, source_id, &part, dt)
}

fn spawn_fake_part(
    world: &mut World,
    execution: &mut WavePowerExecution,
    source_id: EntityId,
    profile: &ThrownDamagePart,
    dt: f32,
) -> Option<EntityId> {
    let source = world.get_unit(source_id)?.clone();
    let direction = formation_offset_to_world(source.base.forward, profile.throw_center_offset());
    let impulse = randomized_impulse(world, direction, THROW_FORCE * profile.force_multiplier());
    let impulse_point = impulse_point(world, &source, profile, direction);
    let final_impulse = impulse + source.base.velocity * SOURCE_VELOCITY_FACTOR;
    let ground_height = world
        .terrain_height(source.base.position, true)
        .unwrap_or(source.base.position.y.min(0.0));
    let part_id = world.units.allocate_id();
    let mut part =
        source.detached_damage_part(part_id, execution.player_id, profile, ground_height);
    if let Some(body) = &mut part.physics {
        body.set_angular_damping(execution.profile.debris_angular_damping);
    }
    part.set_visual_opacity(1.0);
    let _applied = part.apply_impulse_at_point(final_impulse, impulse_point);
    if let Some(source) = world.get_unit_mut(source_id) {
        source.hide_visual_meshes(profile.mesh_names());
    }
    world.units.insert(part_id, part);
    let rest_length = execution.profile.captured_spring_rest_length
        + execution
            .captured_objects
            .len()
            .to_f32()
            .unwrap_or(f32::MAX)
            * execution.profile.captured_radial_spacing;
    let fake = WaveFakeObject {
        unit_id: part_id,
        expires_at_seconds: execution.elapsed_seconds + FAKE_OBJECT_LIFESPAN,
        rest_length,
    };
    apply_spring(world, execution, fake, dt);
    execution.fake_objects.push(fake);
    Some(part_id)
}

fn randomized_impulse(world: &mut World, direction: Vec3, force: f32) -> Vec3 {
    let mut impulse = direction.try_normalize().unwrap_or_else(|| {
        Vec3::new(
            world.trigger_random_float(-1.0, 1.0),
            world.trigger_random_float(-1.0, 1.0),
            world.trigger_random_float(-1.0, 1.0),
        )
    });
    impulse.x *= world.trigger_random_float(IMPULSE_SCALE_MINIMUM, IMPULSE_SCALE_MAXIMUM);
    impulse.y *= world.trigger_random_float(IMPULSE_SCALE_MINIMUM, IMPULSE_SCALE_MAXIMUM);
    impulse.z *= world.trigger_random_float(IMPULSE_SCALE_MINIMUM, IMPULSE_SCALE_MAXIMUM);
    (impulse + Vec3::Y) * force
}

fn impulse_point(
    world: &mut World,
    source: &crate::Unit,
    profile: &ThrownDamagePart,
    oriented_center: Vec3,
) -> Vec3 {
    let mut point = source.base.position + oriented_center;
    if profile.is_single_mesh() {
        let local_offset = Vec3::new(
            world.trigger_random_float(-MAXIMUM_POINT_OFFSET, MAXIMUM_POINT_OFFSET),
            world.trigger_random_float(-MAXIMUM_POINT_OFFSET, MAXIMUM_POINT_OFFSET),
            world.trigger_random_float(-MAXIMUM_POINT_OFFSET, MAXIMUM_POINT_OFFSET),
        );
        point += formation_offset_to_world(source.base.forward, local_offset);
    }
    point
}

pub(super) fn update_fake_objects(world: &mut World, execution: &mut WavePowerExecution, dt: f32) {
    let active = std::mem::take(&mut execution.fake_objects);
    for fake in active {
        if execution.elapsed_seconds >= fake.expires_at_seconds {
            let _part = world.remove_unit(fake.unit_id);
            continue;
        }
        if world.get_unit(fake.unit_id).is_some() {
            update_opacity(world, execution, fake);
            apply_spring(world, execution, fake, dt);
            execution.fake_objects.push(fake);
        }
    }
}

fn update_opacity(world: &mut World, execution: &WavePowerExecution, fake: WaveFakeObject) {
    let spawned_at = fake.expires_at_seconds - FAKE_OBJECT_LIFESPAN;
    let fade_time = (execution.elapsed_seconds - spawned_at).max(0.0);
    let opacity = 1.0 - fade_time / FAKE_OBJECT_ALPHA_FADE;
    if let Some(part) = world.get_unit_mut(fake.unit_id) {
        part.set_visual_opacity(opacity);
    }
}

fn apply_spring(world: &mut World, execution: &WavePowerExecution, fake: WaveFakeObject, dt: f32) {
    let Some(body) = world.get_unit(fake.unit_id).and_then(pull_body) else {
        return;
    };
    let impulse = gravity_ball_impulse(
        body,
        ball_position(world, execution),
        PullSpring {
            rest_length: fake.rest_length,
            strength: execution.profile.captured_spring_strength,
            damping: execution.profile.captured_spring_dampening,
            minimum_lateral_speed: execution.profile.captured_minimum_lateral_speed,
            dt,
        },
    );
    if let Some(part) = world.get_unit_mut(fake.unit_id) {
        let _applied = part.apply_impulse(impulse);
    }
}

fn pull_body(unit: &crate::Unit) -> Option<PullBody> {
    unit.physics.as_ref().map(|body| PullBody {
        center: unit.base.position + body.collider().center_offset,
        velocity: unit.base.velocity,
        mass: body.material().mass,
    })
}

pub(super) fn hash_fake_objects(checksum: &mut SyncChecksum, values: &[WaveFakeObject]) {
    checksum.hash_u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
    for value in values {
        checksum.hash_u32(value.unit_id.as_u32());
        checksum.hash_f32(value.expires_at_seconds);
        checksum.hash_f32(value.rest_length);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonzero_direction_consumes_only_axis_scale_samples() {
        let mut actual = World::with_seed(12);
        let mut expected = World::with_seed(12);
        let impulse = randomized_impulse(&mut actual, Vec3::X, 10.0);
        let x = expected.trigger_random_float(0.6, 1.4);
        let _y = expected.trigger_random_float(0.6, 1.4);
        let _z = expected.trigger_random_float(0.6, 1.4);
        assert!(impulse.abs_diff_eq(Vec3::new(x * 10.0, 10.0, 0.0), 0.000_1));
        assert_eq!(
            actual.trigger_random_index(32_767),
            expected.trigger_random_index(32_767)
        );
    }
}
