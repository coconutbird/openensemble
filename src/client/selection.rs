//! The unit selection of the local player.
//!
//! Retail keeps the selection in `BSelectionManager` (`user+368`). The selection is unit-based:
//! `BSelectionManager_selectUnit` (`0x140389550`) adds a unit and then every unit of its parent
//! squad, so a selection always holds whole squads. `BSelectionManager_rebuildSelectedSquads`
//! (`0x14038B9A0`) derives the squad list from the unit list after every change.

use sim::{EntityId, SquadMode, World};

/// The sort key of a squad without a squad entity (`0x7FFFFFFF` in retail).
const MISSING_SQUAD_SORT_KEY: i32 = i32::MAX;

/// The selected units and the squads that they belong to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    /// The selected units in selection order (`sel+0`).
    units: Vec<EntityId>,
    /// The squads of `units`, sorted like retail (`sel+48`).
    squads: Vec<EntityId>,
    /// The building whose command menu is open (`BuildingMenuOnSelect`, `user+10876`).
    menu_building: Option<EntityId>,
}

impl Selection {
    /// Returns the selected squads in the retail sub-select order.
    pub(crate) fn squads(&self) -> &[EntityId] {
        &self.squads
    }

    /// Returns the selected units in selection order.
    pub(crate) fn units(&self) -> &[EntityId] {
        &self.units
    }

    /// Returns the building whose command menu is open.
    pub(crate) fn menu_building(&self) -> Option<EntityId> {
        self.menu_building
    }

    /// Returns `true` if no unit is selected.
    pub(crate) fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Removes all units and closes the building menu (`BSelectionManager_clearSelections`,
    /// `0x1403894D0`).
    pub(crate) fn clear(&mut self) {
        self.units.clear();
        self.squads.clear();
        self.menu_building = None;
    }

    /// Removes the units that are dead or gone, then derives the squad list.
    ///
    /// Like `BSelectionManager_rebuildSelectedSquads`, the squads come in the order of their
    /// first selected unit, then an exchange sort orders them by `sort_key` (the protosquad
    /// `SubSelectSort`, `protosquad+0xE0`), plus 1 for a squad in mode 7 (`Ability`). Equal keys
    /// keep the exchange-sort order of retail, which is not stable.
    pub(crate) fn rebuild_squads(&mut self, world: &World, sort_key: impl Fn(EntityId) -> i32) {
        self.units.retain(|&unit| {
            world
                .units
                .get(unit)
                .is_some_and(|entity| entity.base.is_alive())
        });
        if self
            .menu_building
            .is_some_and(|building| !self.units.contains(&building))
        {
            self.menu_building = None;
        }
        self.squads.clear();
        for &unit in &self.units {
            let Some(squad) = world.units.get(unit).and_then(|entity| entity.squad_id) else {
                continue;
            };
            if world.squads.contains(squad) && !self.squads.contains(&squad) {
                self.squads.push(squad);
            }
        }
        let key = |squad: EntityId| {
            world
                .squads
                .get(squad)
                .map_or(MISSING_SQUAD_SORT_KEY, |entity| {
                    let key = sort_key(squad);
                    if entity.mode == SquadMode::Ability {
                        key.saturating_add(1)
                    } else {
                        key
                    }
                })
        };
        exchange_sort(&mut self.squads, key);
    }
}

/// Sorts `items` like the retail exchange sort: each slot `i` swaps with every later slot `j`
/// whose key is smaller.
fn exchange_sort(items: &mut [EntityId], key: impl Fn(EntityId) -> i32) {
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            if key(items[i]) > key(items[j]) {
                items.swap(i, j);
            }
        }
    }
}
