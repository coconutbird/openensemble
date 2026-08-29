//! Deterministic rigid-body primitives used by the simulation.
//!
//! The original game delegates rigid-body integration to Havok, while its
//! gameplay actions provide target velocity, impulses, and collision policy.
//! This module keeps that separation: [`PhysicsBody`] owns deterministic body
//! state and `prepare_squad_movement` bridges squad orders to physical units.

use crate::entities::squads::formation_offset_to_world;
use crate::entities::{BaseEntity, Squad, SquadState, Unit, UnitState};
use crate::entity::{Entity, EntityManager};
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use glam::Vec3;
use num_traits::ToPrimitive;
use std::collections::{BTreeMap, BTreeSet};

/// Largest integration step used by the deterministic physics loop.
pub const MAX_PHYSICS_STEP_SECONDS: f32 = 0.05;

const MAX_SUBSTEPS: f32 = 1_000.0;
const MIN_MASS: f32 = 0.001;
const MIN_ACCELERATION: f32 = 0.001;
const MIN_VECTOR_LENGTH_SQUARED: f32 = 0.000_001;
const ARRIVAL_THRESHOLD: f32 = 0.5;
const GROUND_SNAP_SPEED: f32 = 0.5;
const GRAVITY: f32 = 9.81;
const COLLISION_SLOP: f32 = 0.000_1;
const SOLVER_ITERATIONS: usize = 4;

/// How the simulation is allowed to move a physics body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionType {
    /// An immovable obstruction with zero inverse mass.
    Static,
    /// A force-, impulse-, and velocity-driven body.
    #[default]
    Dynamic,
}

/// Material properties used by integration and contact resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsMaterial {
    /// Body mass. Static bodies ignore this value.
    pub mass: f32,
    /// Coulomb friction coefficient.
    pub friction: f32,
    /// Normal-velocity restitution in the range `0..=1`.
    pub restitution: f32,
    /// Linear velocity damping per second.
    pub linear_damping: f32,
    /// Angular velocity damping per second.
    pub angular_damping: f32,
}

impl Default for PhysicsMaterial {
    fn default() -> Self {
        Self {
            mass: 1.0,
            friction: 0.5,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
        }
    }
}

/// Axis-aligned obstruction box around an entity origin.
///
/// Halo Wars calls these values obstruction radii. Warthogs carry the
/// `DontRotateObstruction` flag, so retaining an axis-aligned box also matches
/// their pathing footprint as they turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxCollider {
    /// Positive half-size on each axis.
    pub half_extents: Vec3,
    /// Collider-center offset from the entity origin.
    pub center_offset: Vec3,
}

impl BoxCollider {
    /// Create a sanitized obstruction box.
    #[must_use]
    pub fn new(half_extents: Vec3, center_offset: Vec3) -> Self {
        Self {
            half_extents: finite_vec3_or_zero(half_extents).abs(),
            center_offset: finite_vec3_or_zero(center_offset),
        }
    }
}

/// Runtime state and configuration for one deterministic physics body.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsBody {
    motion_type: MotionType,
    material: PhysicsMaterial,
    collider: BoxCollider,
    max_speed: f32,
    acceleration: f32,
    turn_rate_radians: f32,
    min_turn_radius: f32,
    max_turn_radius: f32,
    ground_height: f32,
    grounded: bool,
    angular_velocity: Vec3,
    accumulated_force: Vec3,
    accumulated_torque: Vec3,
    contacts_this_step: u16,
}

impl PhysicsBody {
    /// Create an immovable obstruction body.
    #[must_use]
    pub fn static_obstruction(collider: BoxCollider) -> Self {
        Self {
            motion_type: MotionType::Static,
            material: PhysicsMaterial::default(),
            collider,
            max_speed: 0.0,
            acceleration: 0.0,
            turn_rate_radians: 0.0,
            min_turn_radius: 0.0,
            max_turn_radius: 0.0,
            ground_height: 0.0,
            grounded: true,
            angular_velocity: Vec3::ZERO,
            accumulated_force: Vec3::ZERO,
            accumulated_torque: Vec3::ZERO,
            contacts_this_step: 0,
        }
    }

