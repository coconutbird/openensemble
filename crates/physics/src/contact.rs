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
///
/// The dynamics step fills the material when it processes a new point, then the solver writes
/// the impulse and its private state back after each step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactProperties {
    /// The mixed friction of the two materials, decoded from its `hkUFloat8` code.
    pub friction: f32,
    /// The mixed restitution of the two materials, in steps of 1/128.
    pub restitution: f32,
    /// The normal impulse that the solver applied to this point in the last step.
    pub impulse_applied: f32,
    /// `true` from the creation of the point until the dynamics step processes it for the first
    /// time: it mixes the material and calls the contact listener.
    pub is_new: bool,
    /// The `hkUFloat8` code of `friction`.
    pub(crate) friction_code: u8,
    /// The restitution byte (`restitution · 128`).
    pub(crate) restitution_code: u8,
    /// The residual position error that the solver exports (`m_internalSolverData`).
    pub(crate) internal_solver_data: f32,
    /// The allowed penetration of the point (`m_internalDataA`). It is never positive after the
    /// first jacobian build.
    pub(crate) internal_data_a: f32,
    /// `true` if the solver pairs this point with the previous point of the manifold (flag bit
    /// 2 of the point, the pair-contact schema).
    pub(crate) paired: bool,
    /// `true` if the restitution bias still waits for the next solve
    /// (`hkpSimpleContactConstraint_processNewPoints_restitution`).
    pub(crate) restitution_pending: bool,
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
    /// The current points. Add and remove points only with [`Manifold::push_point`] and
    /// [`Manifold::remove_point`], because the solver pairs neighbouring points.
    pub points: Vec<ManifoldPoint>,
    /// The factors for the inverse masses of body A and body B in this contact
    /// (`hkpResponseModifier::setInvMassScalingForContact`). Both are 1 by default.
    pub inv_mass_scale: [f32; 2],
    /// The friction state of the manifold (`hkpSimpleContactConstraintDataInfo`).
    pub(crate) solver: ManifoldSolverState,
}

/// The friction state that the solver keeps for a manifold between steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ManifoldSolverState {
    /// `true` after a point was added or removed (`HK_FLAG_AREA_CHANGED`). The solver then
    /// recomputes `radius`.
    pub(crate) area_changed: bool,
    /// The index of the world axis that the first friction tangent is built from. 3 means "not
    /// chosen yet".
    pub(crate) friction_axis: u8,
    /// The mean distance of the points from their centroid, for the angular friction row.
    pub(crate) radius: f32,
    /// The friction results: the impulse and the position drift of the two tangent rows and of
    /// the angular row.
    pub(crate) friction: [f32; 6],
}

impl Default for ManifoldSolverState {
    fn default() -> Self {
        // `hkpSimpleContactConstraintData` ctor (`0x14089C420`) writes 0x30000: flags 0, axis 3.
        Self {
            area_changed: false,
            friction_axis: 3,
            radius: 0.0,
            friction: [0.0; 6],
        }
    }
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
            solver: ManifoldSolverState::default(),
        }
    }

    /// Adds a new point at the end of the manifold and returns its index
    /// (`hkpSimpleContactConstraintAtom_addContactPoint`, `0x14089C4D0`).
    ///
    /// The point gets default properties with `is_new` set. It pairs with the previous point
    /// if that point exists and is not paired itself.
    pub fn push_point(&mut self, key: u32, contact: ContactPoint) -> usize {
        let paired = self
            .points
            .last()
            .is_some_and(|last| !last.properties.paired);
        self.points.push(ManifoldPoint {
            key,
            contact,
            properties: ContactProperties {
                is_new: true,
                paired,
                ..ContactProperties::default()
            },
        });
        self.solver.area_changed = true;
        self.points.len() - 1
    }

    /// Removes the point at `index` and keeps the order of the other points
    /// (`hkpSimpleContactConstraintData_freeContactPoint`, `0x14088BDE0`).
    ///
    /// The point that moves into `index` loses its pairing, as in retail.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of range.
    pub fn remove_point(&mut self, index: usize) {
        self.points.remove(index);
        if let Some(next) = self.points.get_mut(index) {
            next.properties.paired = false;
        }
        self.solver.area_changed = true;
    }
}
