//! The penetration test of two shapes (`hkpCollisionDispatcher::getPenetrationsFunc`).
//!
//! Retail calls it in `BUnitActionPhysics::update` (2008 `unitactionphysics.cpp:290-320`) to
//! find when a thrown part is outside the body of its parent. The convex functions
//! (`hkpGskBaseAgent::staticGetPenetrations`) report a pair when the GSK surface distance is
//! below zero. A transform shape moves its child, and a list shape tests each child
//! (`hkpTransformAgent`, `hkpListAgent`). A heightfield gives no penetration here: retail only
//! asks this for a part against the body of a unit or a building.

use crate::collide::convex::ConvexRef;
use crate::collide::gsk::GskCache;
use crate::math::Transform;
use crate::shape::Shape;

/// Returns `true` if any convex piece of `a` overlaps any convex piece of `b`.
pub(crate) fn shapes_penetrate(
    a: &Shape,
    world_a: &Transform,
    b: &Shape,
    world_b: &Transform,
) -> bool {
    match a {
        Shape::Transform { transform, child } => {
            return shapes_penetrate(child, &(*world_a * *transform), b, world_b);
        }
        Shape::List { children } => {
            return children
                .iter()
                .any(|child| shapes_penetrate(child, world_a, b, world_b));
        }
        Shape::HeightField(_) => return false,
        _ => {}
    }
    match b {
        Shape::Transform { transform, child } => {
            shapes_penetrate(a, world_a, child, &(*world_b * *transform))
        }
        Shape::List { children } => children
            .iter()
            .any(|child| shapes_penetrate(a, world_a, child, world_b)),
        Shape::HeightField(_) => false,
        _ => convex_penetrate(a, world_a, b, world_b),
    }
}

/// The GSK test of two convex shapes: the surface distance is below zero.
fn convex_penetrate(a: &Shape, world_a: &Transform, b: &Shape, world_b: &Transform) -> bool {
    let convex_a = ConvexRef::Shape(a);
    let convex_b = ConvexRef::Shape(b);
    let b_to_a = world_a.inverse() * *world_b;
    let mut cache = GskCache::new(&convex_a, &convex_b, &b_to_a);
    let (_, distance) = cache.separating_normal(convex_a, convex_b, world_a, world_b);
    distance < 0.0
}

#[cfg(test)]
mod tests {
    use super::shapes_penetrate;
    use crate::math::Transform;
    use crate::shape::Shape;
    use glam::{Mat3, Vec3};
    use std::sync::Arc;

    fn unit_box() -> Shape {
        Shape::Box {
            half_extents: Vec3::splat(0.5),
            radius: 0.05,
        }
    }

    fn at(translation: Vec3) -> Transform {
        Transform {
            rotation: Mat3::IDENTITY,
            translation,
        }
    }

    #[test]
    fn two_boxes_penetrate_only_while_their_surfaces_overlap() {
        let shape = unit_box();
        assert!(shapes_penetrate(
            &shape,
            &at(Vec3::ZERO),
            &shape,
            &at(Vec3::X * 1.0)
        ));
        // The surfaces (core 0.5 plus radius 0.05) touch at a distance of 1.1.
        assert!(!shapes_penetrate(
            &shape,
            &at(Vec3::ZERO),
            &shape,
            &at(Vec3::X * 1.2)
        ));
    }

    #[test]
    fn a_list_penetrates_through_any_moved_child() {
        let child = Arc::new(unit_box());
        let moved = Arc::new(Shape::Transform {
            transform: at(Vec3::Z * 5.0),
            child: Arc::clone(&child),
        });
        let list = Shape::List {
            children: vec![child, moved],
        };
        let other = unit_box();
        assert!(shapes_penetrate(
            &list,
            &at(Vec3::ZERO),
            &other,
            &at(Vec3::Z * 5.5)
        ));
        assert!(!shapes_penetrate(
            &list,
            &at(Vec3::ZERO),
            &other,
            &at(Vec3::Z * 2.5)
        ));
    }
}
