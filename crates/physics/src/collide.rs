//! The collision detection of the world: the broadphase, the agents and the manifolds.
//!
//! [`CollisionWorld::collide`] runs the Havok 5.1 collide step of the discrete simulation:
//!
//! 1. Compute the box of each body, expanded by half the collision tolerance
//!    (`hkpEntityAabbUtil`, `0x140894850` uses `0.5 · m_tolerance`).
//! 2. Find the overlapping pairs. Remove the agent and the manifold of each pair that stopped
//!    overlapping.
//! 3. Make an agent and an empty manifold for each new pair, unless both bodies are fixed or
//!    the group filter disables the pair.
//! 4. Run the agent of each pair that has at least one active body. The manifolds of sleeping
//!    pairs stay as they are.
//!
//! The terrain heightfield keeps the height range that it had when it was inserted, like the
//! retail shape that computes its range once (`0x140874E70`).

mod agent;
mod box_box;
mod broadphase;
mod dispatch;
mod gsk;
mod heightfield_agent;
mod primitive;
mod sink;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::Vec3;

use crate::body::BodyId;
use crate::contact::{Manifold, ManifoldId};
use crate::filter::{FilterInfo, GroupFilter};
use crate::heightfield::HeightField;
use crate::math::Transform;
use crate::shape::{Aabb, Shape};

use agent::{Agent, CdBody, CollisionInput};
use sink::ContactSink;

/// The pose of a body for one collide step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CollidablePose {
    /// The body.
    pub(crate) body: BodyId,
    /// The transform from the shape space of the body to the world.
    pub(crate) transform: Transform,
    /// `true` if the body is active (awake).
    pub(crate) active: bool,
}

/// A ray hit on the terrain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// The hit fraction along the ray, from 0 at the start to 1 at the end.
    pub fraction: f32,
    /// The world position of the hit.
    pub position: Vec3,
    /// The unit normal of the hit triangle in world space.
    pub normal: Vec3,
}

/// A body that takes part in the collision detection (`hkpCollidable`).
#[derive(Clone, Debug)]
struct Collidable {
    shape: Arc<Shape>,
    filter: FilterInfo,
    fixed: bool,
    /// The height range of a heightfield shape, cached at the insertion.
    height_range: Option<(f32, f32)>,
    /// The last transform from [`CollisionWorld::collide`].
    transform: Transform,
}

impl Collidable {
    fn new(shape: Arc<Shape>, filter: FilterInfo, fixed: bool) -> Self {
        let (height_range, transform) = match shape.as_ref() {
            Shape::HeightField(field) => (
                Some(field.height_range()),
                Transform {
                    translation: field.body_position(),
                    ..Transform::IDENTITY
                },
            ),
            _ => (None, Transform::IDENTITY),
        };
        Self {
            shape,
            filter,
            fixed,
            height_range,
            transform,
        }
    }

    /// Returns the broadphase box at the last transform.
    fn aabb(&self, tolerance: f32) -> Aabb {
        match (self.shape.as_ref(), self.height_range) {
            (Shape::HeightField(field), Some(range)) => {
                field.aabb(&self.transform, tolerance, range)
            }
            (shape, _) => shape.aabb(&self.transform, tolerance),
        }
    }
}

/// The agent of a body pair and the key counter of its manifold.
#[derive(Clone, Debug)]
struct Pair {
    agent: Agent,
    next_key: u32,
}

/// The collision detection of a physics world.
///
/// It keeps the bodies with their shapes and filters, one agent and one manifold for each pair
/// of bodies with overlapping boxes, and the manifolds in [`ManifoldId`] order.
#[derive(Clone, Debug)]
pub(crate) struct CollisionWorld {
    filter: GroupFilter,
    input: CollisionInput,
    collidables: BTreeMap<BodyId, Collidable>,
    /// The manifolds in id order.
    manifolds: Vec<Manifold>,
    /// The agent of each manifold, at the same index.
    pairs: Vec<Pair>,
    next_manifold: u64,
}

impl CollisionWorld {
    /// Makes an empty collision world with a group filter and a collision tolerance.
    pub(crate) fn new(filter: GroupFilter, tolerance: f32) -> Self {
        Self {
            filter,
            input: CollisionInput::new(tolerance),
            collidables: BTreeMap::new(),
            manifolds: Vec::new(),
            pairs: Vec::new(),
            next_manifold: 0,
        }
    }

