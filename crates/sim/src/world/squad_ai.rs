//! Retail squad-AI roster maintenance.

use super::World;
use crate::entity_id::EntityId;
use crate::player::PlayerId;

impl World {
    pub(super) fn register_squad_ai_squad(&mut self, player_id: PlayerId, squad_id: EntityId) {
        if let Some(player) = self.get_player_mut(player_id) {
            player.add_squad_ai_squad(squad_id);
        }
    }

    pub(crate) fn refresh_squad_ai_membership(&mut self, squad_id: EntityId) {
        let Some((player_id, ignore_ai)) = self.squads.get(squad_id).map(|squad| {
            let ignore_ai = squad.unit_ids.iter().any(|unit_id| {
                self.units
                    .get(*unit_id)
                    .is_some_and(crate::entities::Unit::ignores_squad_ai)
            });
            (squad.base.player_id, ignore_ai)
        }) else {
            return;
        };
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.ignore_ai = ignore_ai;
        }
        let Some(player) = self.get_player_mut(player_id) else {
            return;
        };
        if ignore_ai {
            player.remove_squad_ai_squad(squad_id);
        } else {
            player.add_squad_ai_squad(squad_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_squad_ai_members_are_removed_from_the_rotation() {
        let mut world = World::new();
        world.init_players(1);
        let ignored = world.create_squad(1);
        let eligible = world.create_squad(1);
        let member = world.create_unit(1);
        world
            .get_unit_mut(member)
            .unwrap()
            .set_ignores_squad_ai(true);
        assert!(world.attach_unit_to_squad(member, ignored));

        world.refresh_squad_ai_membership(ignored);

        assert!(world.get_squad(ignored).unwrap().ignore_ai);
        assert_eq!(
            world
                .get_player_mut(1)
                .unwrap()
                .next_secondary_turret_scan_squad(),
            Some(eligible)
        );
    }

    #[test]
    fn ownership_transfer_moves_the_squad_to_the_new_players_list_tail() {
        let mut world = World::new();
        world.init_players(2);
        let transferred = world.create_squad(1);
        let old_owner_next = world.create_squad(1);
        let new_owner_first = world.create_squad(2);
        assert_eq!(
            world
                .get_player_mut(1)
                .unwrap()
                .next_secondary_turret_scan_squad(),
            Some(transferred)
        );

        assert!(world.change_squad_owner(transferred, 2));

        assert_eq!(
            world
                .get_player_mut(1)
                .unwrap()
                .next_secondary_turret_scan_squad(),
            Some(old_owner_next)
        );
        assert_eq!(
            world
                .get_player_mut(2)
                .unwrap()
                .next_secondary_turret_scan_squad(),
            Some(new_owner_first)
        );
        assert_eq!(
            world
                .get_player_mut(2)
                .unwrap()
                .next_secondary_turret_scan_squad(),
            Some(transferred)
        );
    }
}
