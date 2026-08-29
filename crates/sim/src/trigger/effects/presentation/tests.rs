use super::*;
use crate::trigger::{TriggerVar, VarType};

#[test]
fn retail_presentation_ids_are_typed() {
    let expected = [
        (526, EffectType::HudToggle),
        (532, EffectType::SetRenderTerrainSkirt),
        (632, EffectType::CameraShake),
        (809, EffectType::HintCalloutCreate),
        (810, EffectType::HintCalloutDestroy),
        (841, EffectType::EnableChats),
        (687, EffectType::ShowObjectivePointer),
        (773, EffectType::RumbleStart),
        (774, EffectType::RumbleStop),
        (884, EffectType::SetCamera),
        (912, EffectType::FadeToColor),
        (922, EffectType::FadeTransition),
        (1034, EffectType::IgnoreDpad),
        (1037, EffectType::EnableScreenBlur),
        (1044, EffectType::PowerMenuEnable),
        (1045, EffectType::SetMinimapNorthPointerRotation),
        (1048, EffectType::HideCircleMenu),
        (1054, EffectType::SetMinimapSkirtMirroring),
        (1061, EffectType::LockPlayerUser),
    ];
    for (raw_type, effect_type) in expected {
        assert_eq!(EffectType::from_u16(raw_type), Some(effect_type));
    }
}

#[test]
fn fade_transition_writes_timing_to_the_authoritative_world() {
    let mut world = World::new();
    let mut script = TriggerScript::new(1);
    add_value(&mut script, 1, VarType::Time, TriggerValue::Time(100));
    add_value(&mut script, 2, VarType::Time, TriggerValue::Time(50));
    add_value(&mut script, 3, VarType::Time, TriggerValue::Time(100));
    add_value(&mut script, 4, VarType::Bool, TriggerValue::Bool(false));
    add_value(
        &mut script,
        5,
        VarType::Color,
        TriggerValue::Color(crate::trigger::TriggerColor::new(10, 20, 30, 255)),
    );
    let mut effect = Effect::new(1, EffectType::FadeTransition);
    for slot in 1_u16..=5 {
        effect = effect.with_input_at(slot, u32::from(slot));
    }

    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert_eq!(world.screen_fade_overlay().unwrap().rgba(), [10, 20, 30, 0]);
    world.advance_time(100);
    assert_eq!(
        world.screen_fade_overlay().unwrap().rgba(),
        [10, 20, 30, 255]
    );
    world.advance_time(150);
    assert!(world.screen_fade_overlay().is_none());
    assert!(world.screen_fade_completed());
}

