use super::*;
use crate::entity::Entity;
use crate::entity_id::{EntityClass, EntityId};

#[test]
fn revealer_uses_class_zero_and_expands_only_presentation_radius() {
    let id = EntityId::new(EntityClass::Object, 3);
    let revealer = Revealer::new(2, 5.0, 2.0, Some(1_000));
    let mut object = Object::new_revealer(
        id,
        1,
        Vec3::new(10.0, 0.0, 20.0),
        13,
        "sys_revealer".to_owned(),
        revealer,
    );

    assert_eq!(object.id(), id);
    assert!((object.revealer().unwrap().line_of_sight() - 10.0).abs() < f32::EPSILON);
    assert!(
        object
            .revealer()
            .unwrap()
            .covers(object.base.position, Vec3::new(16.0, 99.0, 28.0))
    );
    object.update(0.15);
    assert!((object.revealer().unwrap().reveal_fraction() - 0.4995).abs() < 0.0001);
    assert!((object.revealer().unwrap().line_of_sight() - 10.0).abs() < f32::EPSILON);
}

#[test]
fn global_revealer_covers_every_finite_position() {
    let revealer = Revealer::new(1, Revealer::GLOBAL_LINE_OF_SIGHT, 1.0, None);

    assert!(revealer.covers(Vec3::ZERO, Vec3::new(1_000_000.0, 500.0, -2_000_000.0)));
}
