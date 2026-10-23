//! The agent choice for two convex shapes.

use crate::collide::agent::Agent;
use crate::collide::box_box::BoxBoxAgent;
use crate::collide::gsk::GskAgent;
use crate::collide::primitive::PrimitiveAgent;
use crate::shape::Shape;

/// Selects the agent of two convex shapes.
///
/// Two boxes use the box–box agent and two spheres the primitive agent. All other convex pairs
/// use the GSK agent.
pub(crate) fn convex_pair(a: &Shape, b: &Shape) -> Agent {
    match (a, b) {
        (Shape::Box { .. }, Shape::Box { .. }) => Agent::BoxBox(BoxBoxAgent::new()),
        (Shape::Sphere { .. }, Shape::Sphere { .. }) => Agent::Primitive(PrimitiveAgent::new(a, b)),
        _ => Agent::Gsk(GskAgent::new(a, b)),
    }
}
