//! Tests of the collision world: terrain contacts, the broadphase rules and the filters.

use std::sync::Arc;

use glam::{Quat, Vec3};

use super::{CollidablePose, CollisionWorld};
use crate::body::BodyId;
use crate::contact::Manifold;
use crate::filter::{FilterInfo, GroupFilter, Layer};
use crate::heightfield::HeightField;
use crate::math::Transform;
use crate::shape::{DEFAULT_CONVEX_RADIUS, Shape};

const TOLERANCE: f32 = 0.1;
const TERRAIN: BodyId = BodyId::from_raw(0);

fn flat_field(height: f32) -> HeightField {
    HeightField::new(8, 2.0, vec![height; 64]).expect("valid grid")
}

fn terrain_world(field: HeightField) -> CollisionWorld {
    let mut world = CollisionWorld::new(GroupFilter::retail(), TOLERANCE);
    let group = world.filter_mut().new_system_group();
    world.insert(
        TERRAIN,
        Arc::new(Shape::HeightField(Arc::new(field))),
        FilterInfo::for_layer(Layer::Terrain, group),
        true,
    );
    world
}

fn object_filter(world: &mut CollisionWorld) -> FilterInfo {
    let group = world.filter_mut().new_system_group();
    FilterInfo::for_layer(Layer::Objects, group)
}

fn pose(body: BodyId, translation: Vec3, active: bool) -> CollidablePose {
    CollidablePose {
        body,
        transform: Transform::from_rotation_translation(Quat::IDENTITY, translation),
        active,
    }
}

fn terrain_pose(world: &CollisionWorld) -> CollidablePose {
    let (field, transform) = world.terrain().expect("terrain");
    let _ = field;
    CollidablePose {
        body: TERRAIN,
        transform,
        active: false,
    }
}

fn unit_box() -> Arc<Shape> {
    Arc::new(Shape::Box {
        half_extents: Vec3::new(1.0, 0.5, 1.0),
        radius: DEFAULT_CONVEX_RADIUS,
    })
}

fn manifold_of(world: &CollisionWorld, a: BodyId, b: BodyId) -> Option<&Manifold> {
    world
        .manifolds()
        .iter()
        .find(|manifold| manifold.body_a == a && manifold.body_b == b)
}

#[test]
fn a_box_at_rest_on_the_terrain_touches_with_its_corner_spheres() {
    let mut world = terrain_world(flat_field(2.0));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    world.insert(body, unit_box(), filter, false);
    let rest = Vec3::new(5.0, 2.0 + 0.5 + DEFAULT_CONVEX_RADIUS, 5.0);
    world.collide(&[terrain_pose(&world), pose(body, rest, true)]);

    // Only the four lower corner spheres are within the tolerance.
    let manifold = manifold_of(&world, TERRAIN, body).expect("terrain manifold");
    assert_eq!(manifold.points.len(), 4);
    for point in &manifold.points {
        // The terrain is body A, so the normal points down toward the terrain.
        assert!((point.contact.normal - Vec3::NEG_Y).length() < 1e-6);
        assert!(
            point.contact.distance.abs() < 1e-5,
            "{}",
            point.contact.distance
        );
        // The manifold position is on the terrain surface, below the corner sphere.
        assert!((point.contact.position.y - 2.0).abs() < 1e-5);
        assert!(point.properties.is_new);
    }
    // The box face, without the convex radius, floats one convex radius above the terrain.
    let face = rest.y - 0.5;
    assert!((face - 2.0 - DEFAULT_CONVEX_RADIUS).abs() < 1e-6);

    // A plate thinner than the tolerance touches with all eight corner spheres.
    let plate = BodyId::from_raw(2);
    let filter = object_filter(&mut world);
    let thin = Arc::new(Shape::Box {
        half_extents: Vec3::new(1.0, 0.02, 1.0),
        radius: DEFAULT_CONVEX_RADIUS,
    });
    world.insert(plate, thin, filter, false);
    world.collide(&[
        terrain_pose(&world),
        pose(body, rest, true),
        pose(plate, Vec3::new(12.0, 2.07, 12.0), true),
    ]);
    let manifold = manifold_of(&world, TERRAIN, plate).expect("plate manifold");
    let distances: Vec<f32> = manifold
        .points
        .iter()
        .map(|point| point.contact.distance)
        .collect();
    assert_eq!(distances.len(), 8);
    // The sphere order of retail: Y is positive for corners 0, 1, 4 and 5.
    for (index, distance) in distances.iter().enumerate() {
        let expected = if index & 2 == 0 { 0.04 } else { 0.0 };
        assert!((distance - expected).abs() < 1e-5, "{distances:?}");
    }
}