    /// Returns the group filter.
    pub(crate) fn filter(&self) -> &GroupFilter {
        &self.filter
    }

    /// Returns the group filter for changes, for example new system groups.
    ///
    /// A change to the layer table does not remove existing manifolds.
    pub(crate) fn filter_mut(&mut self) -> &mut GroupFilter {
        &mut self.filter
    }

    /// Adds a collidable, or replaces its shape and filter.
    ///
    /// A new shape removes the manifolds of the body, like `setShape` removes its agents. A new
    /// filter removes the manifolds that the filter now disables.
    pub(crate) fn insert(
        &mut self,
        body: BodyId,
        shape: Arc<Shape>,
        filter: FilterInfo,
        fixed: bool,
    ) {
        match self.collidables.get_mut(&body) {
            Some(collidable) if Arc::ptr_eq(&collidable.shape, &shape) => {
                collidable.filter = filter;
                collidable.fixed = fixed;
                self.remove_disabled_pairs(body);
            }
            Some(collidable) => {
                let transform = collidable.transform;
                let mut replacement = Collidable::new(shape, filter, fixed);
                if replacement.height_range.is_none() {
                    replacement.transform = transform;
                }
                *collidable = replacement;
                self.retain_pairs(|a, b| a != body && b != body);
            }
            None => {
                self.collidables
                    .insert(body, Collidable::new(shape, filter, fixed));
            }
        }
    }

    /// Removes a collidable and its manifolds.
    pub(crate) fn remove(&mut self, body: BodyId) {
        if self.collidables.remove(&body).is_some() {
            self.retain_pairs(|a, b| a != body && b != body);
        }
    }

    /// Changes the filter info and drops the manifolds that the filter now disables.
    pub(crate) fn set_filter_info(&mut self, body: BodyId, filter: FilterInfo) {
        if let Some(collidable) = self.collidables.get_mut(&body) {
            collidable.filter = filter;
            self.remove_disabled_pairs(body);
        }
    }

    /// Runs the broadphase and the agents.
    ///
    /// `poses` has every inserted body once, sorted by id: the world transform of its shape
    /// space and whether it is active. A body without a pose takes no part in this step.
    pub(crate) fn collide(&mut self, poses: &[CollidablePose]) {
        for pose in poses {
            if let Some(collidable) = self.collidables.get_mut(&pose.body) {
                collidable.transform = pose.transform;
            }
        }
        let half_tolerance = 0.5 * self.input.tolerance;
        let boxes: Vec<(BodyId, Aabb)> = poses
            .iter()
            .filter_map(|pose| {
                self.collidables
                    .get(&pose.body)
                    .map(|collidable| (pose.body, collidable.aabb(half_tolerance)))
            })
            .collect();
        let overlaps = broadphase::overlapping_pairs(&boxes);
        self.retain_pairs(|a, b| overlaps.binary_search(&(a, b)).is_ok());
        let mut existing: Vec<(BodyId, BodyId)> = self
            .manifolds
            .iter()
            .map(|manifold| (manifold.body_a, manifold.body_b))
            .collect();
        existing.sort_unstable();
        for &(a, b) in &overlaps {
            if existing.binary_search(&(a, b)).is_err() && self.is_pair_enabled(a, b) {
                self.add_pair(a, b);
            }
        }
        let is_active = |body: BodyId| {
            poses
                .binary_search_by_key(&body, |pose| pose.body)
                .is_ok_and(|index| poses[index].active)
        };
        for (manifold, pair) in self.manifolds.iter_mut().zip(&mut self.pairs) {
            if !(is_active(manifold.body_a) || is_active(manifold.body_b)) {
                continue;
            }
            let (Some(a), Some(b)) = (
                self.collidables.get(&manifold.body_a),
                self.collidables.get(&manifold.body_b),
            ) else {
                continue;
            };
            let body_a = CdBody {
                shape: &a.shape,
                transform: a.transform,
            };
            let body_b = CdBody {
                shape: &b.shape,
                transform: b.transform,
            };
            let mut sink = ContactSink::new(manifold, &mut pair.next_key);
            pair.agent.process(&body_a, &body_b, &self.input, &mut sink);
        }
    }

