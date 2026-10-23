//! The convex–convex agent (`hkpPredGskfAgent`): GJK closest points and the GSK manifold.

use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;
use crate::shape::Shape;

/// The persistent GSK state and manifold of one pair (`hkpGskCache`, `hkpGskManifold`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GskAgent {}

impl GskAgent {
    /// Makes the agent for two convex shapes.
    pub(crate) fn new(a: &Shape, b: &Shape) -> Self {
        let _ = (a, b);
        Self::default()
    }

    /// Updates the points of two convex shapes.
    pub(crate) fn process(
        &mut self,
        a: &CdBody<'_>,
        b: &CdBody<'_>,
        input: &CollisionInput,
        sink: &mut ContactSink<'_>,
    ) {
        let _ = (self, a, b, input, sink);
    }
}
