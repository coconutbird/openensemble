//! The physics world (`hkpWorld` as `BPhysicsWorld_init`, `0x1407BB390`, sets it up).
//!
//! The world owns the bodies, the hinge constraints and the collision detection. The caller
//! steps it like `BPhysicsWorld_update` (`0x1407BBB40`): [`World::substeps`] splits the elapsed
//! time, and for each substep the caller applies its actions and then calls
//! [`World::step_delta_time`].

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{Mat3, Quat, Vec3};

use crate::body::{Body, BodyDesc, BodyId, MotionType, normalize_or_identity};
use crate::collide::{CollidablePose, CollisionWorld, RayHit, shapes_penetrate};
use crate::contact::{ContactPoint, Manifold, ManifoldId};
use crate::dynamics::{self, DynamicsConfig, Hinge};
use crate::filter::{FilterInfo, GroupFilter};
use crate::heightfield::HeightField;
use crate::math::Transform;
use crate::shape::Shape;

/// The settings of a world. [`Default`] gives the retail values.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldConfig {
    /// The gravity: (0, −21.7, 0) in retail (−9.8 m/s² in game units).
    pub gravity: Vec3,
    /// The collision tolerance (`m_collisionTolerance`): 0.1.
    pub collision_tolerance: f32,
    /// The solver tau: 0.6 (`SOLVER_TYPE_4ITERS_MEDIUM`).
    pub solver_tau: f32,
    /// The solver damping: 1.0.
    pub solver_damping: f32,
    /// The solver steps per world step: 4.
    pub solver_iterations: u32,
    /// `m_deactivationReferenceDistance`: 0.02.
    pub deactivation_reference_distance: f32,
    /// `m_enableDeactivation`: `true`.
    pub enable_deactivation: bool,
    /// The collision filter: the retail layer table.
    pub filter: GroupFilter,
    /// Half the edge of the cubic broadphase world about the origin: 1425
    /// (`BPhysicsWorld_init` calls `setBroadPhaseWorldSize(2850)` at `0x1407BB414`).
    pub broad_phase_half_size: f32,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -21.7, 0.0),
            collision_tolerance: 0.1,
            solver_tau: 0.6,
            solver_damping: 1.0,
            solver_iterations: 4,
            deactivation_reference_distance: 0.02,
            enable_deactivation: true,
            filter: GroupFilter::retail(),
            broad_phase_half_size: 1425.0,
        }
    }
}

/// Identifies a constraint. The ids follow the creation order and are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstraintId(u32);

impl ConstraintId {
    /// Returns the raw value of the id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// The description of a limited hinge (`hkpLimitedHingeConstraintData::setInWorldSpace`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimitedHingeDesc {
    /// The first body.
    pub body_a: BodyId,
    /// The second body.
    pub body_b: BodyId,
    /// The pivot in world space at the current body positions.
    pub pivot: Vec3,
    /// The hinge axis in world space.
    pub axis: Vec3,
    /// The smallest angle (radians).
    pub min_angle: f32,
    /// The largest angle (radians).
    pub max_angle: f32,
    /// The friction torque about the axis. 0 disables the friction.
    pub max_friction_torque: f32,
}

/// A new contact point, reported after the collide step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactAdded {
    /// The manifold of the point.
    pub manifold: ManifoldId,
    /// The first body of the manifold. The normal points toward it.
    pub body_a: BodyId,
    /// The second body of the manifold.
    pub body_b: BodyId,
    /// The key of the point in the manifold.
    pub key: u32,
    /// The geometry of the point.
    pub contact: ContactPoint,
    /// The velocity of A relative to B along the normal. Negative means approaching.
    pub projected_velocity: f32,
}

/// The answer of a [`ContactListener`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactResponse {
    /// `false` removes the point.
    pub accept: bool,
    /// New inverse mass factors of body A and body B for the whole manifold
    /// (`hkpResponseModifier::setInvMassScalingForContact`).
    pub inv_mass_scale: Option<[f32; 2]>,
}

impl ContactResponse {
    /// Keeps the point and changes nothing.
    pub const ACCEPT: Self = Self {
        accept: true,
        inv_mass_scale: None,
    };
}