    /// Returns the manifolds in [`ManifoldId`] order.
    ///
    /// New points have `properties.is_new == true` and default properties.
    pub(crate) fn manifolds(&self) -> &[Manifold] {
        &self.manifolds
    }

    /// Returns the manifolds in [`ManifoldId`] order, for the solver and the listeners.
    ///
    /// The solver may change the point properties and remove points. An agent adds a removed
    /// point again on the next step if the shapes still touch.
    pub(crate) fn manifolds_mut(&mut self) -> &mut [Manifold] {
        &mut self.manifolds
    }

    /// Casts a ray from `from` to `to` against the terrain heightfield only.
    ///
    /// The terrain is the heightfield body with the lowest id. The ray uses the transform of the
    /// terrain from the last collide step, or the retail terrain position before the first step.
    pub(crate) fn ray_cast_heightfield(&self, from: Vec3, to: Vec3) -> Option<RayHit> {
        let (field, transform) = self.terrain()?;
        let to_local = transform.inverse();
        let hit =
            field.cast_ray_local(to_local.transform_point(from), to_local.transform_point(to))?;
        Some(RayHit {
            fraction: hit.fraction,
            position: from + (to - from) * hit.fraction,
            normal: transform.transform_vector(hit.normal),
        })
    }

    /// Returns the terrain heights for changes (`flattenInstant`).
    ///
    /// The collision world keeps the old height range of the terrain box, like retail.
    pub(crate) fn heightfield_mut(&mut self) -> Option<&mut HeightField> {
        let collidable = self
            .collidables
            .values_mut()
            .find(|collidable| matches!(collidable.shape.as_ref(), Shape::HeightField(_)))?;
        match Arc::make_mut(&mut collidable.shape) {
            Shape::HeightField(field) => Some(Arc::make_mut(field)),
            _ => None,
        }
    }

    /// Returns the terrain heightfield and its transform.
    fn terrain(&self) -> Option<(&HeightField, Transform)> {
        self.collidables
            .values()
            .find_map(|collidable| match collidable.shape.as_ref() {
                Shape::HeightField(field) => Some((field.as_ref(), collidable.transform)),
                _ => None,
            })
    }

    /// Returns `true` if the pair can get an agent: not both fixed, and enabled by the filter.
    fn is_pair_enabled(&self, a: BodyId, b: BodyId) -> bool {
        match (self.collidables.get(&a), self.collidables.get(&b)) {
            (Some(a), Some(b)) => {
                !(a.fixed && b.fixed) && self.filter.is_collision_enabled(a.filter, b.filter)
            }
            _ => false,
        }
    }

    /// Makes the agent and the empty manifold of a new pair.
    fn add_pair(&mut self, a: BodyId, b: BodyId) {
        let (Some(collidable_a), Some(collidable_b)) =
            (self.collidables.get(&a), self.collidables.get(&b))
        else {
            return;
        };
        let agent = Agent::new(&collidable_a.shape, &collidable_b.shape);
        let id = ManifoldId::from_raw(self.next_manifold);
        self.next_manifold += 1;
        self.manifolds.push(Manifold::new(id, a, b));
        self.pairs.push(Pair { agent, next_key: 0 });
    }

    /// Removes the pairs of a body that the filter now disables.
    fn remove_disabled_pairs(&mut self, body: BodyId) {
        let kept: Vec<bool> = self
            .manifolds
            .iter()
            .map(|manifold| {
                (manifold.body_a != body && manifold.body_b != body)
                    || self.is_pair_enabled(manifold.body_a, manifold.body_b)
            })
            .collect();
        self.retain_flagged(&kept);
    }

    /// Keeps only the pairs for which `keep(body_a, body_b)` is `true`.
    fn retain_pairs(&mut self, keep: impl Fn(BodyId, BodyId) -> bool) {
        let kept: Vec<bool> = self
            .manifolds
            .iter()
            .map(|manifold| keep(manifold.body_a, manifold.body_b))
            .collect();
        self.retain_flagged(&kept);
    }

    /// Keeps the manifold and the agent at each index whose flag is `true`.
    fn retain_flagged(&mut self, kept: &[bool]) {
        let mut flags = kept.iter();
        self.manifolds
            .retain(|_| flags.next().copied().unwrap_or(true));
        let mut flags = kept.iter();
        self.pairs.retain(|_| flags.next().copied().unwrap_or(true));
    }
}