#[test]
fn terrain_points_keep_their_keys_and_leave_beyond_the_tolerance() {
    // A far vertex at height 1 keeps the terrain box high enough for the lifted box.
    let mut heights = vec![0.0; 64];
    heights[63] = 1.0;
    let mut world = terrain_world(HeightField::new(8, 2.0, heights).expect("grid"));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    world.insert(body, unit_box(), filter, false);
    let terrain = terrain_pose(&world);
    world.collide(&[terrain, pose(body, Vec3::new(5.0, 0.6, 5.0), true)]);
    let keys: Vec<u32> = manifold_of(&world, TERRAIN, body)
        .expect("manifold")
        .points
        .iter()
        .map(|point| point.key)
        .collect();
    assert_eq!(keys, [0, 1, 2, 3]);

    // Tilt the box: the four upper corners do not touch, the lower corners keep their keys.
    let tilt = Transform::from_rotation_translation(
        Quat::from_rotation_z(0.3),
        Vec3::new(5.0, 0.6 + 1.0 * 0.3_f32.sin(), 5.0),
    );
    world.collide(&[
        terrain,
        CollidablePose {
            body,
            transform: tilt,
            active: true,
        },
    ]);
    let manifold = manifold_of(&world, TERRAIN, body).expect("manifold");
    let kept: Vec<u32> = manifold.points.iter().map(|point| point.key).collect();
    assert!(!kept.is_empty() && kept.len() < 8, "{kept:?}");
    assert!(kept.iter().all(|key| *key < 8), "{kept:?}");
    assert!(kept.windows(2).all(|pair| pair[0] < pair[1]));

    // Lift the box beyond the tolerance: every point goes, the manifold stays while the boxes
    // still overlap.
    world.collide(&[terrain, pose(body, Vec3::new(5.0, 0.6 + 0.2, 5.0), true)]);
    let manifold = manifold_of(&world, TERRAIN, body).expect("manifold");
    assert_eq!(manifold.points, []);
}

#[test]
fn a_capsule_on_the_terrain_has_two_points() {
    let mut world = terrain_world(flat_field(1.0));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    let capsule = Arc::new(Shape::Capsule {
        a: Vec3::new(-1.0, 0.0, 0.0),
        b: Vec3::new(1.0, 0.0, 0.0),
        radius: 0.3,
    });
    world.insert(body, capsule, filter, false);
    world.collide(&[
        terrain_pose(&world),
        pose(body, Vec3::new(4.0, 1.3, 4.0), true),
    ]);
    let manifold = manifold_of(&world, TERRAIN, body).expect("manifold");
    assert_eq!(manifold.points.len(), 2);
    assert!(
        manifold
            .points
            .iter()
            .all(|point| point.contact.distance.abs() < 1e-5)
    );
}