/// Receives the new contact points of each step (`contactPointAddedCallback`).
pub trait ContactListener {
    /// Called once for each new point, in manifold order and then point order.
    fn contact_point_added(&mut self, event: &ContactAdded) -> ContactResponse;
}

/// The unit type accepts every point.
impl ContactListener for () {
    fn contact_point_added(&mut self, _event: &ContactAdded) -> ContactResponse {
        ContactResponse::ACCEPT
    }
}

/// The step length that `BPhysicsWorld_update` never exceeds.
pub const MAX_SUBSTEP_SECONDS: f32 = 0.0125;

/// A physics world.
#[derive(Clone, Debug)]
pub struct World {
    config: DynamicsConfig,
    collision: CollisionWorld,
    bodies: BTreeMap<BodyId, Body>,
    next_body: u32,
    /// The hinges in id order, with their ids at the same index.
    hinges: Vec<Hinge>,
    hinge_ids: Vec<ConstraintId>,
    next_constraint: u32,
    broad_phase_half_size: f32,
}

impl World {
    /// Makes an empty world.
    #[must_use]
    pub fn new(config: WorldConfig) -> Self {
        Self {
            config: DynamicsConfig {
                gravity: config.gravity,
                solver_tau: config.solver_tau,
                solver_damping: config.solver_damping,
                solver_iterations: config.solver_iterations,
                deactivation_reference_distance: config.deactivation_reference_distance,
                enable_deactivation: config.enable_deactivation,
            },
            collision: CollisionWorld::new(config.filter, config.collision_tolerance),
            bodies: BTreeMap::new(),
            next_body: 0,
            hinges: Vec::new(),
            hinge_ids: Vec::new(),
            next_constraint: 0,
            broad_phase_half_size: config.broad_phase_half_size,
        }
    }

    /// Splits an elapsed time into equal substeps of at most [`MAX_SUBSTEP_SECONDS`]
    /// (`BPhysicsWorld_update`). An elapsed time above 10 s or NaN counts as one substep.
    /// Returns the substep count and length; the count is 0 for no elapsed time.
    #[must_use]
    pub fn substeps(elapsed: f32) -> (u32, f32) {
        let time = if elapsed > 10.0 || elapsed.is_nan() {
            MAX_SUBSTEP_SECONDS
        } else {
            elapsed
        };
        if time <= 0.0 {
            return (0, 0.0);
        }
        let mut count = 0_u32;
        let mut covered = 0.0_f32;
        // `n = (int)(t / 0.0125); if (t − n · 0.0125 > 0) n++`, without a float cast.
        let whole = time / MAX_SUBSTEP_SECONDS;
        while covered + 1.0 <= whole {
            covered += 1.0;
            count += 1;
        }
        if time - covered * MAX_SUBSTEP_SECONDS > 0.0 {
            covered += 1.0;
            count += 1;
        }
        (count, time / covered)
    }

    /// The collision filter.
    #[must_use]
    pub fn filter(&self) -> &GroupFilter {
        self.collision.filter()
    }

    /// The collision filter, for example to give out new system groups.
    pub fn filter_mut(&mut self) -> &mut GroupFilter {
        self.collision.filter_mut()
    }

    /// Adds a body and returns its id.
    pub fn add_body(&mut self, desc: BodyDesc) -> BodyId {
        let id = BodyId::from_raw(self.next_body);
        self.next_body += 1;
        let body = Body::new(id, &desc);
        self.collision.insert(
            id,
            desc.shape,
            desc.filter,
            desc.motion == MotionType::Fixed,
        );
        self.bodies.insert(id, body);
        self.fix_at_broad_phase_border(id);
        id
    }

    /// Removes a body, its manifolds and its constraints.
    pub fn remove_body(&mut self, id: BodyId) {
        if self.bodies.remove(&id).is_none() {
            return;
        }
        self.collision.remove(id);
        let mut index = 0;
        while index < self.hinges.len() {
            if self.hinges[index].body_a == id || self.hinges[index].body_b == id {
                self.hinges.remove(index);
                self.hinge_ids.remove(index);
            } else {
                index += 1;
            }
        }
    }