    /// Create a dynamic ground vehicle.
    #[must_use]
    pub fn ground_vehicle(
        material: PhysicsMaterial,
        collider: BoxCollider,
        ground_height: f32,
        max_speed: f32,
        acceleration: f32,
        turn_rate_degrees: f32,
    ) -> Self {
        Self {
            motion_type: MotionType::Dynamic,
            material: sanitize_material(material),
            collider,
            max_speed: finite_nonnegative(max_speed),
            acceleration: finite_nonnegative(acceleration),
            turn_rate_radians: finite_nonnegative(turn_rate_degrees).to_radians(),
            min_turn_radius: 0.0,
            max_turn_radius: 0.0,
            ground_height: finite_or_zero(ground_height),
            grounded: true,
            angular_velocity: Vec3::ZERO,
            accumulated_force: Vec3::ZERO,
            accumulated_torque: Vec3::ZERO,
            contacts_this_step: 0,
        }
    }

    /// Set the speed-dependent turn-radius range used by vehicle steering.
    pub fn set_turn_radius_range(&mut self, minimum: f32, maximum: f32) {
        self.min_turn_radius = finite_nonnegative(minimum);
        self.max_turn_radius = finite_nonnegative(maximum).max(self.min_turn_radius);
    }

    /// Get the body's motion type.
    #[must_use]
    pub const fn motion_type(&self) -> MotionType {
        self.motion_type
    }

    /// Get the body's material configuration.
    #[must_use]
    pub const fn material(&self) -> PhysicsMaterial {
        self.material
    }

    /// Get the body's collider configuration.
    #[must_use]
    pub const fn collider(&self) -> BoxCollider {
        self.collider
    }

    /// Get the configured maximum ground speed.
    #[must_use]
    pub const fn max_speed(&self) -> f32 {
        self.max_speed
    }

    /// Replace the prototype-owned speed while preserving live scalar effects.
    pub(crate) fn set_max_speed(&mut self, max_speed: f32) {
        self.max_speed = finite_nonnegative(max_speed);
    }

    /// Get the configured acceleration and braking rate.
    #[must_use]
    pub const fn acceleration(&self) -> f32 {
        self.acceleration
    }

    /// Get the configured turn rate in degrees per second.
    #[must_use]
    pub fn turn_rate_degrees(&self) -> f32 {
        self.turn_rate_radians.to_degrees()
    }

    /// Get the current angular velocity.
    #[must_use]
    pub const fn angular_velocity(&self) -> Vec3 {
        self.angular_velocity
    }

    /// Check whether the body is resting on its ground plane.
    #[must_use]
    pub const fn is_grounded(&self) -> bool {
        self.grounded
    }

    /// Get the number of unique contacts seen during the latest substep.
    #[must_use]
    pub const fn contacts_this_step(&self) -> u16 {
        self.contacts_this_step
    }

    /// Accumulate a world-space force for the next integration substep.
    pub fn apply_force(&mut self, force: Vec3) {
        if self.motion_type == MotionType::Dynamic && force.is_finite() {
            self.accumulated_force += force;
        }
    }

    /// Apply an immediate world-space linear impulse.
    pub fn apply_impulse(&mut self, entity: &mut BaseEntity, impulse: Vec3) {
        if self.motion_type != MotionType::Dynamic || !impulse.is_finite() {
            return;
        }
        entity.velocity += impulse * self.inverse_mass();
        if entity.velocity.y > 0.0 {
            self.grounded = false;
        }
    }

    /// Apply an immediate impulse at a world-space point.
    pub fn apply_impulse_at_point(&mut self, entity: &mut BaseEntity, impulse: Vec3, point: Vec3) {
        if !point.is_finite() || !impulse.is_finite() {
            return;
        }
        self.apply_impulse(entity, impulse);
        let center = entity.position + self.collider.center_offset;
        self.angular_velocity += self.inverse_inertia() * (point - center).cross(impulse);
    }

