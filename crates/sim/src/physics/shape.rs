//! The mapping from the loaded `.shp` shapes to the shapes of the physics world.
//!
//! The pipeline builds the shapes like `BShape_load` (`0x1407BD0C0`) and already collapses the
//! transforms. Each loaded shape maps to one physics shape.

use std::sync::Arc;

use glam::{Mat3, Vec3};
use pipeline::hw1::physics::{PhysicsShape, ShapeTransform};

use ::physics::{DEFAULT_CONVEX_RADIUS, Shape, Transform};

/// Maps a loaded shape to a physics shape.
#[must_use]
pub(crate) fn physics_shape(shape: &PhysicsShape) -> Shape {
    match shape {
        PhysicsShape::Box {
            half_extents,
            radius,
        } => Shape::Box {
            half_extents: Vec3::from_array(*half_extents),
            radius: *radius,
        },
        PhysicsShape::Sphere { radius } => Shape::Sphere { radius: *radius },
        PhysicsShape::Capsule {
            vertex_a,
            vertex_b,
            radius,
        } => Shape::Capsule {
            a: Vec3::from_array(*vertex_a),
            b: Vec3::from_array(*vertex_b),
            radius: *radius,
        },
        PhysicsShape::Cylinder {
            vertex_a,
            vertex_b,
            cylinder_radius,
            radius,
        } => Shape::Cylinder {
            a: Vec3::from_array(*vertex_a),
            b: Vec3::from_array(*vertex_b),
            radius: *cylinder_radius,
            convex_radius: *radius,
        },
        PhysicsShape::ConvexTranslate { child, translation } => Shape::Transform {
            transform: Transform {
                rotation: Mat3::IDENTITY,
                translation: Vec3::from_array(*translation),
            },
            child: Arc::new(physics_shape(child)),
        },
        PhysicsShape::ConvexTransform { child, transform }
        | PhysicsShape::Transform { child, transform } => Shape::Transform {
            transform: physics_transform(transform),
            child: Arc::new(physics_shape(child)),
        },
        PhysicsShape::List { children } => Shape::List {
            children: children
                .iter()
                .map(|child| Arc::new(physics_shape(child)))
                .collect(),
        },
    }
}

/// The box of a blueprint without a shape (`hkpBoxShape(halfExtents, 0.05)` in
/// `BPhysicsObject_ctorFromBlueprint`, `0x1407B5C70`).
#[must_use]
pub(crate) fn blueprint_box(half_extents: Vec3) -> Shape {
    Shape::Box {
        half_extents,
        radius: DEFAULT_CONVEX_RADIUS,
    }
}

/// Half the size of the box around a shape in its own space, without the convex radius. The
/// gameplay code uses it as the size of the body.
#[must_use]
pub(crate) fn shape_half_extents(shape: &Shape) -> Vec3 {
    let (min, max) = shape_bounds(shape);
    (max - min) * 0.5
}

fn shape_bounds(shape: &Shape) -> (Vec3, Vec3) {
    match shape {
        Shape::Box { half_extents, .. } => (-*half_extents, *half_extents),
        Shape::Sphere { radius } => (Vec3::splat(-*radius), Vec3::splat(*radius)),
        Shape::Capsule { a, b, radius } | Shape::Cylinder { a, b, radius, .. } => (
            a.min(*b) - Vec3::splat(*radius),
            a.max(*b) + Vec3::splat(*radius),
        ),
        Shape::ConvexVertices { vertices, .. } => vertices.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(min, max), vertex| (min.min(*vertex), max.max(*vertex)),
        ),
        Shape::Transform { transform, child } => {
            let (min, max) = shape_bounds(child);
            let center = transform.transform_point((min + max) * 0.5);
            let half = (max - min) * 0.5;
            let rotation = transform.rotation;
            let extent = Vec3::new(
                rotation.row(0).abs().dot(half),
                rotation.row(1).abs().dot(half),
                rotation.row(2).abs().dot(half),
            );
            (center - extent, center + extent)
        }
        Shape::List { children } => children.iter().map(|child| shape_bounds(child)).fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(min, max), (child_min, child_max)| (min.min(child_min), max.max(child_max)),
        ),
        Shape::HeightField(_) => (Vec3::ZERO, Vec3::ZERO),
    }
}

fn physics_transform(transform: &ShapeTransform) -> Transform {
    let [c0, c1, c2] = transform.rotation;
    Transform {
        rotation: Mat3::from_cols(
            Vec3::from_array(c0),
            Vec3::from_array(c1),
            Vec3::from_array(c2),
        ),
        translation: Vec3::from_array(transform.translation),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Vec3;
    use pipeline::hw1::physics::{PhysicsShape, ShapeTransform};

    use super::{physics_shape, shape_half_extents};
    use ::physics::Shape;

    #[test]
    fn each_loaded_shape_maps_to_the_same_physics_shape() {
        let cylinder = PhysicsShape::Cylinder {
            vertex_a: [0.0, -1.0, 0.0],
            vertex_b: [0.0, 1.0, 0.0],
            cylinder_radius: 0.5,
            radius: 0.05,
        };
        assert_eq!(
            physics_shape(&cylinder),
            Shape::Cylinder {
                a: Vec3::NEG_Y,
                b: Vec3::Y,
                radius: 0.5,
                convex_radius: 0.05,
            }
        );
        let moved = PhysicsShape::Transform {
            child: Arc::new(PhysicsShape::Box {
                half_extents: [1.0, 2.0, 3.0],
                radius: 0.05,
            }),
            transform: ShapeTransform {
                translation: [0.0, 5.0, 0.0],
                ..ShapeTransform::IDENTITY
            },
        };
        let Shape::Transform { transform, child } = physics_shape(&moved) else {
            panic!("a transform maps to a transform");
        };
        assert_eq!(transform.translation, Vec3::new(0.0, 5.0, 0.0));
        assert_eq!(
            child.as_ref(),
            &Shape::Box {
                half_extents: Vec3::new(1.0, 2.0, 3.0),
                radius: 0.05,
            }
        );
    }

    #[test]
    fn half_extents_cover_every_child() {
        let list = PhysicsShape::List {
            children: vec![
                Arc::new(PhysicsShape::Sphere { radius: 1.0 }),
                Arc::new(PhysicsShape::ConvexTranslate {
                    child: Arc::new(PhysicsShape::Sphere { radius: 1.0 }),
                    translation: [4.0, 0.0, 0.0],
                }),
            ],
        };
        let extents = shape_half_extents(&physics_shape(&list));
        assert!(
            (extents - Vec3::new(3.0, 1.0, 1.0)).length() < 1e-6,
            "{extents}"
        );
    }
}
