use super::super::execute;
use crate::trigger::effects::EffectOutcome;
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue, TriggerVar, VarType};
use crate::world::World;

fn add(script: &mut TriggerScript, id: u32, var_type: VarType, value: TriggerValue) {
    script.add_variable(TriggerVar::new(id, var_type).with_value(value));
}

#[test]
fn scene_cue_ids_are_typed() {
    assert_eq!(
        EffectType::from_u16(515),
        Some(EffectType::PlayWorldSoundOnEntity)
    );
    assert_eq!(EffectType::from_u16(521), Some(EffectType::SetFollowCam));
    assert_eq!(EffectType::from_u16(533), Some(EffectType::LightsetAnimate));
}

#[test]
fn follow_cam_takes_the_first_used_entity_and_mode_slot() {
    let mut world = World::new();
    let squad = world.create_squad(1);
    let unit = world.create_unit(1);
    let mut script = TriggerScript::new(1);
    add(&mut script, 1, VarType::Squad, TriggerValue::Squad(squad));
    add(&mut script, 2, VarType::Unit, TriggerValue::Unit(unit));
    add(&mut script, 3, VarType::Bool, TriggerValue::Bool(true));
    let effect = Effect::new(1, EffectType::SetFollowCam)
        .with_input_at(2, 1)
        .with_input_at(1, 2)
        .with_input_at(5, 3);
    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Presentation)
    );
    let follow = world.follow_camera().unwrap();
    assert_eq!(follow.entity, unit);
    assert!(follow.follow);
    assert!(follow.direction.is_none());
    let first_revision = follow.revision;

    let squad_only = Effect::new(1, EffectType::SetFollowCam).with_input_at(2, 1);
    let _ = execute(&squad_only, &mut script, &mut world);
    let follow = world.follow_camera().unwrap();
    assert_eq!(follow.entity, squad);
    assert!(!follow.follow);
    assert!(follow.revision > first_revision);
}

#[test]
fn world_sound_v1_rejects_squads_and_v2_reads_slots_two_four_five() {
    let mut world = World::new();
    let squad = world.create_squad(1);
    let mut script = TriggerScript::new(1);
    add(
        &mut script,
        1,
        VarType::Sound,
        TriggerValue::String("cue".to_owned()),
    );
    add(&mut script, 2, VarType::Squad, TriggerValue::Squad(squad));
    let mut effect = Effect::new(1, EffectType::PlayWorldSoundOnEntity)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    effect.version = 1;
    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Skipped)
    );
    assert_eq!(world.latest_world_sound_sequence(), 0);

    let mut effect = Effect::new(1, EffectType::PlayWorldSoundOnEntity)
        .with_input_at(1, 1)
        .with_input_at(4, 2);
    effect.version = 2;
    let _ = execute(&effect, &mut script, &mut world);
    let request = world.world_sound_requests_after(0).next().unwrap();
    assert_eq!(request.sound_cue(), "cue");
    assert_eq!(request.entity(), squad);
    assert_eq!(
        world.world_sound_requests_after(request.sequence()).count(),
        0
    );
}

#[test]
fn lightset_animate_starts_only_for_a_known_id() {
    let mut world = World::new();
    world.configure_lightsets(vec![2]);
    let mut script = TriggerScript::new(1);
    add(&mut script, 1, VarType::Integer, TriggerValue::Int(2));
    add(&mut script, 2, VarType::Time, TriggerValue::Time(3_000));
    add(&mut script, 3, VarType::Integer, TriggerValue::Int(5));
    let effect = Effect::new(1, EffectType::LightsetAnimate)
        .with_input_at(1, 3)
        .with_input_at(2, 2);
    assert_eq!(
        execute(&effect, &mut script, &mut world),
        Some(EffectOutcome::Skipped)
    );
    let effect = Effect::new(1, EffectType::LightsetAnimate)
        .with_input_at(1, 1)
        .with_input_at(2, 2);
    let _ = execute(&effect, &mut script, &mut world);
    assert_eq!(world.lightset_animation().unwrap().end_ms, 3_000);
}
