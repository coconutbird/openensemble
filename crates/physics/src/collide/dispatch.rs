//! The agent choice for two convex shapes.
//!
//! # Retail dispatch table
//!
//! `hkpAgentRegisterUtil::registerAllAgents` (`0x140D94750`) first calls
//! `hkRegisterAlternateShapeTypes` (`0x140D94AE0`). Then it registers the agents in this order.
//! A later registration overwrites an earlier one for the same pair
//! (`hkpCollisionDispatcher::internalRegisterCollisionAgent`, `0x140D8E160`):
//!
//! 1. `hkpBvAgent`, `hkpMultiSphereAgent`, the BV tree agents, `hkpShapeCollectionAgent`.
//! 2. `hkpListAgent` (`ALL`), `hkpConvexListAgent`, `hkpHeightFieldAgent`
//!    (`SPHERE_REP` × `HEIGHT_FIELD`), `hkListAgent3`.
//! 3. `hkpCollectionCollectionAgent3` (`COLLECTION` × `COLLECTION`), `hkpTransformAgent`
//!    (`TRANSFORM` × `ALL`), `hkpPhantomAgent`.
//! 4. The terminal agents (`0x140D94830`): `hkpPredGskfAgent` and `hkPredGskAgent3`
//!    (`CONVEX` × `CONVEX`), `hkPredGskCylinderAgent3` (`CYLINDER` × `CONVEX`, both orders, one
//!    agent), `hkpBoxBoxAgent` and `hkBoxBoxAgent3`, `hkpSphereSphereAgent`,
//!    `hkpSphereCapsuleAgent`, `hkpSphereTriangleAgent`, `hkpSphereBoxAgent`,
//!    `hkpCapsuleCapsuleAgent`, then the triangle and multi-ray agents.
//!
//! An agent2 registration (`0x140D8CE30`) also writes the agent3 bridge (type 1) into the
//! agent3 table. A PSI pair has no continuous physics, so the world reads the non-predictive
//! agent3 table (`dispatcher+5312`, `hkAgentNnMachine_getAgentType` `0x1408A34C0`). The bridge
//! then reads the non-predictive agent2 table (`dispatcher+688`).
//!
//! The shape types (`hkpShape::m_type`, offset `0x18`) are 3 sphere, 4 cylinder, 6 box,
//! 7 capsule, 8 convex vertices, 10 list, 13 convex translate, 14 convex transform,
//! 24 sampled heightfield and 28 transform. Every convex type has the alternate type 1
//! (`CONVEX`), and `CONVEX` has the alternate type 25 (`SPHERE_REP`).
//!
//! The convex pairs, with A in the rows (S sphere, C capsule, B box, Cy cylinder, V convex
//! vertices, T convex transform or translate):
//!
//! | A \ B | S | C | B | Cy | V, T |
//! |---|---|---|---|---|---|
//! | S | `SphereSphere` | `SphereCapsule` | `SphereBox` | `GskCylinder` | `Gsk` |
//! | C | symmetric `SphereCapsule` | `CapsuleCapsule` | `Gsk` | `GskCylinder` | `Gsk` |
//! | B | symmetric `SphereBox` | `Gsk` | `BoxBox3` | `GskCylinder` | `Gsk` |
//! | Cy | `GskCylinder` | `GskCylinder` | `GskCylinder` | `GskCylinder` | `GskCylinder` |
//! | V, T | `Gsk` | `Gsk` | `Gsk` | `GskCylinder` | `Gsk` |
//!
//! `Gsk` is `hkPredGskAgent3` and `GskCylinder` is `hkPredGskCylinderAgent3`. Only a bare
//! cylinder selects `GskCylinder`: a convex transform of a cylinder has type 13 or 14.
//!
//! The other HW pairs:
//!
//! - convex × heightfield: `hkpHeightFieldAgent`; heightfield × convex: the same agent with the
//!   bodies swapped. Heightfield × heightfield: no agent.
//! - list × convex or heightfield: the list agent (`hkListAgent3`) on A; convex × list: on B.
//! - list × list: `hkpCollectionCollectionAgent3`.
//! - transform × any shape: `hkpTransformAgent` on A; any shape × transform: on B. Transform
//!   is the last of these registrations, so it comes before the list agents.

use crate::collide::agent::Agent;
use crate::collide::box_box::BoxBoxAgent;
use crate::collide::gsk::GskAgent;
use crate::collide::primitive::PrimitiveAgent;
use crate::shape::Shape;