    pub(crate) fn update(
        &mut self,
        entity: &mut BaseEntity,
        move_target: Option<Vec3>,
        dt: f32,
        velocity_scalar: f32,
        reverse_move: bool,
    ) -> bool {
        if self.motion_type == MotionType::Static || !valid_step(dt) {
            self.clear_accumulators();
            return false;
        }
        self.integrate_forces(entity, dt);
        let target_before = move_target.map(|target| planar(target - entity.position));
        if let Some(target) = move_target {
            self.drive_toward(entity, target, dt, velocity_scalar, reverse_move);
        }
        self.integrate_angular_velocity(entity, dt);
        entity.position += entity.velocity * dt;
        self.resolve_ground(entity);
        self.clear_accumulators();
        move_target.is_some_and(|target| Self::finish_arrival(entity, target, target_before))
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.motion_type as u32);
        hash_material(checksum, self.material);
        hash_collider(checksum, self.collider);
        checksum.hash_f32(self.max_speed);
        checksum.hash_f32(self.acceleration);
        checksum.hash_f32(self.turn_rate_radians);
        checksum.hash_f32(self.min_turn_radius);
        checksum.hash_f32(self.max_turn_radius);
        checksum.hash_f32(self.ground_height);
        checksum.hash_u32(u32::from(self.grounded));
        hash_vec3(checksum, self.angular_velocity);
        hash_vec3(checksum, self.accumulated_force);
        hash_vec3(checksum, self.accumulated_torque);
        checksum.hash_u32(u32::from(self.contacts_this_step));
    }

    fn inverse_mass(&self) -> f32 {
        if self.motion_type == MotionType::Static {
            0.0
        } else {
            self.material.mass.recip()
        }
    }

    fn inverse_inertia(&self) -> Vec3 {
        if self.motion_type == MotionType::Static {
            return Vec3::ZERO;
        }
        let extents = self.collider.half_extents.max(Vec3::splat(0.01));
        let scale = self.material.mass / 3.0;
        Vec3::new(
            (scale * (extents.y.mul_add(extents.y, extents.z * extents.z))).recip(),
            (scale * (extents.x.mul_add(extents.x, extents.z * extents.z))).recip(),
            (scale * (extents.x.mul_add(extents.x, extents.y * extents.y))).recip(),
        )
    }

    fn integrate_forces(&mut self, entity: &mut BaseEntity, dt: f32) {
        entity.velocity += self.accumulated_force * self.inverse_mass() * dt;
        self.angular_velocity += self.accumulated_torque * self.inverse_inertia() * dt;
        if !self.grounded || entity.velocity.y > 0.0 {
            self.grounded = false;
            entity.velocity.y -= GRAVITY * dt;
        }
        entity.velocity *= damping_factor(self.material.linear_damping, dt);
    }

    fn drive_toward(
        &self,
        entity: &mut BaseEntity,
        target: Vec3,
        dt: f32,
        velocity_scalar: f32,
        reverse_move: bool,
    ) {
        let delta = planar(target - entity.position);
        let distance = delta.length();
        if distance <= ARRIVAL_THRESHOLD {
            entity.velocity.x = 0.0;
            entity.velocity.z = 0.0;
            return;
        }
        let travel_direction = delta / distance;
        let desired_forward = if reverse_move {
            -travel_direction
        } else {
            travel_direction
        };
        let current_speed = planar(entity.velocity).length();
        let acceleration = (self.acceleration * velocity_scalar).max(MIN_ACCELERATION);
        let braking_distance = 0.5 * current_speed * current_speed / acceleration;
        let desired_speed = if distance > braking_distance + ARRIVAL_THRESHOLD {
            self.max_speed * velocity_scalar
        } else {
            0.0
        };
        let next_speed = approach(current_speed, desired_speed, acceleration * dt);
        let max_turn = self.maximum_turn_rate(next_speed, velocity_scalar) * dt;
        let forward = turn_toward(entity.forward, desired_forward, max_turn);
        entity.set_forward(forward);
        let movement_direction = if reverse_move {
            -entity.forward
        } else {
            entity.forward
        };
        entity.velocity.x = movement_direction.x * next_speed;
        entity.velocity.z = movement_direction.z * next_speed;
    }

    fn maximum_turn_rate(&self, speed: f32, velocity_scalar: f32) -> f32 {
        if speed <= 1.0 || self.max_turn_radius <= 0.0 {
            return self.turn_rate_radians;
        }
        let scaled_max_speed = self.max_speed * velocity_scalar;
        let speed_fraction = if scaled_max_speed <= 0.0 {
            0.0
        } else {
            (speed / scaled_max_speed).clamp(0.0, 1.0)
        };
        let radius = ((self.max_turn_radius - self.min_turn_radius)
            .mul_add(speed_fraction, self.min_turn_radius))
        .max(0.001);
        self.turn_rate_radians.min(speed / radius)
    }

    fn integrate_angular_velocity(&mut self, entity: &mut BaseEntity, dt: f32) {
        let yaw = self.angular_velocity.y * dt;
        if yaw.abs() > f32::EPSILON {
            entity.set_forward(rotate_planar(entity.forward, yaw));
        }
        self.angular_velocity *= damping_factor(self.material.angular_damping, dt);
    }

    fn resolve_ground(&mut self, entity: &mut BaseEntity) {
        if entity.position.y > self.ground_height || entity.velocity.y > 0.0 {
            self.grounded = false;
            return;
        }
        entity.position.y = self.ground_height;
        let rebound = (-entity.velocity.y * self.material.restitution).max(0.0);
        if rebound > GROUND_SNAP_SPEED {
            entity.velocity.y = rebound;
            self.grounded = false;
        } else {
            entity.velocity.y = 0.0;
            self.grounded = true;
        }
    }

    fn finish_arrival(entity: &mut BaseEntity, target: Vec3, target_before: Option<Vec3>) -> bool {
        let target_after = planar(target - entity.position);
        let reached = target_after.length() <= ARRIVAL_THRESHOLD
            || target_before.is_some_and(|before| before.dot(target_after) <= 0.0);
        if reached {
            entity.position.x = target.x;
            entity.position.z = target.z;
            entity.velocity.x = 0.0;
            entity.velocity.z = 0.0;
        }
        reached
    }

    fn clear_accumulators(&mut self) {
        self.accumulated_force = Vec3::ZERO;
        self.accumulated_torque = Vec3::ZERO;
    }
}