#[test]
fn a_peak_between_two_box_corners_makes_no_contact() {
    // A flat grid at 0 with one tall vertex under the middle of the box.
    let mut heights = vec![0.0; 64];
    heights[3 * 8 + 3] = 3.0;
    let mut world = terrain_world(HeightField::new(8, 2.0, heights).expect("grid"));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    let wide = Arc::new(Shape::Box {
        half_extents: Vec3::new(3.0, 0.5, 3.0),
        radius: DEFAULT_CONVEX_RADIUS,
    });
    world.insert(body, wide, filter, false);
    // The box bottom is at 1.0, so the peak at 3.0 pierces it. The corners are far from the
    // peak, over flat terrain 0.95 below them.
    world.collide(&[
        terrain_pose(&world),
        pose(body, Vec3::new(6.0, 1.5, 6.0), true),
    ]);
    let manifold = manifold_of(&world, TERRAIN, body).expect("the boxes overlap");
    assert_eq!(manifold.points, []);
}

#[test]
fn fixed_pairs_and_filtered_pairs_get_no_manifold() {
    let mut world = terrain_world(flat_field(0.0));
    let terrain = terrain_pose(&world);

    // A fixed building on the terrain: fixed against fixed.
    let building = BodyId::from_raw(1);
    let group = world.filter_mut().new_system_group();
    world.insert(
        building,
        unit_box(),
        FilterInfo::for_layer(Layer::Buildings, group),
        true,
    );
    // A vehicle that does not collide with the terrain.
    let hover = BodyId::from_raw(2);
    let group = world.filter_mut().new_system_group();
    world.insert(
        hover,
        unit_box(),
        FilterInfo::for_layer(Layer::NoTerrainVehicles, group),
        false,
    );
    // Two vehicles in one system group.
    let vehicles = world.filter_mut().new_system_group();
    let first = BodyId::from_raw(3);
    let second = BodyId::from_raw(4);
    for body in [first, second] {
        world.insert(
            body,
            unit_box(),
            FilterInfo::for_layer(Layer::Vehicles, vehicles),
            false,
        );
    }
    world.collide(&[
        terrain,
        pose(building, Vec3::new(3.0, 0.5, 3.0), false),
        pose(hover, Vec3::new(8.0, 0.5, 3.0), true),
        pose(first, Vec3::new(3.0, 0.5, 9.0), true),
        pose(second, Vec3::new(4.0, 0.5, 9.0), true),
    ]);
    assert!(manifold_of(&world, TERRAIN, building).is_none());
    assert!(manifold_of(&world, TERRAIN, hover).is_none());
    assert!(manifold_of(&world, first, second).is_none());
    assert!(manifold_of(&world, TERRAIN, first).is_some());
    assert!(manifold_of(&world, TERRAIN, second).is_some());

    // A new filter that disables the terrain drops the manifold at once.
    let group = world.filter_mut().new_system_group();
    world.set_filter_info(
        first,
        FilterInfo::for_layer(Layer::NoTerrainVehicles, group),
    );
    assert!(manifold_of(&world, TERRAIN, first).is_none());
}

#[test]
fn sleeping_pairs_keep_their_points() {
    let mut world = terrain_world(flat_field(0.0));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    world.insert(body, unit_box(), filter, false);
    let terrain = terrain_pose(&world);
    world.collide(&[terrain, pose(body, Vec3::new(5.0, 0.55, 5.0), true)]);
    let before = manifold_of(&world, TERRAIN, body)
        .expect("manifold")
        .clone();
    assert_eq!(before.points.len(), 4);

    // An inactive body is not processed, even if the caller gives a new transform.
    world.collide(&[terrain, pose(body, Vec3::new(5.0, 0.58, 5.0), false)]);
    let after = manifold_of(&world, TERRAIN, body).expect("manifold");
    assert_eq!(after, &before);
}

