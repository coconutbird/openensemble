//! Scenes through the real collision detection.

use std::sync::Arc;

use glam::{Quat, Vec3};

use super::{
    ContactAdded, ContactListener, ContactResponse, LimitedHingeDesc, MAX_SUBSTEP_SECONDS, World,
    WorldConfig,
};
use crate::body::{BodyDesc, BodyId, MotionType};
use crate::filter::{FilterInfo, Layer};
use crate::heightfield::HeightField;
use crate::shape::{DEFAULT_CONVEX_RADIUS, Shape};

const DT: f32 = MAX_SUBSTEP_SECONDS;

/// Adds a terrain body with retail material to the world.
fn add_terrain(world: &mut World, size: usize, height: impl Fn(usize, usize) -> f32) -> BodyId {
    let heights = (0..size * size)
        .map(|index| height(index % size, index / size))
        .collect();
    let field = HeightField::new(size, 1.0, heights).expect("valid terrain");
    let position = field.body_position();
    let group = world.filter_mut().new_system_group();
    let mut desc = BodyDesc::new(
        Arc::new(Shape::HeightField(Arc::new(field))),
        MotionType::Fixed,
        position,
    );
    desc.friction = 1.9;
    desc.restitution = 1.0;
    desc.filter = FilterInfo::for_layer(Layer::Terrain, group);
    world.add_body(desc)
}

/// A dynamic body description with its own system group in the object layer.
fn dynamic(world: &mut World, shape: Shape, position: Vec3, mass: f32) -> BodyDesc {
    let group = world.filter_mut().new_system_group();
    let mut desc = BodyDesc::new(Arc::new(shape), MotionType::BoxInertia, position);
    desc.mass = mass;
    desc.friction = 0.5;
    desc.restitution = 0.0;
    desc.linear_damping = 0.1;
    desc.angular_damping = 0.1;
    desc.filter = FilterInfo::for_layer(Layer::Objects, group);
    desc
}

fn crate_box() -> Shape {
    Shape::Box {
        half_extents: Vec3::new(1.0, 0.5, 1.0),
        radius: DEFAULT_CONVEX_RADIUS,
    }
}

fn step(world: &mut World, steps: usize) {
    for _ in 0..steps {
        world.step_delta_time(DT, &mut ());
    }
}

#[test]
fn substeps_follow_the_retail_rule() {
    let close = |(count, length): (u32, f32), expected: (u32, f32)| {
        count == expected.0 && (length - expected.1).abs() < 1e-6
    };
    assert!(close(World::substeps(0.05), (4, 0.0125)));
    assert!(close(World::substeps(0.2), (16, 0.0125)));
    assert!(close(World::substeps(1.0 / 30.0), (3, 0.011_111_111)));
    assert!(close(World::substeps(f32::NAN), (1, 0.0125)));
    assert!(close(World::substeps(11.0), (1, 0.0125)));
    assert_eq!(World::substeps(0.0).0, 0);
}

#[test]
fn a_dropped_box_rests_at_the_convex_radius_gap_and_then_sleeps() {
    let mut world = World::new(WorldConfig::default());
    add_terrain(&mut world, 16, |_, _| 2.0);
    let desc = dynamic(&mut world, crate_box(), Vec3::new(8.0, 4.0, 8.0), 100.0);
    let body = world.add_body(desc);

    let mut first_sleep = None;
    let mut landed = None;
    for frame in 0..800 {
        world.step_delta_time(DT, &mut ());
        let speed = world.linear_velocity(body).expect("body").length();
        if landed.is_none() && world.manifolds_of(body).any(|m| !m.points.is_empty()) {
            landed = Some(frame);
        }
        if first_sleep.is_none() && !world.is_active(body).expect("body") {
            first_sleep = Some(frame);
            assert!(speed < 0.05, "sleeps only when slow: {speed}");
        }
    }
    let landed = landed.expect("the box reaches the terrain");
    let first_sleep = first_sleep.expect("the box goes to sleep");
    // Six passed high-frequency checks need at least 24 frames after the box stops moving.
    assert!(
        first_sleep > landed + 24,
        "landed {landed}, slept {first_sleep}"
    );
    assert!(
        !world.is_active(body).expect("body"),
        "the box stays asleep"
    );

    let bottom = world.position(body).expect("body").y - 0.5;
    let gap = bottom - 2.0;
    assert!((gap - DEFAULT_CONVEX_RADIUS).abs() < 0.01, "gap {gap}");
    let up = world.rotation(body).expect("body") * Vec3::Y;
    assert!(up.y > 0.999, "the box stays level: {up}");
}