#[test]
fn durable_ui_controls_mutate_only_authoritative_world_state() {
    let mut world = World::new();
    world.init_players(2);
    let mut script = TriggerScript::new(7);
    add_value(
        &mut script,
        1,
        VarType::HUDItem,
        TriggerValue::String("Resources".to_owned()),
    );
    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(false));
    let hud = Effect::new(1, EffectType::HudToggle)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    assert_eq!(
        execute(&hud, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert!(!world.hud_item_enabled(HudItem::Resources));

    let skirt = Effect::new(2, EffectType::SetRenderTerrainSkirt).with_input_at(1, 2);
    assert_eq!(
        execute(&skirt, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert!(!world.render_terrain_skirt_enabled());

    add_value(&mut script, 20, VarType::Bool, TriggerValue::Bool(false));
    let chats = Effect::new(20, EffectType::EnableChats).with_input_at(1, 20);
    assert_eq!(
        execute(&chats, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert!(!world.chats_enabled());

    add_value(&mut script, 3, VarType::Float, TriggerValue::Float(180.0));
    let rotation = Effect::new(3, EffectType::SetMinimapNorthPointerRotation).with_input_at(1, 3);
    assert_eq!(
        execute(&rotation, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert_eq!(
        world.minimap_rotation_degrees().to_bits(),
        180.0_f32.to_bits()
    );

    let hide = Effect::new(4, EffectType::HideCircleMenu);
    let before = world.circle_menu_reset_revision();
    assert_eq!(
        execute(&hide, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert!(world.circle_menu_reset_revision() > before);
}

#[test]
fn objective_pointer_v5_resolves_targets_and_empty_audiences_to_all_users() {
    let mut world = World::new();
    world.init_players(2);
    let mut script = TriggerScript::new(3);
    add_value(&mut script, 1, VarType::Integer, TriggerValue::Int(2));
    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(true));
    add_value(
        &mut script,
        5,
        VarType::Vector,
        TriggerValue::Vector(crate::trigger::value::Vec3::new(4.0, 5.0, 6.0)),
    );
    add_value(
        &mut script,
        7,
        VarType::PlayerList,
        TriggerValue::PlayerList(Vec::new()),
    );
    add_value(&mut script, 8, VarType::Bool, TriggerValue::Bool(false));
    add_value(&mut script, 9, VarType::Bool, TriggerValue::Bool(true));
    let mut show = Effect::new(1, EffectType::ShowObjectivePointer);
    show.version = 5;
    for slot in [1_u16, 2, 5, 7, 8, 9] {
        show = show.with_input_at(slot, u32::from(slot));
    }

    assert_eq!(
        execute(&show, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    for player_id in [1, 2] {
        let pointer = world.objective_pointer(player_id, 2).copied().unwrap();
        assert_eq!(pointer.target_position(), Vec3::new(4.0, 5.0, 6.0));
        assert!(!pointer.use_target());
        assert!(pointer.force_target_visible());
    }

    script.get_variable_mut(2).unwrap().value = TriggerValue::Bool(false);
    let hide = Effect {
        inputs: vec![show.inputs[0], show.inputs[1], show.inputs[3]],
        ..show
    };
    assert_eq!(
        execute(&hide, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    assert!(world.objective_pointers(1).next().is_none());
    assert!(world.objective_pointers(2).next().is_none());
}

#[test]
fn camera_shake_v2_uses_retail_defaults_for_all_users() {
    let mut world = World::new();
    world.init_players(2);
    let mut script = TriggerScript::new(4);
    add_value(&mut script, 1, VarType::Time, TriggerValue::Time(500));
    add_value(&mut script, 2, VarType::Float, TriggerValue::Float(2.0));
    add_value(
        &mut script,
        6,
        VarType::PlayerList,
        TriggerValue::PlayerList(Vec::new()),
    );
    let mut effect = Effect::new(1, EffectType::CameraShake);
    effect.version = 2;
    for slot in [1_u16, 2, 6] {
        effect = effect.with_input_at(slot, u32::from(slot));
    }

    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    for player_id in [1, 2] {
        let shake = world.camera_shake(player_id).unwrap();
        assert_eq!(shake.strength().to_bits(), 2.0_f32.to_bits());
        assert_eq!(shake.conservation_factor().to_bits(), 0.5_f32.to_bits());
    }
}

#[test]
fn camera_v4_combines_players_and_preserves_one_shot_pose() {
    let mut world = World::new();
    world.init_players(2);
    let mut script = TriggerScript::new(9);
    add_value(&mut script, 1, VarType::Bool, TriggerValue::Bool(false));
    add_value(&mut script, 2, VarType::Bool, TriggerValue::Bool(true));
    add_value(&mut script, 3, VarType::Bool, TriggerValue::Bool(false));
    add_value(
        &mut script,
        4,
        VarType::Vector,
        TriggerValue::Vector(crate::trigger::value::Vec3::new(10.0, 20.0, 30.0)),
    );
    add_value(
        &mut script,
        5,
        VarType::Vector,
        TriggerValue::Vector(crate::trigger::value::Vec3::new(1.0, 8.0, 0.0)),
    );
    add_value(&mut script, 6, VarType::Player, TriggerValue::Player(2));
    add_value(
        &mut script,
        7,
        VarType::PlayerList,
        TriggerValue::PlayerList(vec![1, 2]),
    );
    add_value(&mut script, 10, VarType::Float, TriggerValue::Float(4.0));
    let mut effect = Effect::new(1, EffectType::SetCamera);
    effect.version = 4;
    for slot in [1_u16, 2, 3, 4, 5, 6, 7, 10] {
        effect = effect.with_input_at(slot, u32::from(slot));
    }

    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    for player_id in [1, 2] {
        let state = world.player_presentation_state(player_id);
        assert!(!state.camera_controls.scroll);
        assert!(state.camera_controls.yaw);
        assert!(!state.camera_controls.zoom);
        let directive = state.camera_directive.unwrap();
        assert_eq!(directive.location, Some(Vec3::new(10.0, 20.0, 30.0)));
        assert_eq!(directive.direction, Some(Vec3::X));
        assert_eq!(directive.hover_height_offset, Some(4.0));
    }
}

fn add_value(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}