#[test]
fn the_terrain_box_keeps_the_height_range_of_the_insertion() {
    let mut world = terrain_world(flat_field(0.0));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    world.insert(body, unit_box(), filter, false);
    // Raise all heights after the insertion, like `flattenInstant`.
    let field = world.heightfield_mut().expect("terrain");
    for z in 0..8 {
        for x in 0..8 {
            assert!(field.set_sim_height(x, z, 5.0));
        }
    }
    // The box rests on the new surface, but the terrain box still ends at 0 + tolerance / 2.
    world.collide(&[
        terrain_pose(&world),
        pose(body, Vec3::new(5.0, 5.55, 5.0), true),
    ]);
    assert!(manifold_of(&world, TERRAIN, body).is_none());

    // Inside the old range, the agent sees the new heights: the corners are deep below them.
    world.collide(&[
        terrain_pose(&world),
        pose(body, Vec3::new(5.0, 0.55, 5.0), true),
    ]);
    let manifold = manifold_of(&world, TERRAIN, body).expect("manifold");
    // The lower corners are 5 below the surface and the upper corners 4.
    assert_eq!(manifold.points.len(), 8);
    for point in &manifold.points {
        let distance = point.contact.distance;
        assert!(
            (distance + 5.0).abs() < 1e-5 || (distance + 4.0).abs() < 1e-5,
            "{distance}"
        );
    }
}

#[test]
fn removing_a_body_removes_its_manifolds() {
    let mut world = terrain_world(flat_field(0.0));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    world.insert(body, unit_box(), filter, false);
    world.collide(&[
        terrain_pose(&world),
        pose(body, Vec3::new(5.0, 0.55, 5.0), true),
    ]);
    assert_eq!(world.manifolds().len(), 1);
    world.remove(body);
    assert_eq!(world.manifolds(), []);
}

#[test]
fn a_point_that_a_listener_removed_comes_back_with_a_new_key() {
    let mut world = terrain_world(flat_field(0.0));
    let body = BodyId::from_raw(1);
    let filter = object_filter(&mut world);
    assert!(!world.filter().is_collision_enabled(filter, filter));
    world.insert(body, unit_box(), filter, false);
    let terrain = terrain_pose(&world);
    let rest = pose(body, Vec3::new(5.0, 0.55, 5.0), true);
    world.collide(&[terrain, rest]);
    let manifold = &mut world.manifolds_mut()[0];
    manifold.points.remove(0);
    for point in &mut manifold.points {
        point.properties.is_new = false;
    }
    world.collide(&[terrain, rest]);
    let manifold = &world.manifolds()[0];
    let keys: Vec<u32> = manifold.points.iter().map(|point| point.key).collect();
    assert_eq!(keys, [1, 2, 3, 4]);
    assert!(manifold.points[3].properties.is_new);
    assert!(!manifold.points[0].properties.is_new);
}

/// The key, the geometry bits and the new flag of one point.
type PointBits = (u32, [u32; 7], bool);

/// The id, the two bodies and the points of one manifold.
type ManifoldBits = (u64, u32, u32, Vec<PointBits>);

