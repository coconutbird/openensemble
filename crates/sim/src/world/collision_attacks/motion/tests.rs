use super::*;
use crate::entities::SquadMode;
use crate::physics::test_support::box_template;
use crate::player::PlayerId;
use std::f32::consts::FRAC_1_SQRT_2;

const MASS: f32 = 10.0;

#[test]
fn rammed_impulse_reads_the_upward_velocity_of_the_body() {
    let gameplay = GameplayCatalog::default();
    let (mut world, attacker, target) = two_bodies();
    // The unit velocity is zero; only the body moves up, between the medium and top caps.
    assert!(world.set_unit_body_linear_velocity(target.id, Vec3::new(0.0, 4.5, 0.0)));

    world.apply_rammed_target_motion(
        &attacker,
        &target,
        Vec3::X,
        CollisionDisposition::Rammable,
        &gameplay,
    );

    let (sin, cos) = math::sin_cos(RAMMED_IMPULSE_ANGLE);
    let velocity = world.unit_body_linear_velocity(target.id).unwrap();
    assert_vec_close(
        velocity,
        Vec3::new(
            RAMMED_IMPULSE_FORCE * cos,
            4.5 + RAMMED_IMPULSE_FORCE * sin * 0.5,
            0.0,
        ),
    );
}

#[test]
fn bowled_target_is_thrown_off_the_rammer_heading() {
    let gameplay = GameplayCatalog::default();
    let (mut world, mut attacker, target) = two_bodies();
    attacker.velocity = Vec3::X * 5.0;

    world.apply_rammed_target_motion(
        &attacker,
        &target,
        Vec3::X,
        CollisionDisposition::Bowlable,
        &gameplay,
    );

    // The target is on the +Z side of the heading, so the throw turns 45 degrees to +Z.
    let (sin, cos) = math::sin_cos(THROW_LAUNCH_ANGLE);
    let planar = FRAC_1_SQRT_2 * cos * THROW_SPEED;
    let expected = Vec3::new(planar, sin * THROW_SPEED, planar);
    assert_vec_close(
        world.unit_body_linear_velocity(target.id).unwrap(),
        expected,
    );
    let target = world.get_unit(target.id).unwrap();
    assert_vec_close(target.base.velocity, expected);
}

#[test]
fn bowled_target_in_cover_is_not_thrown() {
    let gameplay = GameplayCatalog::default();
    let (mut world, attacker, target) = two_bodies();
    world.get_squad_mut(target.squad_id.unwrap()).unwrap().mode = SquadMode::Cover;

    world.apply_rammed_target_motion(
        &attacker,
        &target,
        Vec3::X,
        CollisionDisposition::Bowlable,
        &gameplay,
    );

    assert_eq!(world.unit_body_linear_velocity(target.id), Some(Vec3::ZERO));
}

#[test]
fn throw_without_thrower_velocity_goes_straight_up() {
    assert_eq!(
        bowled_throw_velocity(Vec3::ZERO, Vec3::ZERO, Vec3::X),
        Vec3::Y * THROW_SPEED
    );
}

#[test]
fn overrun_bounce_needs_the_minimum_body_speed() {
    let (mut world, attacker, _) = two_bodies();
    let tuning = OverrunTuning {
        minimum_speed: 3.0,
        jump_force: 50.0,
    };
    assert!(world.set_unit_body_linear_velocity(attacker.id, Vec3::X * 3.0));
    world.apply_overrun_bounce(attacker.id, tuning);
    assert_vec_close(
        world.unit_body_linear_velocity(attacker.id).unwrap(),
        Vec3::X * 3.0,
    );

    assert!(world.set_unit_body_linear_velocity(attacker.id, Vec3::X * 4.0));
    world.apply_overrun_bounce(attacker.id, tuning);
    assert_vec_close(
        world.unit_body_linear_velocity(attacker.id).unwrap(),
        Vec3::new(4.0, 50.0 / MASS, 0.0),
    );
}

#[test]
fn rammer_stops_bounces_back_and_moves_its_squad_behind_it() {
    let (mut world, attacker, _) = two_bodies();
    assert!(world.set_unit_body_linear_velocity(attacker.id, Vec3::new(8.0, 0.0, 1.0)));
    assert!(world.set_unit_body_angular_velocity(attacker.id, Vec3::Y));
    let unit = world.get_unit(attacker.id).unwrap();
    let (position, radius) = (unit.base.position, unit.obstruction_radius());

    world.push_rammer_back(attacker.id, attacker.squad_id, Vec3::X);

    let (sin, cos) = math::sin_cos(RAMMER_IMPULSE_ANGLE);
    assert_vec_close(
        world.unit_body_linear_velocity(attacker.id).unwrap(),
        Vec3::new(-cos, sin, 0.0) * RAMMER_IMPULSE_FORCE,
    );
    assert_eq!(
        world.unit_body_angular_velocity(attacker.id),
        Some(Vec3::ZERO)
    );
    let squad = world.get_squad(attacker.squad_id).unwrap();
    let expected = position - Vec3::X * radius;
    assert_vec_close(squad.position(), expected);
    assert_vec_close(squad.leash_position(), expected);
    assert_vec_close(squad.anchor_position(), expected);
}

fn two_bodies() -> (World, RammerSnapshot, RamTargetSnapshot) {
    let mut world = World::with_seed(31);
    world.init_players(2);
    let attacker_squad = world.create_squad_at(1, Vec3::ZERO);
    let attacker_id = body_unit(&mut world, 1, Vec3::ZERO);
    assert!(world.attach_unit_to_squad(attacker_id, attacker_squad));
    let target_position = Vec3::new(1.5, 0.0, 0.5);
    let target_squad = world.create_squad_at(2, target_position);
    let target_id = body_unit(&mut world, 2, target_position);
    assert!(world.attach_unit_to_squad(target_id, target_squad));
    let attacker = RammerSnapshot {
        id: attacker_id,
        squad_id: attacker_squad,
        player_id: 1,
        position: Vec3::ZERO,
        forward: Vec3::X,
        velocity: Vec3::ZERO,
        proto_object_name: "rammer".to_owned(),
        ordered_target_id: target_squad,
        ability_id: None,
        damage_multiplier: 1.0,
    };
    let target = RamTargetSnapshot {
        id: target_id,
        squad_id: Some(target_squad),
        player_id: 2,
        position: target_position,
        forward: Vec3::X,
        proto_object_name: "target".to_owned(),
        mode: SquadMode::Normal,
        hitpoints: 100.0,
        in_cover: false,
    };
    (world, attacker, target)
}

fn body_unit(world: &mut World, player_id: PlayerId, position: Vec3) -> EntityId {
    let unit_id = world.create_unit_at(player_id, position);
    let unit = world.get_unit_mut(unit_id).unwrap();
    unit.set_max_hitpoints(100.0);
    unit.obstruction_half_extents = Vec3::ONE;
    let template = box_template("body", Vec3::ONE, MASS, Vec3::ZERO);
    assert!(world.create_physics_object(unit_id, &template, false));
    unit_id
}

fn assert_vec_close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.abs_diff_eq(expected, 0.000_1),
        "{actual:?} != {expected:?}"
    );
}