    /// Adds a limited hinge between two bodies at their current transforms. Returns `None` if
    /// a body is unknown.
    pub fn add_limited_hinge(&mut self, desc: &LimitedHingeDesc) -> Option<ConstraintId> {
        let transform_a = self.bodies.get(&desc.body_a)?.transform();
        let transform_b = self.bodies.get(&desc.body_b)?.transform();
        let mut hinge = Hinge::in_world_space(
            (desc.body_a, &transform_a),
            (desc.body_b, &transform_b),
            desc.pivot,
            desc.axis,
        );
        hinge.min_angle = desc.min_angle;
        hinge.max_angle = desc.max_angle;
        hinge.max_friction_torque = desc.max_friction_torque;
        let id = ConstraintId(self.next_constraint);
        self.next_constraint += 1;
        self.hinges.push(hinge);
        self.hinge_ids.push(id);
        Some(id)
    }

    /// Removes a constraint.
    pub fn remove_constraint(&mut self, id: ConstraintId) {
        if let Ok(index) = self.hinge_ids.binary_search(&id) {
            self.hinges.remove(index);
            self.hinge_ids.remove(index);
        }
    }

    /// The current angle of a hinge as the solver last measured it.
    #[must_use]
    pub fn hinge_angle(&self, id: ConstraintId) -> Option<f32> {
        let index = self.hinge_ids.binary_search(&id).ok()?;
        Some(self.hinges[index].angle())
    }

    /// Runs one Havok step of `dt` seconds (`hkpWorld_stepDeltaTime`): solve and integrate
    /// the active islands, collide, then report the new contact points to the listener. The
    /// broadphase border runs last, as the locked world postpones its motion type changes.
    pub fn step_delta_time(&mut self, dt: f32, listener: &mut dyn ContactListener) {
        let step = self.config.step_info(dt);
        dynamics::step_islands(
            &mut self.bodies,
            self.collision.manifolds_mut(),
            &mut self.hinges,
            &step,
            self.config.enable_deactivation,
        );
        let poses: Vec<CollidablePose> = self
            .bodies
            .iter()
            .map(|(&body, state)| CollidablePose {
                body,
                transform: state.transform(),
                active: state.active,
            })
            .collect();
        self.collision.collide(&poses);
        dynamics::process_new_points(&self.bodies, self.collision.manifolds_mut(), listener);
        let moving: Vec<BodyId> = self
            .bodies
            .iter()
            .filter_map(|(&id, body)| (body.motion != MotionType::Fixed).then_some(id))
            .collect();
        for id in moving {
            self.fix_at_broad_phase_border(id);
        }
    }

    /// `hkpBroadPhaseBorder` (`hkpBroadPhaseBorder_ctor`, `0x14088AB00`) with the default
    /// `BROADPHASE_BORDER_ASSERT`: a body whose broadphase box reaches a face of the broadphase
    /// world becomes fixed (`hkpBroadPhaseBorder_maxPositionExceededCallback`, `0x14088AFD0`).
    fn fix_at_broad_phase_border(&mut self, id: BodyId) {
        let Some(body) = self.bodies.get(&id) else {
            return;
        };
        if body.motion == MotionType::Fixed {
            return;
        }
        let aabb = self
            .collision
            .broad_phase_box(&body.shape, &body.transform());
        let half = self.broad_phase_half_size;
        if aabb.min.min_element() <= -half || aabb.max.max_element() >= half {
            self.set_motion_type(id, MotionType::Fixed);
        }
    }