/// Runs a scene with every agent kind for 40 steps and returns the bits of all manifolds.
fn run_scene() -> Vec<ManifoldBits> {
    let heights = (0..64u16)
        .map(|index| f32::from(index % 8) * 0.25 + f32::from(index / 8 % 3) * 0.5)
        .collect();
    let mut world = terrain_world(HeightField::new(8, 2.0, heights).expect("grid"));
    let block = Arc::new(Shape::Box {
        half_extents: Vec3::new(0.8, 0.4, 0.6),
        radius: DEFAULT_CONVEX_RADIUS,
    });
    let shapes = [
        unit_box(),
        Arc::clone(&block),
        Arc::new(Shape::Sphere { radius: 0.5 }),
        Arc::new(Shape::Capsule {
            a: Vec3::new(0.0, -0.5, 0.0),
            b: Vec3::new(0.0, 0.5, 0.0),
            radius: 0.3,
        }),
        Arc::new(Shape::Cylinder {
            a: Vec3::new(-0.5, 0.0, 0.0),
            b: Vec3::new(0.5, 0.0, 0.0),
            radius: 0.4,
            convex_radius: DEFAULT_CONVEX_RADIUS,
        }),
        Arc::new(Shape::List {
            children: vec![
                Arc::new(Shape::Transform {
                    transform: Transform::from_rotation_translation(
                        Quat::from_rotation_y(0.4),
                        Vec3::new(0.0, 0.5, 0.0),
                    ),
                    child: Arc::clone(&block),
                }),
                Arc::new(Shape::Sphere { radius: 0.6 }),
            ],
        }),
    ];
    let bodies: Vec<BodyId> = (1..=6).map(BodyId::from_raw).collect();
    for (body, shape) in bodies.iter().zip(&shapes) {
        let filter = object_filter(&mut world);
        let filter = FilterInfo::for_layer(Layer::Zero, filter.system_group());
        world.insert(*body, Arc::clone(shape), filter, false);
    }
    let terrain = terrain_pose(&world);
    let mut frames = Vec::new();
    for step in 0..40u16 {
        let time = f32::from(step) * 0.05;
        let mut poses = vec![terrain];
        for (index, body) in (0..6u16).zip(&bodies) {
            let phase = f32::from(index);
            let center = Vec3::new(
                4.0 + phase * 0.9 + time * 0.3,
                1.4 - time * 0.2 + (phase * 0.7).sin() * 0.3,
                5.0 + (time + phase).cos() * 0.8,
            );
            let rotation = Quat::from_euler(glam::EulerRot::YXZ, time + phase, 0.2 * phase, 0.1);
            poses.push(CollidablePose {
                body: *body,
                transform: Transform::from_rotation_translation(rotation, center),
                active: true,
            });
        }
        world.collide(&poses);
        frames.extend(world.manifolds().iter().map(|manifold| {
            let points = manifold
                .points
                .iter()
                .map(|point| {
                    let contact = point.contact;
                    let bits = [
                        contact.position.x.to_bits(),
                        contact.position.y.to_bits(),
                        contact.position.z.to_bits(),
                        contact.normal.x.to_bits(),
                        contact.normal.y.to_bits(),
                        contact.normal.z.to_bits(),
                        contact.distance.to_bits(),
                    ];
                    (point.key, bits, point.properties.is_new)
                })
                .collect();
            (
                manifold.id.raw(),
                manifold.body_a.raw(),
                manifold.body_b.raw(),
                points,
            )
        }));
    }
    frames
}

#[test]
fn the_same_scene_gives_bit_identical_manifolds() {
    let first = run_scene();
    let second = run_scene();
    assert!(first.iter().any(|(_, _, _, points)| !points.is_empty()));
    assert_eq!(first, second);
}

#[test]
fn the_ray_hits_the_flipped_diagonal_triangle_at_its_height() {
    // Cell (0, 0) has h00 = 0, h10 = 2, h01 = 6 and h11 = 4. With the diagonal from (0, 0) to
    // (1, 1), the point (0.75, 0.15) of the cell lies on the triangle h00–h10–h11 at height
    // 0.15 · 2 + 0.75 · 2 = 1.8. The other diagonal would give 2.4.
    let mut heights = vec![0.0; 9];
    heights[1] = 2.0;
    heights[3] = 6.0;
    heights[4] = 4.0;
    let world = terrain_world(HeightField::new(3, 2.0, heights).expect("grid"));
    let from = Vec3::new(1.5, 10.0, 0.3);
    let to = Vec3::new(1.5, -10.0, 0.3);
    let hit = world.ray_cast_heightfield(from, to).expect("hit");
    assert!((hit.position.y - 1.8).abs() < 1e-5, "{hit:?}");
    assert!((hit.fraction - 0.41).abs() < 1e-5, "{hit:?}");
    let normal = Vec3::new(-1.0, 1.0, -1.0).normalize();
    assert!((hit.normal - normal).length() < 1e-5, "{hit:?}");

    let field = world.terrain().expect("terrain").0;
    let (height, surface_normal) = field.surface_at(1.5, 0.3).expect("inside");
    assert!((height - 1.8).abs() < 1e-5);
    assert!((surface_normal - normal).length() < 1e-5);
}
