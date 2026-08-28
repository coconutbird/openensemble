//! Authoritative retail power-menu entries owned by one player.

use super::Player;
use crate::EntityId;
use crate::sync::SyncChecksum;

/// Runtime index into the scenario-layered `powers.xml` table.
pub type ProtoPowerId = i32;

/// One source or charge within a player's power entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerEntryItem {
    squad_id: EntityId,
    uses_remaining: i32,
    times_used: i32,
    charge_cap: i32,
    next_grant_time: u32,
    infinite_uses: bool,
    recharging: bool,
}

impl Default for PowerEntryItem {
    fn default() -> Self {
        Self {
            squad_id: EntityId::INVALID,
            uses_remaining: 0,
            times_used: 0,
            charge_cap: 0,
            next_grant_time: 0,
            infinite_uses: false,
            recharging: false,
        }
    }
}

impl PowerEntryItem {
    /// Squad that supplied this power, or [`EntityId::INVALID`] for a global entry.
    #[must_use]
    pub fn squad_id(&self) -> EntityId {
        self.squad_id
    }

    /// Finite uses currently available on this item.
    #[must_use]
    pub fn uses_remaining(&self) -> i32 {
        self.uses_remaining
    }

    /// Number of completed casts attributed to this item.
    #[must_use]
    pub fn times_used(&self) -> i32 {
        self.times_used
    }

    /// Maximum sequentially rechargeable charges on this item.
    #[must_use]
    pub fn charge_cap(&self) -> i32 {
        self.charge_cap
    }

    /// Absolute simulation time at which the next charge becomes available.
    #[must_use]
    pub fn next_grant_time(&self) -> u32 {
        self.next_grant_time
    }

    /// Whether this item can be used without consuming a finite charge.
    #[must_use]
    pub fn has_infinite_uses(&self) -> bool {
        self.infinite_uses
    }

    /// Whether this item is waiting for its next automatic charge.
    #[must_use]
    pub fn is_recharging(&self) -> bool {
        self.recharging
    }

    fn is_available(&self) -> bool {
        self.infinite_uses || self.uses_remaining > 0
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.squad_id.as_u32());
        checksum.hash_i32(self.uses_remaining);
        checksum.hash_i32(self.times_used);
        checksum.hash_i32(self.charge_cap);
        checksum.hash_u32(self.next_grant_time);
        checksum.hash_u32(u32::from(self.infinite_uses));
        checksum.hash_u32(u32::from(self.recharging));
    }
}

/// One power-menu entry, potentially backed by several squads or charges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerEntry {
    items: Vec<PowerEntryItem>,
    proto_power_id: ProtoPowerId,
    times_used: i32,
    icon_location: i32,
    ignore_cost: bool,
    ignore_tech_prerequisites: bool,
    ignore_population: bool,
}

impl PowerEntry {
    fn new(grant: PowerGrant) -> Self {
        Self {
            items: Vec::new(),
            proto_power_id: grant.proto_power_id,
            times_used: 0,
            icon_location: grant.icon_location,
            ignore_cost: grant.ignore_cost,
            ignore_tech_prerequisites: grant.ignore_tech_prerequisites,
            ignore_population: grant.ignore_population,
        }
    }

    /// Runtime `powers.xml` index for this entry.
    #[must_use]
    pub fn proto_power_id(&self) -> ProtoPowerId {
        self.proto_power_id
    }

    /// Ordered backing squads or charge items.
    #[must_use]
    pub fn items(&self) -> &[PowerEntryItem] {
        &self.items
    }

    /// Total number of completed casts for this power.
    #[must_use]
    pub fn times_used(&self) -> i32 {
        self.times_used
    }

    /// Explicit menu slot, or `-1` to use slots authored by the power definition.
    #[must_use]
    pub fn icon_location(&self) -> i32 {
        self.icon_location
    }

    /// Whether casts bypass their authored resource cost.
    #[must_use]
    pub fn ignores_cost(&self) -> bool {
        self.ignore_cost
    }

    /// Whether casts bypass authored technology prerequisites.
    #[must_use]
    pub fn ignores_tech_prerequisites(&self) -> bool {
        self.ignore_tech_prerequisites
    }

    /// Whether casts bypass authored population requirements.
    #[must_use]
    pub fn ignores_population(&self) -> bool {
        self.ignore_population
    }

    /// Whether any backing item has an available finite or infinite use.
    #[must_use]
    pub fn has_available_uses(&self) -> bool {
        self.items.iter().any(PowerEntryItem::is_available)
    }

    /// Sum of positive finite charges across every backing item.
    #[must_use]
    pub fn finite_uses_remaining(&self) -> i32 {
        self.items.iter().fold(0, |total, item| {
            total.saturating_add(item.uses_remaining.max(0))
        })
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_i32(self.proto_power_id);
        checksum.hash_i32(self.times_used);
        checksum.hash_i32(self.icon_location);
        checksum.hash_u32(u32::from(self.ignore_cost));
        checksum.hash_u32(u32::from(self.ignore_tech_prerequisites));
        checksum.hash_u32(u32::from(self.ignore_population));
        checksum.hash_u32(u32::try_from(self.items.len()).unwrap_or(u32::MAX));
        for item in &self.items {
            item.hash_state(checksum);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct PlayerPowerState {
    entries: Vec<PowerEntry>,
}

/// Database flags needed to reproduce retail entry mutation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PowerRules {
    pub infinite_uses: bool,
    pub multi_recharge: bool,
    pub sequential_recharge: bool,
}

/// Fully resolved inputs for one retail power grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PowerGrant {
    pub proto_power_id: ProtoPowerId,
    pub squad_id: EntityId,
    pub uses: i32,
    pub icon_location: i32,
    pub ignore_cost: bool,
    pub ignore_tech_prerequisites: bool,
    pub ignore_population: bool,
}

impl Player {
    /// Ordered power-menu state owned by this player.
    #[must_use]
    pub fn power_entries(&self) -> &[PowerEntry] {
        &self.powers.entries
    }