/// Return a deterministic substep count and duration for an elapsed time.
pub(crate) fn substeps(dt: f32) -> Option<(u32, f32)> {
    if !valid_step(dt) {
        return None;
    }
    let count = (dt / MAX_PHYSICS_STEP_SECONDS)
        .ceil()
        .clamp(1.0, MAX_SUBSTEPS)
        .to_u32()
        .unwrap_or(1);
    let count_as_float = count.to_f32().unwrap_or(1.0);
    Some((count, dt / count_as_float))
}

/// Forward squad orders to physical members and return each physical anchor.
pub(crate) fn prepare_squad_movement(
    squads: &EntityManager<Squad>,
    units: &mut EntityManager<Unit>,
) -> BTreeMap<EntityId, EntityId> {
    let mut anchors = BTreeMap::new();
    for (squad_id, squad) in squads.iter() {
        if squad.garrison.is_garrisoned() || !squad.base.is_mobile() {
            for &unit_id in &squad.unit_ids {
                if let Some(unit) = units.get_mut(unit_id)
                    && unit.is_physics_driven()
                {
                    unit.stop();
                }
            }
            continue;
        }
        let anchor_id = squad.unit_ids.iter().copied().find(|&unit_id| {
            units
                .get(unit_id)
                .is_some_and(|unit| unit.is_physics_driven() && !unit.is_garrisoned())
        });
        let Some(anchor_id) = anchor_id else {
            continue;
        };
        anchors.insert(squad_id, anchor_id);
        for &unit_id in &squad.unit_ids {
            let Some(unit) = units.get_mut(unit_id) else {
                continue;
            };
            if !unit.is_physics_driven() {
                continue;
            }
            if let Some(target) = squad.move_target {
                let offset = formation_offset_to_world(squad.base.forward, unit.formation_offset);
                unit.move_as_squad_member(target + offset);
            } else {
                unit.stop();
            }
        }
    }
    anchors
}

