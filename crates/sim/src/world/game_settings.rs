//! Authoritative session settings queried by scenario triggers.

use super::World;

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
    pub const fn veterancy_enabled(&self) -> bool {
        self.veterancy_enabled
    }

    /// Configure the effective retail config-and-scenario veterancy gate.
    ///
    /// Scenario loaders set this before creating entities. Directly constructed
    /// worlds default to enabled so gameplay fixtures can opt out explicitly.
    pub fn set_veterancy_enabled(&mut self, enabled: bool) {
        self.veterancy_enabled = enabled;
    }

    /// Define a synchronized configuration symbol for trigger queries.
    pub fn define_config(&mut self, name: impl Into<String>) -> bool {
        self.config_symbols.insert(name.into())
    }

    /// Remove a synchronized configuration symbol.
    pub fn undefine_config(&mut self, name: &str) -> bool {
        self.config_symbols.remove(name)
    }

    /// Return whether an exact, case-sensitive configuration symbol is defined.
    #[must_use]
    pub fn is_config_defined(&self, name: &str) -> bool {
        self.config_symbols.contains(name)
    }

    pub(super) fn config_symbols(&self) -> impl Iterator<Item = &str> {
        self.config_symbols.iter().map(String::as_str)
    }
}
