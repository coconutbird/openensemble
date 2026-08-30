use super::*;
use crate::entities::UnitDetonatePhase;
use crate::physics::MotionType;
use pipeline::database::hw1::tactics::{Action, ActionDuration, TacticData, TacticRules, Weapon};
use pipeline::database::hw1::{Database, ProtoObject};

#[test]
fn nonrolling_building_falls_and_detonates_on_terrain_contact() {
    let (database, gameplay) = fixture(-1.0, 10.0);
    let mut world = configured_world();
    let source = configured_bomb(&mut world, Vec3::Y);
    let target = configured_target(&mut world, Vec3::X * 2.0);

    update(&mut world, &database, &gameplay);
    let bomb = world.get_unit(source).expect("Bomb after first update");
    assert_eq!(bomb.bomb_phase(), BombPhase::Working);
    assert!(!bomb.bomb_rolls());
    assert!(bomb.base.position.y < 1.0, "non-mobile body must fall");
    assert_eq!(bomb.detonate_phase(), UnitDetonatePhase::Working);

    for _ in 0..40 {
        if world.get_unit(source).is_none() {
            break;
        }
        update(&mut world, &database, &gameplay);
    }

    assert!(world.get_unit(source).is_none());
    assert!(world.get_unit(target).unwrap().hitpoints < 100.0);
}

#[test]
fn rolling_branch_completes_bomb_but_keeps_primary_dynamic_body() {
    let (database, gameplay) = fixture(1.0, 10.0);
    let mut world = configured_world();
    let source = configured_bomb(&mut world, Vec3::Y);

    for _ in 0..40 {
        update(&mut world, &database, &gameplay);
        if world
            .get_unit(source)
            .is_some_and(|unit| unit.bomb_phase() == BombPhase::Complete)
        {
            break;
        }
    }

    let bomb = world
        .get_unit(source)
        .expect("rolling Bomb before countdown");
    assert_eq!(bomb.bomb_phase(), BombPhase::Complete);
    assert!(bomb.bomb_rolls());
    assert!(bomb.bomb_has_collided());
    assert_eq!(
        bomb.physics.as_ref().unwrap().motion_type(),
        MotionType::Dynamic
    );
    assert_eq!(bomb.detonate_phase(), UnitDetonatePhase::Working);
}

fn configured_world() -> World {
    let mut world = World::with_seed(7);
    world.init_players(2);
    world.get_player_mut(1).unwrap().team_id = 1;
    world.get_player_mut(2).unwrap().team_id = 2;
    world.configure_standard_team_relations();
    world
}

fn configured_bomb(world: &mut World, position: Vec3) -> EntityId {
    let id = world.create_building_at(1, position);
    let unit = world.get_unit_mut(id).unwrap();
    unit.proto_object_name = "bomb".to_owned();
    unit.set_max_hitpoints(100.0);
    unit.obstruction_half_extents = Vec3::splat(0.5);
    unit.physics = Some(PhysicsBody::static_obstruction(BoxCollider::new(
        Vec3::splat(0.5),
        Vec3::ZERO,
    )));
    id
}

fn configured_target(world: &mut World, position: Vec3) -> EntityId {
    let id = world.create_unit_at(2, position);
    let unit = world.get_unit_mut(id).unwrap();
    unit.proto_object_name = "target".to_owned();
    unit.set_max_hitpoints(100.0);
    unit.obstruction_half_extents = Vec3::splat(0.5);
    id
}

fn update(world: &mut World, database: &Database, gameplay: &GameplayCatalog) {
    world.update_entities_with_database_and_gameplay(0.05, database, gameplay);
}

fn fixture(roll_chance: f32, duration: f32) -> (Database, GameplayCatalog) {
    let mut database = Database::new();
    database.objects.extend([
        ProtoObject {
            name: "bomb".to_owned(),
            tactics: Some("bomb.tactics".to_owned()),
            physics_info: Some("egg".to_owned()),
            ..ProtoObject::default()
        },
        ProtoObject {
            name: "target".to_owned(),
            ..ProtoObject::default()
        },
    ]);
    let tactics = TacticData {
        actions: vec![
            Action {
                name: "Bomb".to_owned(),
                action_type: Some("Bomb".to_owned()),
                work_range: Some(roll_chance),
                ..Action::default()
            },
            Action {
                name: "Detonate".to_owned(),
                action_type: Some("Detonate".to_owned()),
                weapon: Some("BombWeapon".to_owned()),
                duration: Some(ActionDuration {
                    seconds: duration,
                    ..ActionDuration::default()
                }),
                ..Action::default()
            },
        ],
        weapons: vec![Weapon {
            name: "BombWeapon".to_owned(),
            damage_per_second: Some(50.0),
            aoe_radius: Some(8.0),
            ..Weapon::default()
        }],
        tactic: Some(TacticRules {
            persistent_actions: vec!["Bomb".to_owned(), "Detonate".to_owned()],
            ..TacticRules::default()
        }),
        ..TacticData::default()
    };
    let gameplay = GameplayCatalog::from_tactics(&database, [("bomb".to_owned(), tactics)]);
    (database, gameplay)
}
