//! Body identifiers.

/// Identifies a body in a physics world.
///
/// The world gives out the ids in creation order and never reuses an id. The broadphase, the
/// contact pairs and the solver use this order as the deterministic order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyId(u32);

impl BodyId {
    /// Makes an id from its raw value.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw value of the id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}