/// Drops a sphere and returns the highest centre height after the first bounce.
fn bounce_height(restitution: f32) -> f32 {
    let mut world = World::new(WorldConfig::default());
    add_terrain(&mut world, 16, |_, _| 0.0);
    let mut desc = dynamic(
        &mut world,
        Shape::Sphere { radius: 0.5 },
        Vec3::new(8.0, 5.5, 8.0),
        10.0,
    );
    desc.restitution = restitution;
    desc.linear_damping = 0.0;
    let body = world.add_body(desc);
    let mut bounced = false;
    let mut peak = f32::MIN;
    for _ in 0..300 {
        world.step_delta_time(DT, &mut ());
        let velocity = world.linear_velocity(body).expect("body");
        let height = world.position(body).expect("body").y;
        if velocity.y > 0.0 {
            bounced = true;
        }
        if bounced {
            peak = peak.max(height);
        }
    }
    peak - 0.5
}

#[test]
fn restitution_makes_the_body_bounce_back() {
    let elastic = bounce_height(1.0);
    let dead = bounce_height(0.0);
    assert!(
        elastic > 1.0,
        "a restitution of 1 bounces from 5: {elastic}"
    );
    assert!(elastic < 5.0, "no energy appears: {elastic}");
    assert!(dead < 0.3, "no restitution gives almost no bounce: {dead}");
}

/// Slides a box down a slope and returns its speed after `seconds`.
fn slope_speed(friction: f32, steps: usize) -> f32 {
    let mut world = World::new(WorldConfig::default());
    // A slope of 0.25 along X (14°).
    #[expect(clippy::cast_precision_loss, reason = "small grid indices")]
    add_terrain(&mut world, 24, |x, _| 10.0 - x as f32 * 0.25);
    let mut desc = dynamic(&mut world, crate_box(), Vec3::new(6.0, 9.0, 12.0), 100.0);
    desc.friction = friction;
    desc.rotation = Quat::from_rotation_z(-0.25_f32.atan());
    let body = world.add_body(desc);
    world.set_linear_velocity(body, Vec3::new(2.0, -0.5, 0.0));
    step(&mut world, steps);
    world.linear_velocity(body).expect("body").length()
}

#[test]
fn friction_stops_a_box_sliding_down_a_slope() {
    let rough = slope_speed(1.0, 240);
    let slick = slope_speed(0.0, 240);
    assert!(rough < 0.05, "the rough box stops: {rough}");
    assert!(slick > 2.0, "the frictionless box keeps sliding: {slick}");
}

#[test]
fn the_hinge_stays_inside_its_limits() {
    let mut world = World::new(WorldConfig::default());
    // A bar hinged at its end. It starts horizontal, so gravity swings it down about Z.
    let desc = dynamic(
        &mut world,
        Shape::Box {
            half_extents: Vec3::new(0.75, 0.1, 0.1),
            radius: DEFAULT_CONVEX_RADIUS,
        },
        Vec3::new(0.75, 10.0, 0.0),
        90.0,
    );
    // The anchor shares the system group of the bar, so the two never collide.
    let mut anchor_desc = BodyDesc::new(
        Arc::new(Shape::Sphere { radius: 0.2 }),
        MotionType::Fixed,
        Vec3::new(0.0, 10.0, 0.0),
    );
    anchor_desc.filter = desc.filter;
    let anchor = world.add_body(anchor_desc);
    let arm = world.add_body(desc);
    let (min, max) = (-std::f32::consts::PI / 8.0, std::f32::consts::PI / 4.0);
    let hinge = world
        .add_limited_hinge(&LimitedHingeDesc {
            body_a: anchor,
            body_b: arm,
            pivot: Vec3::new(0.0, 10.0, 0.0),
            axis: Vec3::Z,
            min_angle: min,
            max_angle: max,
            max_friction_torque: 3.0,
        })
        .expect("both bodies exist");
    let mut extreme = 0.0_f32;
    for _ in 0..400 {
        world.step_delta_time(DT, &mut ());
        let angle = world.hinge_angle(hinge).expect("hinge");
        assert!(angle > min - 0.06 && angle < max + 0.06, "angle {angle}");
        extreme = extreme.max(angle.abs());
        let distance = (world.position(arm).expect("arm") - Vec3::new(0.0, 10.0, 0.0)).length();
        assert!(
            (distance - 0.75).abs() < 0.05,
            "the pivot holds: {distance}"
        );
    }
    assert!(extreme > 0.3, "the arm reaches a limit: {extreme}");
}

