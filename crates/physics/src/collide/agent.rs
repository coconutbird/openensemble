//! The collision agents of a body pair and the dispatch from shape types to agents.
//!
//! Retail registers the Havok 5.1 agents in `hkpAgentRegisterUtil::registerAllAgents`
//! (`0x140D94750`). The discrete world selects the agent of a pair through the agent3 table
//! (`hkAgentNnMachine_getAgentType`, `0x1408A34C0`). Each pair of shape types gets one agent:
//!
//! - a transform of a shape that is not convex, against any shape: the transform agent
//!   (`hkpTransformAgent`, registered last, so it wins against a list);
//! - a list shape against any other shape: one child agent for each child (`hkpListAgent`);
//! - a heightfield against a convex shape: the heightfield agent with the collision spheres;
//! - two convex shapes: [`dispatch::convex_pair`] (box–box, the primitive agents or GSK).
//!
//! Retail registers some pairs in one order only. For the other order, the world swaps the two
//! bodies when it creates the agent (`0x1408928D0`), so the manifold takes the agent order.
//! Inside a list or a transform the bodies cannot swap; there the agent runs with the bodies
//! swapped and its points are turned back (`hkpSymmetricAgent`).
//!
//! Two lists use `hkpCollectionCollectionAgent3` in retail, which expands both lists. This
//! crate expands the list of body A first; the same child pairs meet.

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
            (Shape::Transform { child, .. }, _) if !child.is_convex() => {
                Self::Transform(Box::new(Self::new(child, b)))
            }
            (_, Shape::Transform { child, .. }) if !child.is_convex() => Self::symmetric(a, b),
            (Shape::List { children }, _) => {
                Self::List(children.iter().map(|child| Self::new(child, b)).collect())
            }
            (Shape::HeightField(_), _) | (_, Shape::List { .. }) => Self::symmetric(a, b),
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Vec3;

    use super::Agent;
    use crate::heightfield::HeightField;
    use crate::math::Transform;
    use crate::shape::{DEFAULT_CONVEX_RADIUS, Shape};

    fn cube() -> Arc<Shape> {
        Arc::new(Shape::Box {
            half_extents: Vec3::ONE,
            radius: DEFAULT_CONVEX_RADIUS,
        })
    }

    fn terrain() -> Shape {
        Shape::HeightField(Arc::new(
            HeightField::new(2, 1.0, vec![0.0; 4]).expect("grid"),
        ))
    }

    fn list() -> Shape {
        Shape::List {
            children: vec![cube(), Arc::new(Shape::Sphere { radius: 1.0 })],
        }
    }

    fn moved_list() -> Shape {
        Shape::Transform {
            transform: Transform::IDENTITY,
            child: Arc::new(list()),
        }
    }

    #[test]
    fn a_convex_body_against_the_terrain_uses_the_heightfield_agent() {
        assert!(matches!(
            Agent::new(&cube(), &terrain()),
            Agent::HeightField(_)
        ));
        let swapped = Agent::new(&terrain(), &cube());
        assert!(
            matches!(swapped, Agent::Symmetric(inner) if matches!(*inner, Agent::HeightField(_)))
        );
        assert!(matches!(Agent::new(&terrain(), &terrain()), Agent::Null));
    }

    #[test]
    fn a_list_runs_one_agent_for_each_child() {
        let Agent::List(children) = Agent::new(&list(), &terrain()) else {
            panic!("list agent expected");
        };
        assert_eq!(children.len(), 2);
        assert!(
            children
                .iter()
                .all(|child| matches!(child, Agent::HeightField(_)))
        );
        assert!(matches!(
            Agent::new(&terrain(), &list()),
            Agent::Symmetric(inner) if matches!(*inner, Agent::List(_))
        ));
    }

    #[test]
    fn a_transform_of_a_list_wins_over_a_list() {
        assert!(matches!(
            Agent::new(&moved_list(), &list()),
            Agent::Transform(inner) if matches!(*inner, Agent::List(_))
        ));
        assert!(matches!(
            Agent::new(&list(), &moved_list()),
            Agent::Symmetric(inner) if matches!(*inner, Agent::Transform(_))
        ));
    }

    #[test]
    fn a_transform_of_a_convex_shape_is_convex() {
        let moved_cube = Shape::Transform {
            transform: Transform::IDENTITY,
            child: cube(),
        };
        assert!(matches!(
            Agent::new(&moved_cube, &terrain()),
            Agent::HeightField(_)
        ));
        assert!(matches!(Agent::new(&moved_cube, &cube()), Agent::Gsk(_)));
    }
}
