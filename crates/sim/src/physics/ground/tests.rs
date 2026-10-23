use super::*;
use crate::entity_id::{EntityClass, EntityId};
use crate::physics::{BoxCollider, PhysicsMaterial};

const SLOPE: f32 = 0.25;

fn ramp(point: Vec3) -> f32 {
    SLOPE * point.x
}

fn ramp_normal() -> Vec3 {
    Vec3::new(-SLOPE, 1.0, 0.0).normalize()
}

fn assert_vec_close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.distance(expected) < 0.000_1,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn ground_sample_reproduces_retail_footprint_on_a_plane() {
    let sample = sample_ground(
        Vec3::new(8.0, 50.0, 3.0),
        Vec3::new(1.0, 0.4, 1.0),
        |point| Some(ramp(point)),
    )
    .expect("the ramp has a height everywhere");

    assert!((sample.height - SLOPE * 8.0).abs() < 0.000_1);
    assert_vec_close(sample.normal, ramp_normal());
}

#[test]
fn ground_sample_uses_the_higher_diagonal_on_a_ridge() {
    // A ridge along the heading: only the higher diagonal sets the normal.
    let ridge = |point: Vec3| Some(if point.x > 0.0 { 0.0 } else { 1.0 });
    let sample = sample_ground(Vec3::ZERO, Vec3::Z, ridge).expect("ridge height");

    assert!((sample.height - 0.5).abs() < 0.000_1);
    assert!(sample.normal.y > 0.0);
    assert!(sample.normal.is_normalized());
}

#[test]
fn ground_frame_projects_the_heading_onto_the_terrain() {
    let (right, forward) = ground_frame(ramp_normal(), Vec3::X);

    assert!(forward.dot(ramp_normal()).abs() < 0.000_1);
    assert!(forward.x > 0.0 && forward.y > 0.0);
    assert_vec_close(right, Vec3::NEG_Z);
}

#[test]
fn filter_snaps_from_world_up_and_blends_after() {
    assert_eq!(filter_up(Vec3::Y, ramp_normal(), Vec3::X), ramp_normal());

    let tilted = Vec3::new(0.0, 1.0, 0.25).normalize();
    let blended = filter_up(tilted, ramp_normal(), Vec3::X);
    assert!(blended.is_normalized());
    assert!(blended.angle_between(tilted) < blended.angle_between(ramp_normal()));
    assert!(blended.angle_between(ramp_normal()) < tilted.angle_between(ramp_normal()));
}

#[test]
fn grounded_body_slides_on_the_terrain_plane() {
    let mut entity = BaseEntity::new(EntityId::new(EntityClass::Unit, 0), 1);
    let mut body = PhysicsBody::ground_vehicle(
        PhysicsMaterial::default(),
        BoxCollider::new(Vec3::splat(1.0), Vec3::ZERO),
        0.0,
        10.0,
        60.0,
        450.0,
    );
    for _ in 0..40 {
        let sample = sample_ground(entity.position, entity.forward, |point| Some(ramp(point)))
            .expect("ramp height");
        body.set_ground(entity.position, sample);
        let _arrived = body.update(
            &mut entity,
            Some(Vec3::new(100.0, 0.0, 0.0)),
            0.05,
            1.0,
            false,
        );
        assert!(body.is_grounded());
        assert!((entity.position.y - SLOPE * entity.position.x).abs() < 0.001);
    }
    assert!(entity.position.x > 5.0);
    assert_vec_close(entity.up, ramp_normal());
}
