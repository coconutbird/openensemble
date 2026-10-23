//! The collision agents of a body pair and the dispatch from shape types to agents.
//!
//! Retail registers the Havok 5.1 agents in `hkpAgentRegisterUtil::registerAllAgents`
//! (`0x140D94750`). Each pair of shape types gets one agent:
//!
//! - a heightfield against a convex shape: the heightfield agent with the collision spheres;
//! - a list shape against any shape: one child agent for each child (`hkpListAgent`);
//! - a transform of a shape that is not convex: the transform agent (`hkpTransformAgent`);
//! - two convex shapes: [`dispatch::convex_pair`] (box–box, the primitive agents or GSK).
//!
//! A pair in the other order runs the same agent with the bodies swapped
//! (`hkpSymmetricAgent`).

use crate::collide::box_box::BoxBoxAgent;
use crate::collide::dispatch;
use crate::collide::gsk::GskAgent;
use crate::collide::heightfield_agent::HeightFieldAgent;
use crate::collide::primitive::PrimitiveAgent;
use crate::collide::sink::ContactSink;
use crate::math::Transform;
use crate::shape::Shape;

/// The gravity that the collision qualities use (`initCollisionQualityInfo`, `0x140D8DAE0`).
/// It is a constant, not the world gravity.
const QUALITY_GRAVITY: f32 = 9.81;

/// The shortest expected step of the collision qualities (`m_expectedMinPsiDeltaTime`).
const MIN_PSI_DELTA_TIME: f32 = 1.0 / 30.0;

/// A shape at its world transform (`hkpCdBody`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CdBody<'a> {
    /// The shape.
    pub(crate) shape: &'a Shape,
    /// The transform from the shape space to the world.
    pub(crate) transform: Transform,
}

/// The thresholds of a collision quality (`hkpCollisionQualityInfo`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CollisionQuality {
    /// A point stays while its distance is at most this value (`m_keepContact`).
    pub(crate) keep_contact: f32,
    /// A face–vertex or edge–edge point is made at most at this distance
    /// (`m_create4dContact`).
    pub(crate) create_4d_contact: f32,
    /// Any other point is made at most at this distance (`m_createContact`).
    pub(crate) create_contact: f32,
    /// The distance below which a manifold uses its time of impact (`m_manifoldTimDistance`).
    pub(crate) manifold_tim_distance: f32,
}

impl CollisionQuality {
    /// Returns the PSI quality of the retail dispatcher for a tolerance.
    ///
    /// Every pair of Halo Wars bodies uses this quality: dynamic bodies are `DEBRIS`, and
    /// `DEBRIS` against `FIXED`, `KEYFRAMED` or `DEBRIS` maps to PSI.
    pub(crate) fn psi(tolerance: f32) -> Self {
        let gravity_distance = 0.5 * QUALITY_GRAVITY * MIN_PSI_DELTA_TIME * MIN_PSI_DELTA_TIME;
        Self {
            keep_contact: tolerance,
            create_4d_contact: tolerance,
            create_contact: -gravity_distance,
            manifold_tim_distance: -2.0 * gravity_distance,
        }
    }
}

/// The input of the agents for one collide step (`hkpProcessCollisionInput`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CollisionInput {
    /// The collision tolerance of the world (`m_tolerance`).
    pub(crate) tolerance: f32,
    /// The quality of the pair.
    pub(crate) quality: CollisionQuality,
}

impl CollisionInput {
    /// Makes the input of the retail world for a tolerance.
    pub(crate) fn new(tolerance: f32) -> Self {
        Self {
            tolerance,
            quality: CollisionQuality::psi(tolerance),
        }
    }
}

/// The agent of one shape pair. The agent keeps the state that makes its points persistent.
#[derive(Clone, Debug)]
pub(crate) enum Agent {
    /// No contacts for this shape pair (`hkpNullAgent`).
    Null,
    /// A convex shape against a heightfield.
    HeightField(HeightFieldAgent),
    /// Two boxes.
    BoxBox(BoxBoxAgent),
    /// Two convex shapes through GJK and the GSK manifold.
    Gsk(GskAgent),
    /// Two primitive shapes with a dedicated agent.
    Primitive(PrimitiveAgent),
    /// A list shape against a shape: one agent for each child, in the child order.
    List(Vec<Agent>),
    /// A transform of a shape that is not convex, against a shape.
    Transform(Box<Agent>),
    /// The inner agent with the two bodies swapped.
    Symmetric(Box<Agent>),
}

impl Agent {
    /// Selects the agent for the shape of body A and the shape of body B.
    pub(crate) fn new(a: &Shape, b: &Shape) -> Self {
        match (a, b) {
            (Shape::HeightField(_), Shape::HeightField(_)) => Self::Null,
            (Shape::List { children }, _) => {
                Self::List(children.iter().map(|child| Self::new(child, b)).collect())
            }
            (Shape::HeightField(_), _) | (_, Shape::List { .. }) => Self::symmetric(a, b),
            (Shape::Transform { child, .. }, _) if !child.is_convex() => {
                Self::Transform(Box::new(Self::new(child, b)))
            }
            (_, Shape::Transform { child, .. }) if !child.is_convex() => Self::symmetric(a, b),
            (_, Shape::HeightField(_)) if a.is_convex() => {
                Self::HeightField(HeightFieldAgent::new(a))
            }
            _ if a.is_convex() && b.is_convex() => dispatch::convex_pair(a, b),
            _ => Self::Null,
        }
    }

    /// Returns the agent for the swapped pair, wrapped so that it runs swapped.
    fn symmetric(a: &Shape, b: &Shape) -> Self {
        Self::Symmetric(Box::new(Self::new(b, a)))
    }

    /// Updates the points of the pair (`processCollision`).
    ///
    /// The shapes of `a` and `b` must be the shapes that made the agent.
    pub(crate) fn process(
        &mut self,
        a: &CdBody<'_>,
        b: &CdBody<'_>,
        input: &CollisionInput,
        sink: &mut ContactSink<'_>,
    ) {
        match self {
            Self::Null => {}
            Self::HeightField(agent) => {
                if let Shape::HeightField(field) = b.shape {
                    agent.process(a, b.transform, field, input, sink);
                }
            }
            Self::BoxBox(agent) => agent.process(a, b, input, sink),
            Self::Gsk(agent) => agent.process(a, b, input, sink),
            Self::Primitive(agent) => agent.process(a, b, input, sink),
            Self::List(agents) => {
                if let Shape::List { children } = a.shape {
                    for (agent, child) in agents.iter_mut().zip(children) {
                        let child = CdBody {
                            shape: child,
                            transform: a.transform,
                        };
                        agent.process(&child, b, input, sink);
                    }
                }
            }
            Self::Transform(agent) => {
                if let Shape::Transform { transform, child } = a.shape {
                    let child = CdBody {
                        shape: child,
                        transform: a.transform * *transform,
                    };
                    agent.process(&child, b, input, sink);
                }
            }
            Self::Symmetric(agent) => agent.process(b, a, input, &mut sink.flipped()),
        }
    }
}