    /// Find one power-menu entry by its scenario-layered database ID.
    #[must_use]
    pub fn power_entry(&self, proto_power_id: ProtoPowerId) -> Option<&PowerEntry> {
        self.powers
            .entries
            .iter()
            .find(|entry| entry.proto_power_id == proto_power_id)
    }

    /// Whether the requested power currently has at least one usable charge.
    #[must_use]
    pub fn has_available_power_uses(&self, proto_power_id: ProtoPowerId) -> bool {
        self.power_entry(proto_power_id)
            .is_some_and(PowerEntry::has_available_uses)
    }

    pub(crate) fn grant_power(
        &mut self,
        grant: PowerGrant,
        rules: PowerRules,
        implicit_icon_matches: impl Fn(ProtoPowerId, i32) -> bool,
    ) {
        if grant.icon_location > -1 {
            self.remove_power_at_icon_location(grant.icon_location, implicit_icon_matches);
        }
        self.revoke_power(grant.proto_power_id, grant.squad_id, rules);
        self.add_power(grant, rules);
    }

    pub(crate) fn revoke_power(
        &mut self,
        proto_power_id: ProtoPowerId,
        squad_id: EntityId,
        rules: PowerRules,
    ) {
        let Some(entry_index) = self
            .powers
            .entries
            .iter()
            .position(|entry| entry.proto_power_id == proto_power_id)
        else {
            return;
        };
        if rules.multi_recharge {
            let entry = &mut self.powers.entries[entry_index];
            if let Some(item_index) = longest_recharge_item(&entry.items) {
                entry.items.remove(item_index);
            }
        } else if squad_id.is_invalid() {
            self.powers.entries.remove(entry_index);
        } else {
            let entry = &mut self.powers.entries[entry_index];
            if let Some(item_index) = entry
                .items
                .iter()
                .position(|item| item.squad_id == squad_id)
            {
                entry.items.remove(item_index);
            }
        }
    }

    pub(crate) fn revoke_first_power_from_squad(&mut self, squad_id: EntityId) {
        for entry in &mut self.powers.entries {
            if let Some(item_index) = entry
                .items
                .iter()
                .position(|item| item.squad_id == squad_id)
            {
                entry.items.remove(item_index);
                return;
            }
        }
    }

    pub(crate) fn hash_power_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.powers.entries.len()).unwrap_or(u32::MAX));
        for entry in &self.powers.entries {
            entry.hash_state(checksum);
        }
    }

    fn remove_power_at_icon_location(
        &mut self,
        icon_location: i32,
        implicit_icon_matches: impl Fn(ProtoPowerId, i32) -> bool,
    ) {
        let matching_index = self.powers.entries.iter().position(|entry| {
            if entry.icon_location == -1 {
                implicit_icon_matches(entry.proto_power_id, icon_location)
            } else {
                entry.icon_location == icon_location
            }
        });
        if let Some(index) = matching_index {
            self.powers.entries.remove(index);
        }
    }

    fn add_power(&mut self, grant: PowerGrant, rules: PowerRules) {
        let entry_index = self
            .powers
            .entries
            .iter()
            .position(|entry| entry.proto_power_id == grant.proto_power_id)
            .unwrap_or_else(|| {
                self.powers.entries.push(PowerEntry::new(grant));
                self.powers.entries.len() - 1
            });
        let entry = &mut self.powers.entries[entry_index];
        if rules.multi_recharge {
            for _ in 0..grant.uses {
                entry.items.push(PowerEntryItem {
                    uses_remaining: 1,
                    ..PowerEntryItem::default()
                });
            }
            return;
        }

        let item_index = entry
            .items
            .iter()
            .position(|item| item.squad_id == grant.squad_id)
            .unwrap_or_else(|| {
                entry.items.push(PowerEntryItem {
                    squad_id: grant.squad_id,
                    ..PowerEntryItem::default()
                });
                entry.items.len() - 1
            });
        let item = &mut entry.items[item_index];
        if rules.sequential_recharge {
            item.charge_cap = item.charge_cap.wrapping_add(grant.uses);
        } else {
            item.charge_cap = 1;
        }
        if !item.infinite_uses && rules.infinite_uses {
            item.infinite_uses = true;
        }
        if !item.infinite_uses && !rules.sequential_recharge {
            item.uses_remaining = item.uses_remaining.wrapping_add(grant.uses);
        }
        if item.recharging && (item.infinite_uses || item.uses_remaining >= item.charge_cap) {
            item.recharging = false;
        }
    }
}

fn longest_recharge_item(items: &[PowerEntryItem]) -> Option<usize> {
    let mut selected = None;
    let mut selected_recharging = false;
    let mut selected_next_grant_time = 0;
    for (index, item) in items.iter().enumerate() {
        if selected.is_none()
            || (!selected_recharging && item.recharging)
            || (item.recharging && item.next_grant_time > selected_next_grant_time)
        {
            selected = Some(index);
            selected_recharging = item.recharging;
            selected_next_grant_time = item.next_grant_time;
        }
    }
    selected
}

#[cfg(test)]
mod tests;
