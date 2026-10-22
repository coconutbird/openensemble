//! Parsed trigger script files that the engine can launch during the simulation.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{TriggerScript, TriggerValue};

/// File extension of a retail trigger script file.
const SCRIPT_EXTENSION: &str = ".triggerscript";

/// Parsed trigger script files, keyed by their lowercase file name.
///
/// Retail `BTriggerManager_createScriptFromFile` reads a script from the preloaded XML files
/// (`preloadxmlfiles.xml`) or from `data\triggerscripts`. The simulation must not read files in
/// the deterministic tick. Thus the scenario loader parses every script that a launch can ask
/// for before the first tick, and the engine launches copies of these scripts.
#[derive(Debug, Clone, Default)]
pub struct TriggerScriptLibrary {
    /// `None` marks a file with a `TriggerSystem` root that the loader could not parse. Retail
    /// gives such a file a script ID and then deletes the script.
    scripts: BTreeMap<String, Option<Arc<TriggerScript>>>,
}

/// The library state of one trigger script file.
#[derive(Debug, Clone, Copy)]
pub enum LibraryScript<'a> {
    /// The file is not in the library. Retail does not give it a script ID.
    Missing,
    /// The file has a `TriggerSystem` root, but the loader could not parse it.
    Invalid,
    /// The parsed script.
    Parsed(&'a TriggerScript),
}

impl TriggerScriptLibrary {
    /// Add a parsed script for a file name. The name match is case-insensitive.
    pub fn insert(&mut self, file_name: &str, script: TriggerScript) {
        self.scripts
            .insert(library_key(file_name), Some(Arc::new(script)));
    }

    /// Record a file with a `TriggerSystem` root that the loader could not parse.
    pub fn insert_invalid(&mut self, file_name: &str) {
        self.scripts.insert(library_key(file_name), None);
    }

    /// Return whether the library has an entry for a file name.
    #[must_use]
    pub fn contains(&self, file_name: &str) -> bool {
        self.scripts.contains_key(&library_key(file_name))
    }

    /// Look up a file name.
    #[must_use]
    pub fn get(&self, file_name: &str) -> LibraryScript<'_> {
        match self.scripts.get(&library_key(file_name)) {
            None => LibraryScript::Missing,
            Some(None) => LibraryScript::Invalid,
            Some(Some(script)) => LibraryScript::Parsed(script),
        }
    }

    /// Return the number of file entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    /// Return whether the library has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
}

/// Return the trigger script file names that the string variables of a script contain.
///
/// `LaunchScript` (392) reads its file name from a string variable. The loader uses these names
/// to parse every script that a launch can ask for before the first tick.
pub fn referenced_script_files(script: &TriggerScript) -> impl Iterator<Item = &str> {
    script
        .variables
        .values()
        .filter_map(|variable| match &variable.value {
            TriggerValue::String(value) => Some(value.as_str()),
            _ => None,
        })
        .filter(|value| is_script_file_name(value))
}

fn is_script_file_name(value: &str) -> bool {
    value.len() > SCRIPT_EXTENSION.len()
        && value
            .get(value.len() - SCRIPT_EXTENSION.len()..)
            .is_some_and(|extension| extension.eq_ignore_ascii_case(SCRIPT_EXTENSION))
}

fn library_key(file_name: &str) -> String {
    file_name.to_ascii_lowercase()
}
