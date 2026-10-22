//! Loading of scenario trigger systems and of the trigger script files that scripts launch.

use std::collections::BTreeSet;

use super::LoadedScenario;
use crate::trigger::{
    Effect, EffectType, LoadResult, TriggerExternals, TriggerLoadContext, TriggerScript,
    TriggerScriptId, TriggerValue, VanillaLoader, referenced_script_files,
};
use pipeline::database::hw1::Database;
use pipeline::source::{AssetSource, StdFileProvider};
use pipeline::xmb::Node;

/// Directory of the retail trigger script files.
const TRIGGER_SCRIPT_DIRECTORY: &str = "data\\triggerscripts\\";

/// Retail list of XML files that `BDatabase` reads before a game starts.
const PRELOAD_XML_FILES: &str = "data\\preloadxmlfiles.xml";

impl LoadedScenario {
    /// Parse a trigger script file and the script files that it can launch into the library.
    ///
    /// The engine reads the library when a script launches another script, so the tick does
    /// not read files. The file is `data\triggerscripts\` plus `file_name`, and the name match
    /// ignores case. Returns `Ok(false)` if the file does not exist or its root is not
    /// `TriggerSystem`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file of `file_name` does not parse. The library then marks the
    /// file as invalid, like a retail script that fails to load.
    pub fn load_trigger_script(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        database: &Database,
        file_name: &str,
    ) -> LoadResult<bool> {
        let loaded = load_script_file(self, source, database, file_name)?;
        if loaded {
            load_script_files(self, source, database, vec![file_name.to_owned()]);
        }
        Ok(loaded)
    }

    /// Load a trigger script file if necessary, then launch it with external inputs.
    ///
    /// This is the setup path of retail `BTriggerManager_createScriptFromFile`, the
    /// `setExternal*` calls and `BTriggerManager_activateScript`, for example for the game-mode
    /// world and player scripts. Returns the new script ID, or `None` if the file does not
    /// exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file does not parse.
    pub fn launch_trigger_script(
        &mut self,
        source: &mut AssetSource<StdFileProvider>,
        database: &Database,
        file_name: &str,
        externals: TriggerExternals,
    ) -> LoadResult<Option<TriggerScriptId>> {
        if !self.load_trigger_script(source, database, file_name)? {
            return Ok(None);
        }
        let current_time = self.world.game_time_ms;
        Ok(self
            .world
            .trigger_engine_mut()
            .launch_script(file_name, externals, current_time))
    }
}

/// Parse the trigger scripts of `preloadxmlfiles.xml` and every script file that a resident
/// script names, so that `LaunchScript` finds them during the tick.
pub(super) fn preload_trigger_scripts(
    scenario: &mut LoadedScenario,
    source: &mut AssetSource<StdFileProvider>,
    database: &Database,
) {
    let mut file_names = preloaded_trigger_scripts(source);
    for (_, script) in scenario.world.trigger_engine().scripts() {
        file_names.extend(referenced_script_files(script).map(str::to_owned));
    }
    load_script_files(scenario, source, database, file_names);
}

fn preloaded_trigger_scripts(source: &mut AssetSource<StdFileProvider>) -> Vec<String> {
    let Some(document) = source.read_xmb(PRELOAD_XML_FILES) else {
        return Vec::new();
    };
    let Some(root) = document.root() else {
        return Vec::new();
    };
    root.children
        .iter()
        .filter(|node| node.name == "File")
        .filter(|node| {
            node.get_attribute("dir")
                .is_some_and(|dir| dir.value_string().eq_ignore_ascii_case("TriggerScripts"))
        })
        .map(pipeline::xmb::Node::text_string)
        .collect()
}

/// Load script files and the files that they name. A file that does not parse stays invalid.
fn load_script_files(
    scenario: &mut LoadedScenario,
    source: &mut AssetSource<StdFileProvider>,
    database: &Database,
    mut pending: Vec<String>,
) {
    let mut visited = BTreeSet::new();
    while let Some(file_name) = pending.pop() {
        if !visited.insert(file_name.to_ascii_lowercase()) {
            continue;
        }
        let _loaded = load_script_file(scenario, source, database, &file_name);
        if let crate::trigger::LibraryScript::Parsed(script) = scenario
            .world
            .trigger_engine()
            .script_library()
            .get(&file_name)
        {
            pending.extend(referenced_script_files(script).map(str::to_owned));
        }
    }
}

/// Parse one script file into the library if the library does not have it yet.
fn load_script_file(
    scenario: &mut LoadedScenario,
    source: &mut AssetSource<StdFileProvider>,
    database: &Database,
    file_name: &str,
) -> LoadResult<bool> {
    if scenario
        .world
        .trigger_engine()
        .script_library()
        .contains(file_name)
    {
        return Ok(true);
    }
    let Some(document) = source.read_xmb(&format!("{TRIGGER_SCRIPT_DIRECTORY}{file_name}")) else {
        return Ok(false);
    };
    let Some(root) = document
        .root()
        .filter(|root| root.name.eq_ignore_ascii_case("TriggerSystem"))
    else {
        return Ok(false);
    };
    let context = TriggerLoadContext {
        scenario_entities: Some(&scenario.scenario_id_to_entity_id),
        scenario_units: Some(&scenario.scenario_id_to_unit_id),
        database: Some(database),
    };
    let parsed = VanillaLoader::from_node_with_context(root, context);
    let library = scenario.world.trigger_engine_mut().script_library_mut();
    match parsed {
        Ok(script) => {
            library.insert(file_name, script);
            Ok(true)
        }
        Err(error) => {
            library.insert_invalid(file_name);
            Err(error)
        }
    }
}

/// Create and activate one `.scn` `TriggerSystem` when the load reaches its element.
///
/// This follows `BTriggerManager_createScriptFromNode` @ `0x140487700` and
/// `BTriggerManager_activateScript` @ `0x140487800` (step `W` of `BScenario_load`).
pub(super) fn create_trigger_system(
    scenario: &mut LoadedScenario,
    database: &Database,
    node: &Node,
) -> LoadResult<TriggerScriptId> {
    let context = TriggerLoadContext {
        scenario_entities: Some(&scenario.scenario_id_to_entity_id),
        scenario_units: Some(&scenario.scenario_id_to_unit_id),
        database: Some(database),
    };
    let script = VanillaLoader::from_node_with_context(node, context)?;
    let current_time = scenario.world.game_time_ms;
    let engine = scenario.world.trigger_engine_mut();
    let script_id = engine.add_script(script);
    engine.activate_script(script_id, current_time);
    Ok(script_id)
}

pub(super) fn scripted_animation_requests(scenario: &LoadedScenario) -> Vec<(String, String)> {
    scenario
        .world
        .trigger_engine()
        .scripts()
        .flat_map(|(_, script)| {
            script.triggers.iter().flat_map(|trigger| {
                trigger
                    .effects()
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
