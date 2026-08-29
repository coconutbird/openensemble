//! Authoritative squad ownership transfer.

use super::World;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, PopulationCost};
use std::collections::BTreeSet;

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
    /// one of the transferred members. Retail tower-wall associations carry
    /// the ownership change to the linked destination tower.
    pub fn change_squad_owner(&mut self, squad_id: EntityId, new_owner: PlayerId) -> bool {
        if self.get_player(new_owner).is_none() {
            return false;
        }
        self.change_squad_owner_linked(squad_id, new_owner, &mut BTreeSet::new())
    }

    fn change_squad_owner_linked(
        &mut self,
        squad_id: EntityId,
        new_owner: PlayerId,
        visited: &mut BTreeSet<EntityId>,
    ) -> bool {
        let Some(squad) = self.squads.get(squad_id) else {
            return false;
        };
        if squad.base.player_id == new_owner {
            return true;
        }
        if !visited.insert(squad_id) {
            return true;
        }
        let linked_tower = squad.associated_wall_towers().first().copied();
        self.change_single_squad_owner(squad_id, new_owner);
        if let Some(linked_tower) = linked_tower {
            let _linked_changed = self.change_squad_owner_linked(linked_tower, new_owner, visited);
        }
        true
    }

    fn change_single_squad_owner(&mut self, squad_id: EntityId, new_owner: PlayerId) {
        let (old_owner, squad_costs, unit_ids) = self.squads.get(squad_id).map_or_else(
            || unreachable!("validated squad disappeared during ownership transfer"),
            |squad| {
                (
                    squad.base.player_id,
                    squad.population_costs.clone(),
                    squad.unit_ids.clone(),
                )
            },
        );

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squad_with_member(world: &mut World, player_id: PlayerId) -> (EntityId, EntityId) {
        let squad_id = world.create_squad(player_id);
        let unit_id = world.create_building(player_id);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        (squad_id, unit_id)
    }

    #[test]
    fn ownership_follows_the_first_associated_wall_tower() {
        let mut world = World::new();
        world.init_players(2);
        let (source, source_unit) = squad_with_member(&mut world, 1);
        let (target, target_unit) = squad_with_member(&mut world, 1);
        let (unrelated, unrelated_unit) = squad_with_member(&mut world, 1);
        world
            .get_squad_mut(source)
            .unwrap()
            .add_associated_wall_tower(target);

        assert!(world.change_squad_owner(source, 2));
        assert_eq!(world.entity_owner(source), Some(2));
        assert_eq!(world.entity_owner(source_unit), Some(2));
        assert_eq!(world.entity_owner(target), Some(2));
        assert_eq!(world.entity_owner(target_unit), Some(2));
        assert_eq!(world.entity_owner(unrelated), Some(1));
        assert_eq!(world.entity_owner(unrelated_unit), Some(1));
    }

    #[test]
    fn unchanged_source_owner_does_not_replay_the_wall_link() {
        let mut world = World::new();
        world.init_players(2);
        let (source, _) = squad_with_member(&mut world, 2);
        let (target, _) = squad_with_member(&mut world, 1);
        world
            .get_squad_mut(source)
            .unwrap()
            .add_associated_wall_tower(target);

        assert!(world.change_squad_owner(source, 2));
        assert_eq!(world.entity_owner(target), Some(1));
    }
}