    /// The bodies in id order.
    pub fn bodies(&self) -> impl Iterator<Item = BodyId> + '_ {
        self.bodies.keys().copied()
    }

    /// The manifolds in id order.
    #[must_use]
    pub fn manifolds(&self) -> &[Manifold] {
        self.collision.manifolds()
    }

    /// The manifolds of one body, in id order.
    pub fn manifolds_of(&self, body: BodyId) -> impl Iterator<Item = &Manifold> {
        self.collision
            .manifolds()
            .iter()
            .filter(move |manifold| manifold.body_a == body || manifold.body_b == body)
    }

    /// Casts a ray against the terrain heightfield only.
    #[must_use]
    pub fn ray_cast_heightfield(&self, from: Vec3, to: Vec3) -> Option<RayHit> {
        self.collision.ray_cast_heightfield(from, to)
    }

    /// The terrain heights, for `flattenInstant`.
    pub fn heightfield_mut(&mut self) -> Option<&mut HeightField> {
        self.collision.heightfield_mut()
    }

    /// The terrain heights (`getHeightAndNormalAt` reads them).
    #[must_use]
    pub fn heightfield(&self) -> Option<&HeightField> {
        self.collision.heightfield()
    }

    /// The gravity of the world (`hkpWorld::getGravity`).
    #[must_use]
    pub fn gravity(&self) -> Vec3 {
        self.config.gravity
    }

    fn body(&self, id: BodyId) -> Option<&Body> {
        self.bodies.get(&id)
    }

    /// The position of the body origin.
    #[must_use]
    pub fn position(&self, id: BodyId) -> Option<Vec3> {
        self.body(id).map(Body::position)
    }

    /// The rotation of the body.
    #[must_use]
    pub fn rotation(&self, id: BodyId) -> Option<Quat> {
        self.body(id).map(|body| body.rotation)
    }

    /// The transform from body space to world space.
    #[must_use]
    pub fn transform(&self, id: BodyId) -> Option<Transform> {
        self.body(id).map(Body::transform)
    }

    /// The centre of mass in world space.
    #[must_use]
    pub fn center_of_mass_world(&self, id: BodyId) -> Option<Vec3> {
        self.body(id).map(|body| body.center_of_mass)
    }

    /// The centre of mass in body space.
    #[must_use]
    pub fn center_of_mass_local(&self, id: BodyId) -> Option<Vec3> {
        self.body(id).map(|body| body.local_center_of_mass)
    }

    /// The linear velocity of the centre of mass.
    #[must_use]
    pub fn linear_velocity(&self, id: BodyId) -> Option<Vec3> {
        self.body(id).map(|body| body.linear_velocity)
    }

    /// The angular velocity in world space (rad/s).
    #[must_use]
    pub fn angular_velocity(&self, id: BodyId) -> Option<Vec3> {
        self.body(id).map(|body| body.angular_velocity)
    }

    /// The velocity of the body at a world point.
    #[must_use]
    pub fn point_velocity(&self, id: BodyId, point: Vec3) -> Option<Vec3> {
        self.body(id).map(|body| body.point_velocity(point))
    }

    /// The mass. Fixed and keyframed bodies keep the mass of their description.
    #[must_use]
    pub fn mass(&self, id: BodyId) -> Option<f32> {
        self.body(id).map(|body| body.mass)
    }

    /// The diagonal inertia in body space.
    #[must_use]
    pub fn inertia_local(&self, id: BodyId) -> Option<Vec3> {
        self.body(id).map(|body| body.inertia)
    }

    /// The inverse inertia tensor in world space (0 for fixed and keyframed bodies).
    #[must_use]
    pub fn inverse_inertia(&self, id: BodyId) -> Option<Mat3> {
        self.body(id).map(Body::inverse_inertia_world)
    }

    /// `true` if the body is awake.
    #[must_use]
    pub fn is_active(&self, id: BodyId) -> Option<bool> {
        self.body(id).map(|body| body.active)
    }

    /// The motion type.
    #[must_use]
    pub fn motion_type(&self, id: BodyId) -> Option<MotionType> {
        self.body(id).map(|body| body.motion)
    }

    /// The linear and angular damping (1/s).
    #[must_use]
    pub fn damping(&self, id: BodyId) -> Option<(f32, f32)> {
        self.body(id)
            .map(|body| (body.linear_damping, body.angular_damping))
    }

    /// `false` if the body has `DEACTIVATOR_NEVER` (see [`World::enable_deactivation`]).
    #[must_use]
    pub fn is_deactivation_enabled(&self, id: BodyId) -> Option<bool> {
        self.body(id).map(|body| body.deactivation.counter != 0xff)
    }

    /// The collision filter information.
    #[must_use]
    pub fn filter_info(&self, id: BodyId) -> Option<FilterInfo> {
        self.body(id).map(|body| body.filter)
    }

    /// The collision shape.
    #[must_use]
    pub fn shape(&self, id: BodyId) -> Option<&Arc<Shape>> {
        self.body(id).map(|body| &body.shape)
    }

    /// `true` if the shapes of two bodies overlap at their current transforms
    /// (`hkpCollisionDispatcher::getPenetrationsFunc` with an `hkpAllCdBodyPairCollector`).
    /// `None` if a body is gone.
    #[must_use]
    pub fn bodies_penetrate(&self, a: BodyId, b: BodyId) -> Option<bool> {
        let (body_a, body_b) = (self.body(a)?, self.body(b)?);
        Some(shapes_penetrate(
            &body_a.shape,
            &body_a.transform(),
            &body_b.shape,
            &body_b.transform(),
        ))
    }

    /// Wakes the island of a body (`hkpEntity_activate`). A fixed body stays inactive.
    pub fn activate(&mut self, id: BodyId) {
        if let Some(body) = self.bodies.get_mut(&id) {
            activate(body);
        }
    }

    /// Puts the island of a body to sleep (`forceDeactivate`): the body and every dynamic
    /// body that a manifold or a hinge joins to it.
    pub fn deactivate(&mut self, id: BodyId) {
        let mut island = vec![id];
        let mut index = 0;
        while index < island.len() {
            let current = island[index];
            index += 1;
            if !self.bodies.get(&current).is_some_and(Body::is_dynamic) {
                continue;
            }
            let joined = self
                .collision
                .manifolds()
                .iter()
                .map(|manifold| (manifold.body_a, manifold.body_b))
                .chain(self.hinges.iter().map(|hinge| (hinge.body_a, hinge.body_b)));
            for (a, b) in joined {
                let other = if a == current {
                    b
                } else if b == current {
                    a
                } else {
                    continue;
                };
                if !island.contains(&other) && self.bodies.get(&other).is_some_and(Body::is_dynamic)
                {
                    island.push(other);
                }
            }
        }
        for member in island {
            if let Some(body) = self.bodies.get_mut(&member)
                && body.motion != MotionType::Fixed
            {
                body.active = false;
            }
        }
    }

    /// Moves the body origin and wakes the body.
    pub fn set_position(&mut self, id: BodyId, position: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.center_of_mass = body.rotation_matrix() * body.local_center_of_mass + position;
            activate(body);
        }
    }

    /// Turns the body about its origin and wakes it. The rotation is normalized.
    pub fn set_rotation(&mut self, id: BodyId, rotation: Quat) {
        if let Some(body) = self.bodies.get_mut(&id) {
            let position = body.position();
            body.rotation = normalize_or_identity(rotation);
            body.center_of_mass = body.rotation_matrix() * body.local_center_of_mass + position;
            activate(body);
        }
    }

    /// Sets the linear velocity and wakes the body. Fixed bodies ignore it.
    pub fn set_linear_velocity(&mut self, id: BodyId, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id)
            && body.motion != MotionType::Fixed
        {
            body.linear_velocity = velocity;
            activate(body);
        }
    }

    /// Sets the angular velocity (world space, rad/s) and wakes the body. Fixed bodies ignore
    /// it.
    pub fn set_angular_velocity(&mut self, id: BodyId, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id)
            && body.motion != MotionType::Fixed
        {
            body.angular_velocity = velocity;
            activate(body);
        }
    }

    /// Sets the linear and angular damping (1/s).
    pub fn set_damping(&mut self, id: BodyId, linear: f32, angular: f32) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.linear_damping = linear;
            body.angular_damping = angular;
        }
    }

    /// Changes the motion type. A fixed body loses its velocity and sleeps; a body that
    /// stops being fixed wakes. The mass properties stay.
    pub fn set_motion_type(&mut self, id: BodyId, motion: MotionType) {
        let Some(body) = self.bodies.get_mut(&id) else {
            return;
        };
        body.motion = motion;
        if motion == MotionType::Fixed {
            body.linear_velocity = Vec3::ZERO;
            body.angular_velocity = Vec3::ZERO;
            body.active = false;
        } else {
            activate(body);
        }
        let shape = Arc::clone(&body.shape);
        // The same shape only updates the filter and the fixed flag of the collidable.
        self.collision
            .insert(id, shape, body.filter, motion == MotionType::Fixed);
    }

    /// Changes the collision filter information.
    pub fn set_filter_info(&mut self, id: BodyId, filter: FilterInfo) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.filter = filter;
            self.collision.set_filter_info(id, filter);
        }
    }

    /// Replaces the collision shape and keeps the mass properties (`hkpRigidBody::setShape`).
    pub fn set_shape(&mut self, id: BodyId, shape: Arc<Shape>) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.shape = Arc::clone(&shape);
            self.collision
                .insert(id, shape, body.filter, body.motion == MotionType::Fixed);
        }
    }

    /// Replaces the diagonal of the inertia in body space (`hkpRigidBody_setInertiaLocal`;
    /// `hkpBoxMotion` keeps only the diagonal).
    pub fn set_inertia_local(&mut self, id: BodyId, inertia: Vec3) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.inertia = inertia;
        }
    }

    /// Switches between the spatial deactivator and `DEACTIVATOR_NEVER`
    /// (`hkpRigidBody_setDeactivator`).
    pub fn enable_deactivation(&mut self, id: BodyId, enabled: bool) {
        if let Some(body) = self.bodies.get_mut(&id) {
            body.deactivation.counter = if enabled {
                u8::try_from(id.raw() & 0xf).unwrap_or(0)
            } else {
                0xff
            };
            if !enabled {
                body.deactivation.inactive_frames = [0, 0];
            }
        }
    }

    /// `hkpRigidBody::applyForce(dt, force)`: a linear impulse of `force · dt`.
    pub fn apply_force(&mut self, id: BodyId, dt: f32, force: Vec3) {
        if let Some(body) = self.dynamic_body(id) {
            body.linear_velocity += force * dt * body.inverse_mass();
        }
    }

    /// `hkpRigidBody::applyForce(dt, force, point)`: a point impulse of `force · dt`.
    pub fn apply_force_at_point(&mut self, id: BodyId, dt: f32, force: Vec3, point: Vec3) {
        if let Some(body) = self.dynamic_body(id) {
            point_impulse(body, force * dt, point);
        }
    }

    /// The DE linear impulse wrapper (`0x1407B6EA0`): skips keyframed bodies, wakes the body
    /// and replaces a NaN or infinite impulse with zero.
    pub fn apply_linear_impulse(&mut self, id: BodyId, impulse: Vec3) {
        if let Some(body) = self.dynamic_body(id) {
            body.linear_velocity += sanitized(impulse) * body.inverse_mass();
        }
    }

    /// The DE point impulse wrapper (`PhysicsObject_applyImpulseAtPoint`, `0x1407B6DB0`):
    /// skips keyframed bodies, wakes the body and replaces a NaN or infinite impulse or point
    /// with zero.
    pub fn apply_point_impulse(&mut self, id: BodyId, impulse: Vec3, point: Vec3) {
        if let Some(body) = self.dynamic_body(id) {
            point_impulse(body, sanitized(impulse), sanitized(point));
        }
    }

    /// The DE angular impulse wrapper (`0x1407B6F40`): skips keyframed bodies and wakes the
    /// body. Retail does not sanitize this impulse.
    pub fn apply_angular_impulse(&mut self, id: BodyId, impulse: Vec3) {
        if let Some(body) = self.dynamic_body(id) {
            body.angular_velocity += world_inverse_inertia_times(body, impulse);
        }
    }

    /// Wakes a dynamic body and returns it; `None` for other motion types.
    fn dynamic_body(&mut self, id: BodyId) -> Option<&mut Body> {
        let body = self.bodies.get_mut(&id)?;
        if !body.is_dynamic() {
            return None;
        }
        activate(body);
        Some(body)
    }
}

/// Wakes a body that can move. The counts of passed deactivation checks restart.
fn activate(body: &mut Body) {
    if body.motion != MotionType::Fixed && !body.active {
        body.active = true;
        body.deactivation.inactive_frames = [0, 0];
    }
}

/// `R · (diag(1/I) · (Rᵀ · v))`.
fn world_inverse_inertia_times(body: &Body, value: Vec3) -> Vec3 {
    let rotation = body.rotation_matrix();
    rotation * (body.inverse_inertia_local() * (rotation.transpose() * value))
}

/// Applies an impulse at a world point.
fn point_impulse(body: &mut Body, impulse: Vec3, point: Vec3) {
    body.linear_velocity += impulse * body.inverse_mass();
    let torque = (point - body.center_of_mass).cross(impulse);
    body.angular_velocity += world_inverse_inertia_times(body, torque);
}

/// Zero for a vector with a NaN or infinite component (the DE `isOk3` check).
fn sanitized(value: Vec3) -> Vec3 {
    if value.is_finite() { value } else { Vec3::ZERO }
}

#[cfg(test)]
mod tests;
