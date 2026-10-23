//! Contact points and manifolds.
//!
//! The collision detection keeps one manifold for each pair of bodies that touch or almost
//! touch. The solver reads the manifolds and writes the impulses back into the points.

use glam::Vec3;

use crate::body::BodyId;

/// Identifies a manifold. The ids follow the creation order and are never reused.
///
/// The solver uses this order as the order of the contact constraints, like the order in which
/// Havok adds constraints to an island.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManifoldId(u64);

impl ManifoldId {
    /// Makes an id from its raw value.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw value of the id.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// The geometry of a contact point (`hkContactPoint`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactPoint {
    /// The world position of the point. The solver uses this one position for the lever arms
    /// of both bodies.
    pub position: Vec3,
    /// The unit normal in world space. It points from body B toward body A, so moving A along
    /// the normal separates the bodies.
    pub normal: Vec3,
    /// The signed distance along the normal: positive for a gap, negative for a penetration.
    pub distance: f32,
}

/// The solver data of a contact point (`hkContactPointProperties`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactProperties {
    /// The mixed friction of the two materials.
    pub friction: f32,
    /// The mixed restitution of the two materials.
    pub restitution: f32,
    /// The normal impulse that the solver applied to this point in the last step.
    pub impulse_applied: f32,
    /// `true` from the creation of the point until the dynamics step processes it for the first
    /// time (the Havok new-point flag).
    pub is_new: bool,
}

/// A contact point in a manifold.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ManifoldPoint {
    /// The key that the collision agent gives the point, for example the index of the
    /// collision sphere or the feature pair of a box. A point keeps its key while it exists.
    pub key: u32,
    /// The current geometry of the point.
    pub contact: ContactPoint,
    /// The solver data of the point. The collision detection keeps it unchanged while the
    /// point exists.
    pub properties: ContactProperties,
}

/// The contact points between two bodies.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifold {
    /// The id of the manifold.
    pub id: ManifoldId,
    /// The first body. Contact normals point toward this body.
    pub body_a: BodyId,
    /// The second body.
    pub body_b: BodyId,
    /// The current points.
    pub points: Vec<ManifoldPoint>,
    /// The factors for the inverse masses of body A and body B in this contact
    /// (`hkpResponseModifier::setInvMassScalingForContact`). Both are 1 by default.
    pub inv_mass_scale: [f32; 2],
}

impl Manifold {
    /// Makes an empty manifold between two bodies, with an inverse mass scale of 1.
    #[must_use]
    pub fn new(id: ManifoldId, body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            id,
            body_a,
            body_b,
            points: Vec::new(),
            inv_mass_scale: [1.0, 1.0],
        }
    }
}
