use std::sync::Arc;

use glam::{Mat3, Quat, Vec3, Vec4};

use super::shape_volume_mass_properties;
use crate::math::Transform;
use crate::shape::{DEFAULT_CONVEX_RADIUS, Shape};

fn close(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance * b.abs().max(1.0)
}

fn diagonal(inertia: Mat3) -> Vec3 {
    Vec3::new(inertia.x_axis.x, inertia.y_axis.y, inertia.z_axis.z)
}

fn warthog_box() -> Shape {
    Shape::Box {
        half_extents: Vec3::new(1.5, 1.0, 3.0),
        radius: DEFAULT_CONVEX_RADIUS,
    }
}

#[test]
fn box_inertia_matches_the_retail_warthog_example() {
    let properties = shape_volume_mass_properties(&warthog_box(), 150.0).expect("box has volume");
    let inertia = diagonal(properties.inertia);
    assert!(close(inertia.x, 500.0, 1e-5), "{inertia}");
    assert!(close(inertia.y, 562.5, 1e-5), "{inertia}");
    assert!(close(inertia.z, 162.5, 1e-5), "{inertia}");
    assert!(close(properties.mass, 150.0, 1e-6));
    assert_eq!(properties.center_of_mass, Vec3::ZERO);
}

#[test]
fn a_transform_shape_moves_the_centre_of_mass() {
    let shape = Shape::Transform {
        transform: Transform::from_rotation_translation(Quat::IDENTITY, Vec3::new(0.0, 2.0, 0.0)),
        child: Arc::new(warthog_box()),
    };
    let properties = shape_volume_mass_properties(&shape, 150.0).expect("box has volume");
    assert!((properties.center_of_mass - Vec3::new(0.0, 2.0, 0.0)).length() < 1e-5);
    // The inertia about the centre of mass does not change.
    assert!(close(diagonal(properties.inertia).x, 500.0, 1e-5));
}

#[test]
fn a_list_of_two_spheres_adds_the_parallel_axis_terms() {
    let sphere = |x: f32| {
        Arc::new(Shape::Transform {
            transform: Transform::from_rotation_translation(Quat::IDENTITY, Vec3::new(x, 0.0, 0.0)),
            child: Arc::new(Shape::Sphere { radius: 1.0 }),
        })
    };
    let shape = Shape::List {
        children: vec![sphere(-2.0), sphere(2.0)],
    };
    let properties = shape_volume_mass_properties(&shape, 10.0).expect("spheres have volume");
    let inertia = diagonal(properties.inertia);
    // Each sphere: 0.4·5·1 = 2 about its centre, plus 5·2² about the common centre on Y and Z.
    assert!(close(inertia.x, 4.0, 1e-5), "{inertia}");
    assert!(close(inertia.y, 44.0, 1e-5), "{inertia}");
    assert!(close(inertia.z, 44.0, 1e-5), "{inertia}");
    assert!(properties.center_of_mass.length() < 1e-6);
}

#[test]
fn a_hull_of_cube_vertices_matches_the_box() {
    let mut vertices = Vec::new();
    for x in [-1.0, 1.0] {
        for y in [-2.0, 2.0] {
            for z in [-3.0, 3.0] {
                vertices.push(Vec3::new(x, y, z));
            }
        }
    }
    let planes = vec![
        Vec4::new(1.0, 0.0, 0.0, -1.0),
        Vec4::new(-1.0, 0.0, 0.0, -1.0),
        Vec4::new(0.0, 1.0, 0.0, -2.0),
        Vec4::new(0.0, -1.0, 0.0, -2.0),
        Vec4::new(0.0, 0.0, 1.0, -3.0),
        Vec4::new(0.0, 0.0, -1.0, -3.0),
    ];
    let hull = Shape::ConvexVertices {
        vertices,
        planes,
        radius: DEFAULT_CONVEX_RADIUS,
    };
    let cube = Shape::Box {
        half_extents: Vec3::new(1.0, 2.0, 3.0),
        radius: DEFAULT_CONVEX_RADIUS,
    };
    let from_hull = shape_volume_mass_properties(&hull, 12.0).expect("hull has volume");
    let from_box = shape_volume_mass_properties(&cube, 12.0).expect("box has volume");
    assert!(
        close(from_hull.volume, from_box.volume, 1e-4),
        "{}",
        from_hull.volume
    );
    let (hull_inertia, box_inertia) = (diagonal(from_hull.inertia), diagonal(from_box.inertia));
    for axis in 0..3 {
        assert!(
            close(hull_inertia[axis], box_inertia[axis], 1e-4),
            "{hull_inertia} vs {box_inertia}"
        );
    }
    assert!(from_hull.center_of_mass.length() < 1e-5);
}

#[test]
fn a_capsule_along_y_has_symmetric_inertia_about_its_axis() {
    let capsule = Shape::Capsule {
        a: Vec3::new(0.0, -1.0, 0.0),
        b: Vec3::new(0.0, 1.0, 0.0),
        radius: 0.5,
    };
    let properties = shape_volume_mass_properties(&capsule, 1.0).expect("capsule has volume");
    let inertia = diagonal(properties.inertia);
    assert!(close(inertia.x, inertia.z, 1e-5), "{inertia}");
    assert!(inertia.y < inertia.x, "{inertia}");
    assert!(properties.center_of_mass.length() < 1e-6);
}

#[test]
fn the_heightfield_has_no_mass_properties() {
    let field = crate::heightfield::HeightField::new(2, 1.0, vec![0.0; 4]).expect("valid grid");
    assert!(shape_volume_mass_properties(&Shape::HeightField(Arc::new(field)), 1.0).is_none());
}