/// Resolve deterministic unit/building obstruction contacts.
pub(crate) fn resolve_unit_collisions(units: &mut EntityManager<Unit>) {
    for (_, unit) in units.iter_mut() {
        if let Some(body) = &mut unit.physics {
            body.contacts_this_step = 0;
        }
    }
    let mut contact_pairs = BTreeSet::new();
    for _ in 0..SOLVER_ITERATIONS {
        let snapshots = collision_snapshots(units);
        let mut deltas = BTreeMap::new();
        let mut found_overlap = false;
        for first_index in 0..snapshots.len() {
            for second_index in (first_index + 1)..snapshots.len() {
                let first = snapshots[first_index];
                let second = snapshots[second_index];
                if same_squad(first, second) {
                    continue;
                }
                let Some(contact) = find_contact(first, second) else {
                    continue;
                };
                found_overlap = true;
                contact_pairs.insert((first.id, second.id));
                accumulate_contact_deltas(&mut deltas, first, second, contact);
            }
        }
        apply_collision_deltas(units, deltas);
        if !found_overlap {
            break;
        }
    }
    record_contact_counts(units, &contact_pairs);
}

/// Synchronize physical squad origins and every non-physical formation member.
pub(crate) fn sync_squad_members(
    squads: &mut EntityManager<Squad>,
    units: &mut EntityManager<Unit>,
    anchors: &BTreeMap<EntityId, EntityId>,
) {
    sync_physical_squad_origins(squads, units, anchors);
    let snapshots: Vec<_> = squads
        .iter()
        .map(|(id, squad)| {
            (
                id,
                squad.base.position,
                squad.base.forward,
                squad.base.velocity,
                squad.unit_ids.clone(),
            )
        })
        .collect();
    for (squad_id, position, forward, velocity, unit_ids) in snapshots {
        for unit_id in unit_ids {
            if anchors.get(&squad_id) == Some(&unit_id) {
                continue;
            }
            let Some(unit) = units.get_mut(unit_id) else {
                continue;
            };
            if unit.squad_id != Some(squad_id) || unit.is_physics_driven() {
                continue;
            }
            unit.base.position =
                position + formation_offset_to_world(forward, unit.formation_offset);
            unit.base.forward = forward;
            unit.base.velocity = velocity;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BodySnapshot {
    id: EntityId,
    squad_id: Option<EntityId>,
    position: Vec3,
    velocity: Vec3,
    collider: BoxCollider,
    inverse_mass: f32,
    material: PhysicsMaterial,
}

#[derive(Debug, Clone, Copy)]
struct Contact {
    normal: Vec3,
    penetration: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct CollisionDelta {
    position: Vec3,
    velocity: Vec3,
}

fn collision_snapshots(units: &EntityManager<Unit>) -> Vec<BodySnapshot> {
    units
        .iter()
        .filter_map(|(id, unit)| {
            let body = unit.physics.as_ref()?;
            (unit.is_alive() && !unit.is_garrisoned()).then_some(BodySnapshot {
                id,
                squad_id: unit.squad_id,
                position: unit.base.position,
                velocity: unit.base.velocity,
                collider: body.collider,
                inverse_mass: body.inverse_mass(),
                material: body.material,
            })
        })
        .collect()
}

fn same_squad(first: BodySnapshot, second: BodySnapshot) -> bool {
    first.squad_id.is_some() && first.squad_id == second.squad_id
}

fn find_contact(first: BodySnapshot, second: BodySnapshot) -> Option<Contact> {
    if first.inverse_mass + second.inverse_mass <= 0.0 {
        return None;
    }
    let first_center = first.position + first.collider.center_offset;
    let second_center = second.position + second.collider.center_offset;
    let center_delta = second_center - first_center;
    let combined = first.collider.half_extents + second.collider.half_extents;
    if center_delta.y.abs() > combined.y + COLLISION_SLOP {
        return None;
    }
    let overlap_x = combined.x - center_delta.x.abs();
    let overlap_z = combined.z - center_delta.z.abs();
    if overlap_x <= 0.0 || overlap_z <= 0.0 {
        return None;
    }
    if overlap_x <= overlap_z {
        Some(Contact {
            normal: Vec3::X * signed_axis(center_delta.x),
            penetration: overlap_x,
        })
    } else {
        Some(Contact {
            normal: Vec3::Z * signed_axis(center_delta.z),
            penetration: overlap_z,
        })
    }
}

fn accumulate_contact_deltas(
    deltas: &mut BTreeMap<EntityId, CollisionDelta>,
    first: BodySnapshot,
    second: BodySnapshot,
    contact: Contact,
) {
    let inverse_mass_sum = first.inverse_mass + second.inverse_mass;
    let correction = contact.normal * ((contact.penetration + COLLISION_SLOP) / inverse_mass_sum);
    add_position_delta(deltas, first.id, -correction * first.inverse_mass);
    add_position_delta(deltas, second.id, correction * second.inverse_mass);
    let relative_velocity = second.velocity - first.velocity;
    let normal_speed = relative_velocity.dot(contact.normal);
    if normal_speed >= 0.0 {
        return;
    }
    let restitution = first.material.restitution.min(second.material.restitution);
    let normal_impulse_size = -(1.0 + restitution) * normal_speed / inverse_mass_sum;
    let normal_impulse = contact.normal * normal_impulse_size;
    let friction_impulse = calculate_friction_impulse(
        relative_velocity,
        contact.normal,
        inverse_mass_sum,
        normal_impulse_size,
        first.material.friction,
        second.material.friction,
    );
    let impulse = normal_impulse + friction_impulse;
    add_velocity_delta(deltas, first.id, -impulse * first.inverse_mass);
    add_velocity_delta(deltas, second.id, impulse * second.inverse_mass);
}

fn calculate_friction_impulse(
    relative_velocity: Vec3,
    normal: Vec3,
    inverse_mass_sum: f32,
    normal_impulse_size: f32,
    first_friction: f32,
    second_friction: f32,
) -> Vec3 {
    let tangent_velocity = relative_velocity - normal * relative_velocity.dot(normal);
    if tangent_velocity.length_squared() <= MIN_VECTOR_LENGTH_SQUARED {
        return Vec3::ZERO;
    }
    let tangent = tangent_velocity.normalize();
    let unconstrained = -relative_velocity.dot(tangent) / inverse_mass_sum;
    let coefficient = (first_friction * second_friction).max(0.0).sqrt();
    let limit = normal_impulse_size.abs() * coefficient;
    tangent * unconstrained.clamp(-limit, limit)
}

fn apply_collision_deltas(
    units: &mut EntityManager<Unit>,
    deltas: BTreeMap<EntityId, CollisionDelta>,
) {
    for (id, delta) in deltas {
        let Some(unit) = units.get_mut(id) else {
            continue;
        };
        unit.base.position += delta.position;
        unit.base.velocity += delta.velocity;
    }
}

fn add_position_delta(deltas: &mut BTreeMap<EntityId, CollisionDelta>, id: EntityId, delta: Vec3) {
    deltas.entry(id).or_default().position += delta;
}

fn add_velocity_delta(deltas: &mut BTreeMap<EntityId, CollisionDelta>, id: EntityId, delta: Vec3) {
    deltas.entry(id).or_default().velocity += delta;
}

fn record_contact_counts(
    units: &mut EntityManager<Unit>,
    contacts: &BTreeSet<(EntityId, EntityId)>,
) {
    for &(first_id, second_id) in contacts {
        increment_contacts(units, first_id);
        increment_contacts(units, second_id);
    }
}

fn increment_contacts(units: &mut EntityManager<Unit>, id: EntityId) {
    if let Some(body) = units.get_mut(id).and_then(|unit| unit.physics.as_mut()) {
        body.contacts_this_step = body.contacts_this_step.saturating_add(1);
    }
}

fn sync_physical_squad_origins(
    squads: &mut EntityManager<Squad>,
    units: &EntityManager<Unit>,
    anchors: &BTreeMap<EntityId, EntityId>,
) {
    for (&squad_id, &anchor_id) in anchors {
        let Some(anchor) = units.get(anchor_id) else {
            continue;
        };
        let Some(squad) = squads.get_mut(squad_id) else {
            continue;
        };
        squad.base.position = anchor.base.position
            - formation_offset_to_world(anchor.base.forward, anchor.formation_offset);
        squad.base.forward = anchor.base.forward;
        squad.base.velocity = anchor.base.velocity;
        if squad.state == SquadState::Moving && anchor.state != UnitState::Moving {
            squad.finish_current_movement();
        }
    }
}

fn turn_toward(current: Vec3, desired: Vec3, maximum_angle: f32) -> Vec3 {
    let current = normalized_planar_or(current, desired);
    let desired = normalized_planar_or(desired, current);
    let dot = current.dot(desired).clamp(-1.0, 1.0);
    let cross_y = current.z.mul_add(desired.x, -current.x * desired.z);
    let angle = cross_y.atan2(dot);
    rotate_planar(current, angle.clamp(-maximum_angle, maximum_angle))
}

fn rotate_planar(vector: Vec3, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    Vec3::new(
        vector.x.mul_add(cos, vector.z * sin),
        0.0,
        (-vector.x).mul_add(sin, vector.z * cos),
    )
    .normalize_or_zero()
}

fn normalized_planar_or(vector: Vec3, fallback: Vec3) -> Vec3 {
    let planar_vector = planar(vector);
    if planar_vector.length_squared() > MIN_VECTOR_LENGTH_SQUARED {
        planar_vector.normalize()
    } else {
        planar(fallback).normalize_or_zero()
    }
}

fn planar(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, 0.0, vector.z)
}

fn approach(current: f32, target: f32, maximum_delta: f32) -> f32 {
    if current < target {
        (current + maximum_delta).min(target)
    } else {
        (current - maximum_delta).max(target)
    }
}

fn damping_factor(damping: f32, dt: f32) -> f32 {
    (1.0 + finite_nonnegative(damping) * dt).recip()
}

fn signed_axis(value: f32) -> f32 {
    if value < 0.0 { -1.0 } else { 1.0 }
}

fn valid_step(dt: f32) -> bool {
    dt.is_finite() && dt > 0.0
}

fn sanitize_material(material: PhysicsMaterial) -> PhysicsMaterial {
    PhysicsMaterial {
        mass: finite_nonnegative(material.mass).max(MIN_MASS),
        friction: finite_nonnegative(material.friction),
        restitution: finite_nonnegative(material.restitution).clamp(0.0, 1.0),
        linear_damping: finite_nonnegative(material.linear_damping),
        angular_damping: finite_nonnegative(material.angular_damping),
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn finite_vec3_or_zero(value: Vec3) -> Vec3 {
    if value.is_finite() { value } else { Vec3::ZERO }
}

fn hash_material(checksum: &mut SyncChecksum, material: PhysicsMaterial) {
    checksum.hash_f32(material.mass);
    checksum.hash_f32(material.friction);
    checksum.hash_f32(material.restitution);
    checksum.hash_f32(material.linear_damping);
    checksum.hash_f32(material.angular_damping);
}

fn hash_collider(checksum: &mut SyncChecksum, collider: BoxCollider) {
    hash_vec3(checksum, collider.half_extents);
    hash_vec3(checksum, collider.center_offset);
}

fn hash_vec3(checksum: &mut SyncChecksum, value: Vec3) {
    checksum.hash_vec3(value.x, value.y, value.z);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Unit;
    use crate::entity_id::EntityClass;

    fn dynamic_body(position: Vec3) -> (BaseEntity, PhysicsBody) {
        let id = EntityId::new(EntityClass::Unit, 0);
        let mut entity = BaseEntity::new(id, 1);
        entity.position = position;
        let body = PhysicsBody::ground_vehicle(
            PhysicsMaterial {
                mass: 100.0,
                friction: 1.0,
                restitution: 0.5,
                linear_damping: 0.0,
                angular_damping: 0.1,
            },
            BoxCollider::new(Vec3::splat(1.0), Vec3::ZERO),
            position.y,
            40.0,
            60.0,
            450.0,
        );
        (entity, body)
    }

    #[test]
    fn vehicle_accelerates_and_turns_with_limits() {
        let (mut entity, mut body) = dynamic_body(Vec3::ZERO);
        let arrived = body.update(
            &mut entity,
            Some(Vec3::new(100.0, 0.0, 0.0)),
            0.05,
            1.0,
            false,
        );

        assert!(!arrived);
        assert!(entity.velocity.length() <= 3.0 + f32::EPSILON);
        assert!(entity.forward.x > 0.0);
        assert!(entity.forward.z > 0.0);
    }

    #[test]
    fn reverse_vehicle_faces_away_while_accelerating_toward_target() {
        let (mut entity, mut body) = dynamic_body(Vec3::ZERO);
        let arrived = body.update(
            &mut entity,
            Some(Vec3::new(100.0, 0.0, 0.0)),
            0.05,
            1.0,
            true,
        );

        assert!(!arrived);
        assert!(entity.forward.x < 0.0);
        assert!(entity.velocity.x > 0.0);
    }

    #[test]
    fn off_center_impulse_changes_linear_and_angular_velocity() {
        let (mut entity, mut body) = dynamic_body(Vec3::ZERO);
        body.apply_impulse_at_point(
            &mut entity,
            Vec3::new(100.0, 20.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        );

        assert!(entity.velocity.x > 0.0);
        assert!(entity.velocity.y > 0.0);
        assert!(body.angular_velocity().y > 0.0);
        assert!(!body.is_grounded());
    }

    #[test]
    fn dynamic_body_is_separated_from_static_obstruction() {
        let mut units = EntityManager::new(EntityClass::Unit);
        let moving_id = units.allocate_id();
        let mut moving = Unit::new(moving_id, 1);
        moving.base.position = Vec3::new(-0.5, 0.0, 0.0);
        moving.physics = Some(dynamic_body(moving.base.position).1);
        moving.base.velocity = Vec3::X;
        units.insert(moving_id, moving);

        let static_id = units.allocate_id();
        let mut obstruction = Unit::new_building(static_id, 0);
        obstruction.physics = Some(PhysicsBody::static_obstruction(BoxCollider::new(
            Vec3::splat(1.0),
            Vec3::ZERO,
        )));
        units.insert(static_id, obstruction);

        resolve_unit_collisions(&mut units);

        let moving = units.get(moving_id).unwrap();
        assert!(moving.base.position.x <= -2.0);
        assert_eq!(moving.physics.as_ref().unwrap().contacts_this_step(), 1);
    }

    #[test]
    fn substeps_bound_large_updates() {
        let (count, duration) = substeps(0.2).unwrap();
        assert_eq!(count, 4);
        assert!((duration - 0.05).abs() < f32::EPSILON);
        assert!(substeps(f32::NAN).is_none());
    }
}