/// Scales the inverse mass of one body to zero in all its contacts.
struct Immovable(BodyId);

impl ContactListener for Immovable {
    fn contact_point_added(&mut self, event: &ContactAdded) -> ContactResponse {
        let scale = if event.body_a == self.0 {
            [0.0, 1.0]
        } else {
            [1.0, 0.0]
        };
        ContactResponse {
            accept: true,
            inv_mass_scale: Some(scale),
        }
    }
}

#[test]
fn an_inverse_mass_scale_of_zero_makes_the_body_ignore_the_contact() {
    let run = |immovable: bool| {
        let mut world = World::new(WorldConfig::default());
        add_terrain(&mut world, 16, |_, _| 0.0);
        let desc = dynamic(&mut world, crate_box(), Vec3::new(8.0, 1.0, 8.0), 100.0);
        let body = world.add_body(desc);
        let mut listener = Immovable(body);
        for _ in 0..120 {
            if immovable {
                world.step_delta_time(DT, &mut listener);
            } else {
                world.step_delta_time(DT, &mut ());
            }
        }
        world.position(body).expect("body").y
    };
    let resting = run(false);
    let sunk = run(true);
    assert!(
        (resting - 0.55).abs() < 0.01,
        "the normal box rests: {resting}"
    );
    assert!(sunk < -1.0, "the scaled box falls through: {sunk}");
}

#[test]
fn impulses_skip_keyframed_bodies_and_zero_bad_input() {
    let mut world = World::new(WorldConfig::default());
    let desc = dynamic(&mut world, crate_box(), Vec3::ZERO, 10.0);
    let body = world.add_body(desc);
    world.apply_linear_impulse(body, Vec3::new(f32::NAN, 1.0, 0.0));
    assert_eq!(world.linear_velocity(body), Some(Vec3::ZERO));
    world.apply_point_impulse(
        body,
        Vec3::new(0.0, 10.0, 0.0),
        Vec3::new(f32::INFINITY, 0.0, 0.0),
    );
    // The point became the origin, where the centre of mass is: no spin.
    assert_eq!(world.angular_velocity(body), Some(Vec3::ZERO));
    assert_eq!(world.linear_velocity(body), Some(Vec3::new(0.0, 1.0, 0.0)));
    world.set_motion_type(body, MotionType::Keyframed);
    world.apply_linear_impulse(body, Vec3::new(5.0, 0.0, 0.0));
    assert_eq!(world.linear_velocity(body), Some(Vec3::new(0.0, 1.0, 0.0)));
}

