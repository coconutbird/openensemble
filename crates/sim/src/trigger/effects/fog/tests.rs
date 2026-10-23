use super::*;
use crate::trigger::{EffectType, TriggerValue, TriggerVar, VarType};
use glam::Vec3;

fn effect(version: u8) -> Effect {
    let mut effect = Effect::new(1, EffectType::EnableFogOfWar).with_input_at(1, 1);
    effect.version = version;
    effect
}

fn script(enabled: bool) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    script.add_variable(TriggerVar::new(1, VarType::Bool).with_value(TriggerValue::Bool(enabled)));
    script
}

fn world_with_map() -> World {
    let mut world = World::new();
    world.configure_terrain_simulation(crate::world::flat_test_terrain(1.0, 0.0));
    world.init_players(1);
    world.get_player_mut(1).unwrap().team_id = 1;
    world
}

#[test]
fn enable_fog_of_war_switches_the_world_fog_for_every_version() {
    let mut world = world_with_map();

    assert_eq!(
        set_enabled(&effect(1), &script(false), &mut world),
        EffectOutcome::Applied
    );
    assert!(!world.fog_of_war_enabled());
    assert!(world.is_position_visible_to_team(1, Vec3::new(4.0, 0.0, 4.0)));
    assert_eq!(
        set_enabled(&effect(2), &script(true), &mut world),
        EffectOutcome::Applied
    );
    assert!(world.fog_of_war_enabled());
    assert!(!world.is_position_visible_to_team(1, Vec3::new(4.0, 0.0, 4.0)));
    assert!(world.is_position_explored_to_team(1, Vec3::new(4.0, 0.0, 4.0)));
}

#[test]
fn invalid_binding_skips() {
    let mut world = World::new();
    let empty_script = TriggerScript::new(1);

    assert_eq!(
        set_enabled(&effect(1), &empty_script, &mut world),
        EffectOutcome::Skipped
    );
    assert!(world.fog_of_war_enabled());
}

#[test]
fn clear_and_reset_black_map_change_the_explored_tiles() {
    let mut clear = Effect::new(2, EffectType::ClearBlackMap);
    clear.version = 1;
    let mut reset = Effect::new(3, EffectType::ResetBlackMap);
    reset.version = 2;
    let mut world = world_with_map();

    assert_eq!(black_map(&clear, &mut world), EffectOutcome::Applied);
    assert!(world.black_map_is_cleared());
    assert!(world.is_position_explored_to_team(1, Vec3::new(7.0, 0.0, 7.0)));
    assert!(!world.is_position_visible_to_team(1, Vec3::new(7.0, 0.0, 7.0)));
    assert_eq!(black_map(&reset, &mut world), EffectOutcome::Applied);
    assert!(!world.black_map_is_cleared());
    assert!(!world.is_position_explored_to_team(1, Vec3::new(7.0, 0.0, 7.0)));
}
