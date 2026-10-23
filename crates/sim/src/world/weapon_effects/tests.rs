use super::dot::dot_end_time;
use super::elapsed_milliseconds;
use crate::entities::squads::weapon_effects::SquadDaze;
use crate::gameplay::GameplayCatalog;
use crate::world::World;
use glam::Vec3;

fn squad_world() -> (World, crate::EntityId, crate::EntityId) {
    let mut world = World::new();
    world.init_players(2);
    let unit_id = world.create_unit_at(2, Vec3::ZERO);
    let squad_id = world.create_squad_at(2, Vec3::ZERO);
    assert!(world.attach_unit_to_squad(unit_id, squad_id));
    (world, squad_id, unit_id)
}

#[test]
fn dot_end_time_uses_the_retail_float_sum() {
    assert_eq!(dot_end_time(1_000, 1.0), 2_000);
    // `(float)gametime` loses the low bits of a large game time.
    assert_eq!(dot_end_time(16_777_217, 0.0), 16_777_216);
}

#[test]
fn step_length_is_whole_milliseconds() {
    assert_eq!(elapsed_milliseconds(0.05), 50);
    assert_eq!(elapsed_milliseconds(0.0), 0);
}

#[test]
fn daze_starts_on_the_first_update_and_ends_after_its_duration() {
    let (mut world, squad_id, _) = squad_world();
    let gameplay = GameplayCatalog::default();
    world.squads.get_mut(squad_id).unwrap().weapon_effects.daze = Some(SquadDaze {
        working: false,
        remaining_ms: 100,
        movement_modifier: 0.0,
        smart_target: false,
        effects: Vec::new(),
    });
    world.update_dazes(50, None, &gameplay);
    let squad = world.get_squad(squad_id).unwrap();
    assert!(squad.is_attack_blocked());
    assert!(squad.is_held_by_weapon_effect());
    assert_eq!(squad.daze_remaining_ms(), Some(100));
    world.update_dazes(50, None, &gameplay);
    assert_eq!(
        world.get_squad(squad_id).unwrap().daze_remaining_ms(),
        Some(50)
    );
    world.update_dazes(50, None, &gameplay);
    let squad = world.get_squad(squad_id).unwrap();
    assert!(!squad.is_dazed());
    assert!(!squad.is_attack_blocked());
    assert!(!squad.is_held_by_weapon_effect());
}

#[test]
fn a_full_stasis_stops_the_squad_and_a_release_frees_it() {
    let (mut world, squad_id, unit_id) = squad_world();
    let gameplay = GameplayCatalog::default();
    world
        .squads
        .get_mut(squad_id)
        .unwrap()
        .weapon_effects
        .stasis_effects = 1;
    world
        .units
        .get_mut(unit_id)
        .unwrap()
        .weapon_effects
        .stasis_action = true;
    world.update_unit_stasis_actions(&gameplay);
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.stasis_speed_multiplier().to_bits(), 0.0_f32.to_bits());
    assert!(squad.is_attack_blocked());
    world
        .squads
        .get_mut(squad_id)
        .unwrap()
        .weapon_effects
        .stasis_effects = 0;
    world.update_unit_stasis_actions(&gameplay);
    let squad = world.get_squad(squad_id).unwrap();
    assert_eq!(squad.stasis_speed_multiplier().to_bits(), 1.0_f32.to_bits());
    assert!(!squad.is_attack_blocked());
    assert!(
        !world
            .get_unit(unit_id)
            .unwrap()
            .weapon_effects
            .stasis_action
    );
}
