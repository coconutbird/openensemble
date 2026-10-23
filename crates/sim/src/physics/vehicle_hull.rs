//! The shared convex hull of the vehicles (`BPhysicsWorld+0x10`).
//!
//! `BPhysicsWorld_init` (`0x1407BB390`) builds one hull from 22 vertices for every vehicle
//! except the aircraft. The vertices are in DE at `0x1411F47A0` to `0x1411F4980` and match 2008
//! `xphysics/physicsworld.cpp:150-200`. The shape is an `hkpConvexVerticesShape` with the
//! default convex radius.

use glam::{Vec3, Vec4};

use ::physics::{DEFAULT_CONVEX_RADIUS, Shape};

/// The half size of the base box (`xSize`, `ySize`, `zSize`).
const SIZE: Vec3 = Vec3::new(1.1, 0.25, 1.75);
/// The bumper corners (`xBumper`, `yBumper`, `zBumper`).
const BUMPER: Vec3 = Vec3::new(1.0, 0.15, 1.9);
/// The roof: `xRoof`, `yRoof = ySize + 0.45`, `zRoofFront`, `zRoofBack`.
const ROOF_X: f32 = 0.7;
const ROOF_Y: f32 = 0.7;
const ROOF_FRONT: f32 = 0.4;
const ROOF_BACK: f32 = -1.0;
/// The doors: `xDoor = xSize + 0.1`, `yDoor = ySize`, at the roof front and back.
const DOOR_X: f32 = 1.2;

/// The 22 vertices in retail order (`v0` to `v21`).
#[must_use]
pub(crate) fn vehicle_hull_vertices() -> [Vec3; 22] {
    let (x, y, z) = (SIZE.x, SIZE.y, SIZE.z);
    let (bx, by, bz) = (BUMPER.x, BUMPER.y, BUMPER.z);
    [
        Vec3::new(x, y, z),
        Vec3::new(-x, y, z),
        Vec3::new(x, -y, z),
        Vec3::new(-x, -y, z),
        Vec3::new(x, -y, -z),
        Vec3::new(-x, -y, -z),
        Vec3::new(bx, by, bz),
        Vec3::new(-bx, by, bz),
        Vec3::new(bx, by, -bz),
        Vec3::new(-bx, by, -bz),
        Vec3::new(ROOF_X, ROOF_Y, ROOF_FRONT),
        Vec3::new(-ROOF_X, ROOF_Y, ROOF_FRONT),
        Vec3::new(ROOF_X, ROOF_Y, ROOF_BACK),
        Vec3::new(-ROOF_X, ROOF_Y, ROOF_BACK),
        Vec3::new(DOOR_X, y, ROOF_FRONT),
        Vec3::new(-DOOR_X, y, ROOF_FRONT),
        Vec3::new(DOOR_X, -y, ROOF_FRONT),
        Vec3::new(-DOOR_X, -y, ROOF_FRONT),
        Vec3::new(DOOR_X, y, ROOF_BACK),
        Vec3::new(-DOOR_X, y, ROOF_BACK),
        Vec3::new(DOOR_X, -y, ROOF_BACK),
        Vec3::new(-DOOR_X, -y, ROOF_BACK),
    ]
}

/// The hull shape: the 22 vertices, the face planes of their convex hull
/// (`hkpGeometryUtility_createConvexGeometry`) and the default convex radius.
#[must_use]
pub(crate) fn vehicle_hull_shape() -> Shape {
    let vertices = vehicle_hull_vertices();
    Shape::ConvexVertices {
        planes: hull_planes(&vertices),
        vertices: vertices.to_vec(),
        radius: DEFAULT_CONVEX_RADIUS,
    }
}

/// The outward face planes of the convex hull of `vertices`: each plane through three vertices
/// that has every vertex on its inner side. A plane is `normal.dot(p) + w == 0`.
fn hull_planes(vertices: &[Vec3]) -> Vec<Vec4> {
    const TOLERANCE: f32 = 1e-4;
    let mut planes: Vec<Vec4> = Vec::new();
    for (i, &a) in vertices.iter().enumerate() {
        for (j, &b) in vertices.iter().enumerate().skip(i + 1) {
            for &c in vertices.iter().skip(j + 1) {
                let cross = (b - a).cross(c - a);
                let length = cross.length();
                if length <= TOLERANCE {
                    continue;
                }
                for normal in [cross / length, -cross / length] {
                    let w = -normal.dot(a);
                    let outside = vertices
                        .iter()
                        .any(|vertex| normal.dot(*vertex) + w > TOLERANCE);
                    let known = planes.iter().any(|plane| {
                        plane.truncate().dot(normal) > 1.0 - TOLERANCE
                            && (plane.w - w).abs() <= TOLERANCE
                    });
                    if !outside && !known {
                        planes.push(normal.extend(w));
                    }
                }
            }
        }
    }
    planes
}

#[cfg(test)]
mod tests {
    use super::{hull_planes, vehicle_hull_shape, vehicle_hull_vertices};
    use ::physics::Shape;
    use glam::Vec3;

    #[test]
    fn the_hull_has_the_22_retail_vertices_and_the_default_radius() {
        let Shape::ConvexVertices {
            vertices,
            planes,
            radius,
        } = vehicle_hull_shape()
        else {
            panic!("the vehicle hull is a convex vertices shape");
        };
        assert_eq!(vertices.len(), 22);
        assert_eq!(vertices[10], Vec3::new(0.7, 0.7, 0.4));
        assert_eq!(vertices[21], Vec3::new(-1.2, -0.25, -1.0));
        assert!((radius - 0.05).abs() < f32::EPSILON);
        for plane in &planes {
            for vertex in &vertices {
                assert!(plane.truncate().dot(*vertex) + plane.w <= 1e-4);
            }
        }
        // The bottom face is at y = -0.25 and the roof at y = 0.7.
        assert!(
            planes
                .iter()
                .any(|p| p.truncate() == Vec3::NEG_Y && (p.w + 0.25).abs() < 1e-6)
        );
        assert!(
            planes
                .iter()
                .any(|p| p.truncate() == Vec3::Y && (p.w + 0.7).abs() < 1e-6)
        );
    }

    #[test]
    fn a_cube_has_six_planes() {
        let mut cube = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    cube.push(Vec3::new(x, y, z));
                }
            }
        }
        assert_eq!(hull_planes(&cube).len(), 6);
        assert_eq!(vehicle_hull_vertices().len(), 22);
    }
}