/// Selects the agent of two convex shapes from the retail table in the module documentation.
///
/// A pair that retail registers only in the other order gets [`Agent::Symmetric`].
pub(crate) fn convex_pair(a: &Shape, b: &Shape) -> Agent {
    match (a, b) {
        (Shape::Cylinder { .. }, _) | (_, Shape::Cylinder { .. }) => {
            Agent::Gsk(GskAgent::new(a, b))
        }
        (Shape::Box { .. }, Shape::Box { .. }) => Agent::BoxBox(BoxBoxAgent::new()),
        (
            Shape::Sphere { .. },
            Shape::Sphere { .. } | Shape::Capsule { .. } | Shape::Box { .. },
        )
        | (Shape::Capsule { .. }, Shape::Capsule { .. }) => {
            Agent::Primitive(PrimitiveAgent::new(a, b))
        }
        (Shape::Capsule { .. } | Shape::Box { .. }, Shape::Sphere { .. }) => {
            Agent::Symmetric(Box::new(Agent::Primitive(PrimitiveAgent::new(b, a))))
        }
        _ => Agent::Gsk(GskAgent::new(a, b)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Vec3;

    use super::convex_pair;
    use crate::collide::agent::Agent;
    use crate::collide::primitive::PrimitiveAgent;
    use crate::math::Transform;
    use crate::shape::Shape;

    /// The kind of agent that the dispatch selects.
    #[derive(Debug, PartialEq, Eq)]
    enum Kind {
        BoxBox,
        Gsk,
        SphereSphere,
        SphereCapsule,
        SphereBox,
        CapsuleCapsule,
        Symmetric(Box<Kind>),
    }

    fn kind(agent: &Agent) -> Kind {
        match agent {
            Agent::BoxBox(_) => Kind::BoxBox,
            Agent::Gsk(_) => Kind::Gsk,
            Agent::Primitive(PrimitiveAgent::SphereSphere(_)) => Kind::SphereSphere,
            Agent::Primitive(PrimitiveAgent::SphereCapsule(_)) => Kind::SphereCapsule,
            Agent::Primitive(PrimitiveAgent::SphereBox(_)) => Kind::SphereBox,
            Agent::Primitive(PrimitiveAgent::CapsuleCapsule(_)) => Kind::CapsuleCapsule,
            Agent::Symmetric(inner) => Kind::Symmetric(Box::new(kind(inner))),
            other => panic!("unexpected agent {other:?}"),
        }
    }

    /// The HW convex shapes in the table order: S, C, B, Cy, V, T.
    fn shapes() -> [Shape; 6] {
        let sphere = Shape::Sphere { radius: 0.5 };
        [
            sphere.clone(),
            Shape::Capsule {
                a: Vec3::ZERO,
                b: Vec3::Y,
                radius: 0.3,
            },
            Shape::Box {
                half_extents: Vec3::ONE,
                radius: 0.05,
            },
            Shape::Cylinder {
                a: Vec3::ZERO,
                b: Vec3::Y,
                radius: 0.5,
                convex_radius: 0.05,
            },
            Shape::ConvexVertices {
                vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z],
                planes: Vec::new(),
                radius: 0.05,
            },
            Shape::Transform {
                transform: Transform::IDENTITY,
                child: Arc::new(sphere),
            },
        ]
    }

    fn symmetric(inner: Kind) -> Kind {
        Kind::Symmetric(Box::new(inner))
    }

    #[test]
    fn every_hw_convex_pair_gets_the_retail_agent() {
        use Kind::{BoxBox, CapsuleCapsule, Gsk, SphereBox, SphereCapsule, SphereSphere};
        let expected = [
            [SphereSphere, SphereCapsule, SphereBox, Gsk, Gsk, Gsk],
            [symmetric(SphereCapsule), CapsuleCapsule, Gsk, Gsk, Gsk, Gsk],
            [symmetric(SphereBox), Gsk, BoxBox, Gsk, Gsk, Gsk],
            [Gsk, Gsk, Gsk, Gsk, Gsk, Gsk],
            [Gsk, Gsk, Gsk, Gsk, Gsk, Gsk],
            [Gsk, Gsk, Gsk, Gsk, Gsk, Gsk],
        ];
        let shapes = shapes();
        for (row, a) in shapes.iter().enumerate() {
            for (column, b) in shapes.iter().enumerate() {
                let actual = kind(&convex_pair(a, b));
                assert_eq!(actual, expected[row][column], "pair ({row}, {column})");
            }
        }
    }

    #[test]
    fn a_symmetric_primitive_agent_holds_the_registered_order() {
        let shapes = shapes();
        let Agent::Symmetric(inner) = convex_pair(&shapes[2], &shapes[0]) else {
            panic!("box–sphere must be symmetric");
        };
        assert!(matches!(
            *inner,
            Agent::Primitive(PrimitiveAgent::SphereBox(None))
        ));
    }
}
