//! Authoritative session settings queried by scenario triggers.

use super::World;
use std::collections::BTreeSet;

const VETERANCY_CONFIG: &str = "veterancy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VeterancySetting {
    Disabled,
    Enabled,
}

impl World {
    /// Return whether this is a campaign co-op game.
    #[must_use]
    pub fn is_coop(&self) -> bool {
        self.coop
    }

    /// Configure whether this is a campaign co-op game.
    pub fn set_coop(&mut self, coop: bool) {
        self.coop = coop;
    }

    /// Return whether the current session permits squad veterancy.
    #[must_use]
    pub fn veterancy_enabled(&self) -> bool {
        self.scenario_allows_veterancy() && self.is_config_defined(VETERANCY_CONFIG)
    }

    /// Configure the effective retail config-and-scenario veterancy gate.
    ///
    /// Scenario loaders set this before creating entities. Directly constructed
    /// worlds default to enabled so gameplay fixtures can opt out explicitly.
    pub fn set_veterancy_enabled(&mut self, enabled: bool) {
        if enabled {
            self.define_config(VETERANCY_CONFIG);
        }
        self.set_scenario_allows_veterancy(enabled);
    }

    pub(crate) fn set_scenario_allows_veterancy(&mut self, enabled: bool) {
        self.veterancy = if enabled {
            VeterancySetting::Enabled
        } else {
            VeterancySetting::Disabled
        };
    }

    pub(super) const fn scenario_allows_veterancy(&self) -> bool {
        matches!(self.veterancy, VeterancySetting::Enabled)
    }

    pub(crate) fn configure_startup_configs<'a>(
        &mut self,
        symbols: impl IntoIterator<Item = &'a str>,
    ) {
        self.config_symbols.clear();
        for symbol in symbols {
            self.define_config(symbol);
        }
    }

    /// Define a synchronized configuration symbol for trigger queries.
    pub fn define_config(&mut self, name: impl Into<String>) -> bool {
        self.config_symbols.insert(name.into().to_ascii_lowercase())
    }

    /// Remove a synchronized configuration symbol.
    pub fn undefine_config(&mut self, name: &str) -> bool {
        self.config_symbols.remove(&name.to_ascii_lowercase())
    }

    /// Return whether a retail case-insensitive configuration symbol is defined.
    #[must_use]
    pub fn is_config_defined(&self, name: &str) -> bool {
        self.config_symbols.contains(&name.to_ascii_lowercase())
    }

    pub(super) fn config_symbols(&self) -> impl Iterator<Item = &str> {
        self.config_symbols.iter().map(String::as_str)
    }
}

pub(super) fn default_config_symbols() -> BTreeSet<String> {
    BTreeSet::from([VETERANCY_CONFIG.to_owned()])
}

#[cfg(test)]
mod tests {
    use super::World;

    #[test]
    fn veterancy_requires_both_case_insensitive_config_and_scenario_permission() {
        let mut world = World::new();
        assert!(world.is_config_defined("VeTeRaNcY"));
        assert!(world.veterancy_enabled());
        let enabled_checksum = world.checksum();

        assert!(world.undefine_config("VETERANCY"));
        assert!(!world.veterancy_enabled());
        assert_ne!(world.checksum(), enabled_checksum);

        assert!(world.define_config("Veterancy"));
        assert!(world.veterancy_enabled());
        assert_eq!(world.checksum(), enabled_checksum);

        world.set_scenario_allows_veterancy(false);
        assert!(!world.veterancy_enabled());
        assert!(!world.define_config("veterancy"));
        assert!(!world.veterancy_enabled());
        assert_ne!(world.checksum(), enabled_checksum);
    }
}
