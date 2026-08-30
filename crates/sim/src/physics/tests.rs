use super::*;
use crate::entities::Unit;
use crate::entity_id::EntityClass;

fn dynamic_body(position: Vec3) -> (BaseEntity, PhysicsBody) {
    let id = EntityId::new(EntityClass::Unit, 0);
    let mut entity = BaseEntity::new(id, 1);
    entity.position = position;
    let body = PhysicsBody::ground_vehicle(
        PhysicsMaterial {
            mass: 100.0,
            friction: 1.0,
            restitution: 0.5,
            linear_damping: 0.0,
            angular_damping: 0.1,
        },
        BoxCollider::new(Vec3::splat(1.0), Vec3::ZERO),
        position.y,
        40.0,
        60.0,
        450.0,
    );
    (entity, body)
}

#[test]
fn vehicle_accelerates_and_turns_with_limits() {
    let (mut entity, mut body) = dynamic_body(Vec3::ZERO);
    let arrived = body.update(
        &mut entity,
        Some(Vec3::new(100.0, 0.0, 0.0)),
        0.05,
        1.0,
        false,
    );

    assert!(!arrived);
    assert!(entity.velocity.length() <= 3.0 + f32::EPSILON);
    assert!(entity.forward.x > 0.0);
    assert!(entity.forward.z > 0.0);
}

#[test]
fn reverse_vehicle_faces_away_while_accelerating_toward_target() {
    let (mut entity, mut body) = dynamic_body(Vec3::ZERO);
    let arrived = body.update(
        &mut entity,
        Some(Vec3::new(100.0, 0.0, 0.0)),
        0.05,
        1.0,
        true,
    );

    assert!(!arrived);
    assert!(entity.forward.x < 0.0);
    assert!(entity.velocity.x > 0.0);
}

#[test]
fn off_center_impulse_changes_linear_and_angular_velocity() {
    let (mut entity, mut body) = dynamic_body(Vec3::ZERO);
    body.apply_impulse_at_point(
        &mut entity,
        Vec3::new(100.0, 20.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    );

    assert!(entity.velocity.x > 0.0);
    assert!(entity.velocity.y > 0.0);
    assert!(body.angular_velocity().y > 0.0);
    assert!(!body.is_grounded());
}

#[test]
fn angular_impulse_is_scaled_by_box_inertia() {
    let (_entity, mut body) = dynamic_body(Vec3::ZERO);

    body.apply_angular_impulse(Vec3::new(0.0, 100.0, 0.0));

    assert!(
        body.angular_velocity()
            .abs_diff_eq(Vec3::new(0.0, 1.5, 0.0), 1.0e-6)
    );
}

#[test]
fn dynamic_body_is_separated_from_static_obstruction() {
    let mut units = EntityManager::new(EntityClass::Unit);
    let moving_id = units.allocate_id();
    let mut moving = Unit::new(moving_id, 1);
    moving.base.position = Vec3::new(-0.5, 0.0, 0.0);
    moving.physics = Some(dynamic_body(moving.base.position).1);
    moving.base.velocity = Vec3::X;
    units.insert(moving_id, moving);

    let static_id = units.allocate_id();
    let mut obstruction = Unit::new_building(static_id, 0);
    obstruction.physics = Some(PhysicsBody::static_obstruction(BoxCollider::new(
        Vec3::splat(1.0),
        Vec3::ZERO,
    )));
    units.insert(static_id, obstruction);

    let contacts = resolve_unit_collisions(&mut units, &std::collections::BTreeSet::new());

    let moving = units.get(moving_id).unwrap();
    assert!(moving.base.position.x <= -2.0);
    assert_eq!(moving.physics.as_ref().unwrap().contacts_this_step(), 1);
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].first, moving_id);
    assert_eq!(contacts[0].second, static_id);
    assert_eq!(contacts[0].projected_velocity.to_bits(), 1.0_f32.to_bits());
}

#[test]
fn substeps_bound_large_updates() {
    let (count, duration) = substeps(0.2).unwrap();
    assert_eq!(count, 4);
    assert!((duration - 0.05).abs() < f32::EPSILON);
    assert!(substeps(f32::NAN).is_none());
}
