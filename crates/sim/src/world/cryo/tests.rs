use super::*;
use crate::entities::SquadCryoState;
use glam::Vec3;
use pipeline::database::hw1::{GameData, Squad as ProtoSquad};

#[test]
fn layered_resistance_and_globals_drive_freeze_then_thaw() {
    let database = database();
    let (mut world, squad_id, unit_id) = world_with_squad();

    assert!(world.add_squad_cryo(squad_id, 20.0, &database));
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.cryo_state(), SquadCryoState::Freezing);
    assert_eq!(squad.cryo_points().to_bits(), 100.0_f32.to_bits());
    assert_eq!(squad.maximum_cryo_points().to_bits(), 120.0_f32.to_bits());
    let unit = world.get_unit(unit_id).unwrap();
    assert_eq!(
        unit.effective_velocity_scalar().to_bits(),
        0.5_f32.to_bits()
    );
    assert_eq!(
        unit.effective_damage_taken_multiplier().to_bits(),
        1.25_f32.to_bits()
    );
    assert!(!unit.is_shatter_on_death());

    world.update_cryo(3.0);
    assert_eq!(
        world.get_squad(squad_id).unwrap().cryo_state(),
        SquadCryoState::Thawing
    );
    world.update_cryo(1.0);
    assert_eq!(
        world.get_squad(squad_id).unwrap().cryo_state(),
        SquadCryoState::None
    );
    let unit = world.get_unit(unit_id).unwrap();
    assert_eq!(
        unit.effective_velocity_scalar().to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        unit.effective_damage_taken_multiplier().to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn frozen_members_stop_and_new_members_inherit_then_clear_the_effect() {
    let database = database();
    let (mut world, squad_id, first_id) = world_with_squad();
    world
        .get_squad_mut(squad_id)
        .unwrap()
        .move_to(Vec3::new(100.0, 0.0, 0.0));

    assert!(world.add_squad_cryo(squad_id, 120.0, &database));
    assert!(world.get_squad(squad_id).unwrap().is_cryo_frozen());
    assert!(world.get_unit(first_id).unwrap().is_shatter_on_death());
    let start = world.get_squad(squad_id).unwrap().base.position;
    world.update_entities(1.0);
    assert_eq!(world.get_squad(squad_id).unwrap().base.position, start);

    let second_id = world.create_unit(1);
    assert!(world.attach_unit_to_squad(second_id, squad_id));
    assert!(world.get_unit(second_id).unwrap().is_cryo_frozen());
    assert!(world.detach_unit_from_squad(second_id));
    assert!(!world.get_unit(second_id).unwrap().is_cryo_frozen());
}

#[test]
fn crossing_from_freezing_uses_retails_short_timer_then_hits_refresh_long_timer() {
    let database = database();
    let (mut world, squad_id, _) = world_with_squad();

    assert!(world.add_squad_cryo(squad_id, 60.0, &database));
    assert!(world.add_squad_cryo(squad_id, 60.0, &database));
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.cryo_state(), SquadCryoState::Frozen);
    assert_eq!(squad.cryo_thaw_delay().to_bits(), 3.0_f32.to_bits());

    assert!(world.add_squad_cryo(squad_id, 1.0, &database));
    assert_eq!(
        world
            .get_squad(squad_id)
            .unwrap()
            .cryo_thaw_delay()
            .to_bits(),
        9.0_f32.to_bits()
    );
}

fn world_with_squad() -> (World, EntityId, EntityId) {
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad(1);
    world.get_squad_mut(squad_id).unwrap().proto_squad_name = "test_squad".to_owned();
    let unit_id = world.create_unit(1);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (world, squad_id, unit_id)
}

fn database() -> Database {
    let mut database = Database::new();
    database.game_data = Some(GameData {
        default_cryo_points: Some(100.0),
        default_thaw_speed: Some(20.0),
        time_freezing_to_thaw: Some(3.0),
        time_frozen_to_thaw: Some(9.0),
        freezing_speed_modifier: Some(0.5),
        freezing_damage_modifier: Some(1.25),
        frozen_damage_modifier: Some(2.0),
        ..GameData::default()
    });
    database.squads.push(ProtoSquad {
        name: "test_squad".to_owned(),
        cryo_points: Some(120.0),
        ..ProtoSquad::default()
    });
    database
}
