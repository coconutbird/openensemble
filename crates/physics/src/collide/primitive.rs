//! The agents of primitive shape pairs (sphere, capsule and box pairs with a dedicated agent).

use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;
use crate::shape::Shape;

/// The persistent state of a primitive agent.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PrimitiveAgent {}

impl PrimitiveAgent {
    /// Makes the agent for two primitive shapes.
    pub(crate) fn new(a: &Shape, b: &Shape) -> Self {
        let _ = (a, b);
        Self::default()
    }

    /// Updates the points of two primitive shapes.
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
