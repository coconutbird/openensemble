//! Authoritative squad ownership transfer.

use super::World;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, PopulationCost};

#[derive(Debug, Clone)]
struct UnitOwnershipCost {
    unit_id: EntityId,
    old_owner: PlayerId,
    population: Vec<PopulationCost>,
    population_cap: Vec<PopulationCost>,
    built: bool,
}

impl World {
    /// Transfer a squad and all of its member units to an existing player.
    ///
    /// Live population and built population-cap contributions move with the
    /// entities. A base record follows its anchor building when that anchor is
    /// one of the transferred members.
    pub fn change_squad_owner(&mut self, squad_id: EntityId, new_owner: PlayerId) -> bool {
        if self.get_player(new_owner).is_none() {
            return false;
        }
        let Some((old_owner, squad_costs, unit_ids)) = self.squads.get(squad_id).map(|squad| {
            (
                squad.base.player_id,
                squad.population_costs.clone(),
                squad.unit_ids.clone(),
            )
        }) else {
            return false;
        };
        if old_owner == new_owner {
            return true;
        }

        let unit_costs = unit_ids
            .iter()
            .filter_map(|unit_id| {
                self.units.get(*unit_id).map(|unit| UnitOwnershipCost {
                    unit_id: *unit_id,
                    old_owner: unit.base.player_id,
                    population: unit.population_costs.clone(),
                    population_cap: unit.population_cap_additions.clone(),
                    built: unit.built,
                })
            })
            .collect::<Vec<_>>();

        if let Some(player) = self.get_player_mut(old_owner) {
            player.release_population(&squad_costs);
        }
        for cost in &unit_costs {
            if let Some(player) = self.get_player_mut(cost.old_owner) {
                player.release_population(&cost.population);
                if cost.built {
                    player.adjust_population_cap(&cost.population_cap, false);
                }
            }
        }
        if let Some(player) = self.get_player_mut(new_owner) {
            player.add_population(&squad_costs);
            for cost in &unit_costs {
                player.add_population(&cost.population);
                if cost.built {
                    player.adjust_population_cap(&cost.population_cap, true);
                }
            }
        }

        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.base.player_id = new_owner;
        }
        for cost in &unit_costs {
            if let Some(unit) = self.units.get_mut(cost.unit_id) {
                unit.base.player_id = new_owner;
            }
        }
        for base in self.bases.values_mut() {
            if unit_ids.contains(&base.anchor_building_id) {
                base.player_id = new_owner;
            }
        }
        true
    }
}