#[test]
fn the_broadphase_border_fixes_bodies_that_reach_it() {
    let mut world = World::new(WorldConfig::default());
    let outside = dynamic(&mut world, crate_box(), Vec3::new(10_000.0, 0.0, 0.0), 10.0);
    let outside = world.add_body(outside);
    assert_eq!(world.motion_type(outside), Some(MotionType::Fixed));
    assert_eq!(world.is_active(outside), Some(false));

    let falling = dynamic(&mut world, crate_box(), Vec3::new(0.0, -1_400.0, 0.0), 10.0);
    let falling = world.add_body(falling);
    world.set_linear_velocity(falling, Vec3::new(0.0, -100.0, 0.0));
    world.step_delta_time(DT, &mut ());
    assert_eq!(world.motion_type(falling), Some(MotionType::BoxInertia));
    for _ in 0..40 {
        world.step_delta_time(DT, &mut ());
    }
    assert_eq!(world.motion_type(falling), Some(MotionType::Fixed));
    assert_eq!(world.linear_velocity(falling), Some(Vec3::ZERO));
    assert_eq!(world.is_active(falling), Some(false));
}

/// Builds a busy scene: bumpy terrain, mixed shapes, a hinge and a restitution body.
fn busy_scene() -> (World, Vec<BodyId>) {
    let mut world = World::new(WorldConfig::default());
    #[expect(clippy::cast_precision_loss, reason = "small grid indices")]
    add_terrain(&mut world, 24, |x, z| {
        1.0 + ((x * 7 + z * 13) % 5) as f32 * 0.15 + x as f32 * 0.05
    });
    let mut bodies = Vec::new();
    let shapes = [
        crate_box(),
        Shape::Sphere { radius: 0.6 },
        Shape::Capsule {
            a: Vec3::new(0.0, -0.6, 0.0),
            b: Vec3::new(0.0, 0.6, 0.0),
            radius: 0.4,
        },
    ];
    for (index, shape) in shapes.into_iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "small indices")]
        let offset = index as f32;
        let mut desc = dynamic(
            &mut world,
            shape,
            Vec3::new(6.0 + offset * 4.0, 5.0 + offset, 8.0 + offset),
            50.0 + offset * 25.0,
        );
        desc.rotation = Quat::from_euler(glam::EulerRot::YXZ, 0.3 * offset, 0.4, -0.2);
        desc.restitution = 0.4;
        let body = world.add_body(desc);
        world.set_angular_velocity(body, Vec3::new(1.0, -0.5 * offset, 0.25));
        bodies.push(body);
    }
    let pelvis_position = world.position(bodies[0]).expect("pelvis");
    let desc = dynamic(
        &mut world,
        Shape::Sphere { radius: 0.15 },
        pelvis_position - Vec3::new(0.0, 1.5, 0.0),
        90.0,
    );
    let lower = world.add_body(desc);
    let pelvis = bodies[0];
    world.add_limited_hinge(&LimitedHingeDesc {
        body_a: pelvis,
        body_b: lower,
        pivot: world.position(pelvis).expect("pelvis"),
        axis: Vec3::X,
        min_angle: -std::f32::consts::PI / 8.0,
        max_angle: std::f32::consts::PI / 4.0,
        max_friction_torque: 3.0,
    });
    bodies.push(lower);
    (world, bodies)
}

/// The bits of every body state.
fn state_bits(world: &World, bodies: &[BodyId]) -> Vec<u32> {
    let mut bits = Vec::new();
    for &body in bodies {
        let position = world.position(body).expect("body");
        let rotation = world.rotation(body).expect("body");
        let linear = world.linear_velocity(body).expect("body");
        let angular = world.angular_velocity(body).expect("body");
        for value in position
            .to_array()
            .into_iter()
            .chain(rotation.to_array())
            .chain(linear.to_array())
            .chain(angular.to_array())
        {
            bits.push(value.to_bits());
        }
        bits.push(u32::from(world.is_active(body).expect("body")));
    }
    bits
}

#[test]
fn the_same_scene_twice_gives_bit_identical_states() {
    let (mut first, bodies) = busy_scene();
    let (mut second, _) = busy_scene();
    for frame in 0..400 {
        first.step_delta_time(DT, &mut ());
        second.step_delta_time(DT, &mut ());
        assert_eq!(
            state_bits(&first, &bodies),
            state_bits(&second, &bodies),
            "frame {frame}"
        );
    }
    // The scene did something: the bodies moved and touched the terrain.
    assert!(
        first
            .manifolds()
            .iter()
            .any(|manifold| !manifold.points.is_empty())
    );
}
