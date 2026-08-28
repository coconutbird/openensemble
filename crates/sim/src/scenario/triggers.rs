//! Loading of top-level trigger systems embedded in scenario SCN files.

use super::LoadedScenario;
use crate::trigger::{LoadResult, TriggerLoadContext, VanillaLoader};
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
