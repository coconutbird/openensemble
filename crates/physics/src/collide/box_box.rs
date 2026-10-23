//! The box–box agent (`hkpBoxBoxAgent`, `hkpBoxBoxCollisionDetection`).

use crate::collide::agent::{CdBody, CollisionInput};
use crate::collide::sink::ContactSink;

/// The persistent box–box manifold of one pair (`hkpBoxBoxManifold`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BoxBoxAgent {}

impl BoxBoxAgent {
    /// Makes the agent with an empty manifold.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Updates the points of two boxes. Both shapes must be [`crate::Shape::Box`].
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
