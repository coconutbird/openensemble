use super::*;
use crate::gameplay::{DamagePartProfile, ThrownDamagePart};
use crate::physics::test_support::box_template;
use pipeline::database::hw1::ProtoObject;

const MASS: f32 = 20.0;

#[test]
fn rock_is_a_thrown_part_body_with_the_damage_template_impulse() {
    let (database, gameplay) = rock_data(1.5);
    let mut world = World::with_seed(77);
    world.init_players(1);
    let mut stream = world.sim_rng;
    let z = stream.range_float(-1.0, 1.0);
    let y = stream.range_float(1.0, 1.1);
    let x = stream.range_float(-1.0, 1.0);
    let impulse = stream.range_float(250.0, 500.0);
    let scale = Vec3::new(
        stream.range_float(0.6, 1.4),
        stream.range_float(0.6, 1.4),
        stream.range_float(0.6, 1.4),
    );
    let direction = Vec3::new(x, y, z).normalize();
    let expected = (direction * scale + Vec3::Y) * impulse * 1.5;

    spawn_rock(&mut world, &database, Some(&gameplay), request());

    let object_id = world.power_manager.debris[0].object_id;
    let object = world.get_object(object_id).unwrap();
    assert_eq!(object.base.position, Vec3::new(4.0, SPAWN_HEIGHT, 0.0));
    let body = world.physics_state(object_id).unwrap().body();
    let velocity = world.physics().world().linear_velocity(body).unwrap();
    assert!(
        velocity.abs_diff_eq(expected / MASS, 0.000_1),
        "{velocity:?} != {:?}",
        expected / MASS
    );
    assert_eq!(world.sim_rng, stream);
}

#[test]
fn rock_without_a_damage_template_is_not_made() {
    let (database, _) = rock_data(1.0);
    let mut world = World::with_seed(77);
    world.init_players(1);

    spawn_rock(
        &mut world,
        &database,
        Some(&GameplayCatalog::default()),
        request(),
    );

    assert!(world.power_manager.debris.is_empty());
    assert_eq!(world.objects.iter().count(), 0);
}

#[test]
fn rock_goes_at_the_end_of_its_life() {
    let (database, gameplay) = rock_data(1.0);
    let mut world = World::with_seed(77);
    world.init_players(1);
    spawn_rock(&mut world, &database, Some(&gameplay), request());
    let object_id = world.power_manager.debris[0].object_id;

    world.game_time_ms = LIFETIME_MS - 1;
    update(&mut world);
    assert!(world.get_object(object_id).is_some());
    world.game_time_ms = LIFETIME_MS;
    update(&mut world);
    assert!(world.get_object(object_id).is_none());
    assert!(world.power_manager.debris.is_empty());
}

fn request() -> DebrisSpawn<'static> {
    DebrisSpawn {
        player_id: 1,
        prototype: "rock",
        impact_position: Vec3::X * 4.0,
        projectile_direction: -Vec3::Y,
        maximum_impulse: 500.0,
        y_range: 0.1,
    }
}

fn rock_data(force_multiplier: f32) -> (Database, GameplayCatalog) {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "rock".to_owned(),
        ..ProtoObject::default()
    });
    let mut gameplay = GameplayCatalog::default();
    let template = box_template(
        "rock_part",
        Vec3::splat(0.5),
        MASS,
        Vec3::new(0.1, 0.2, 0.3),
    );
    let part = ThrownDamagePart::new(
        vec!["rock_a".to_owned(), "rock_b".to_owned()],
        template,
        force_multiplier,
    )
    .unwrap();
    gameplay.insert_test_damage_parts("rock", DamagePartProfile::new(vec![Some(part)]));
    (database, gameplay)
}
