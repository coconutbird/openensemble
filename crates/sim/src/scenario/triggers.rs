//! Loading of top-level trigger systems embedded in scenario SCN files.

use super::LoadedScenario;
use crate::trigger::{
    Effect, EffectType, LoadResult, TriggerLoadContext, TriggerScript, TriggerValue, VanillaLoader,
};
use pipeline::database::hw1::Database;
use pipeline::xmb::Document;

pub(super) fn load_trigger_systems(
    scenario: &mut LoadedScenario,
    database: &Database,
    document: &Document,
) -> LoadResult<()> {
    let root = document
        .root()
        .ok_or_else(|| crate::trigger::LoadError::MissingElement("Scenario root".into()))?;
    let context = TriggerLoadContext {
        scenario_entities: Some(&scenario.scenario_id_to_entity_id),
        scenario_units: Some(&scenario.scenario_id_to_unit_id),
        database: Some(database),
    };
    let scripts = root
        .children
        .iter()
        .filter(|node| node.name == "TriggerSystem")
        .map(|node| VanillaLoader::from_node_with_context(node, context))
        .collect::<LoadResult<Vec<_>>>()?;

    let current_time = scenario.world.game_time_ms;
    for script in scripts {
        let script_id = scenario.world.trigger_engine_mut().add_script(script);
        scenario
            .world
            .trigger_engine_mut()
            .activate_script(script_id, current_time);
    }
    Ok(())
}

pub(super) fn scripted_animation_requests(scenario: &LoadedScenario) -> Vec<(String, String)> {
    scenario
        .world
        .trigger_engine()
        .scripts()
        .flat_map(|(_, script)| {
            script.triggers.iter().flat_map(|trigger| {
                trigger
                    .effects_on_true
                    .iter()
                    .chain(&trigger.effects_on_false)
                    .filter_map(|effect| scripted_animation_request(scenario, script, effect))
            })
        })
        .collect()
}

fn scripted_animation_request(
    scenario: &LoadedScenario,
    script: &TriggerScript,
    effect: &Effect,
) -> Option<(String, String)> {
    if effect.effect_type != EffectType::PlayAnimationObject {
        return None;
    }
    let entity_id = script
        .get_variable(effect.variable_id(1)?)?
        .value
        .as_entity()?;
    let animation = match &script.get_variable(effect.variable_id(2)?)?.value {
        TriggerValue::String(value) => value.trim(),
        _ => return None,
    };
    let prototype = scenario.world.entity_proto_object_name(entity_id)?;
    Some((prototype.to_owned(), animation.to_owned()))
}
