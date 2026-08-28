//! Retail idle-action lifecycle reconciliation.

use num_traits::ToPrimitive;
use std::collections::BTreeSet;

use super::World;
use crate::entities::SquadState;

impl World {
    pub(super) fn update_idle_actions(&mut self, elapsed_seconds: f32) {
        let Some(elapsed_ms) = (elapsed_seconds * 1_000.0).round_ties_even().to_u32() else {
            return;
        };
        for (_, squad) in self.squads.iter_mut() {
            squad.reconcile_idle_action(elapsed_ms);
        }
        let busy_squads = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| (squad.state != SquadState::Idle).then_some(squad_id))
            .collect::<BTreeSet<_>>();
        for (_, unit) in self.units.iter_mut() {
            let parent_is_idle = unit
                .squad_id
                .is_none_or(|squad_id| !busy_squads.contains(&squad_id));
            unit.reconcile_idle_action(elapsed_ms, parent_is_idle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn world_creates_advances_and_cancels_idle_actions_once_per_update() {
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad(1);
        let unit_id = world.create_unit(1);
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        let initial_checksum = world.checksum();

        assert!(!world.get_squad(squad_id).unwrap().has_idle_action());
        assert!(!world.get_unit(unit_id).unwrap().has_idle_action());
        world.update_entities(0.05);
        assert!(world.get_squad(squad_id).unwrap().has_idle_action());
        assert!(world.get_unit(unit_id).unwrap().has_idle_action());
        assert_eq!(world.get_squad(squad_id).unwrap().idle_duration(), 0);
        assert_eq!(world.get_unit(unit_id).unwrap().idle_duration(), 0);
        assert_ne!(world.checksum(), initial_checksum);

        let active_checksum = world.checksum();
        world.update_entities(0.05);
        assert_eq!(world.get_squad(squad_id).unwrap().idle_duration(), 50);
        assert_eq!(world.get_unit(unit_id).unwrap().idle_duration(), 50);
        assert_ne!(world.checksum(), active_checksum);

        world
            .get_squad_mut(squad_id)
            .unwrap()
            .move_to(Vec3::new(10.0, 0.0, 0.0));
        assert!(!world.get_squad(squad_id).unwrap().has_idle_action());
        world.update_entities(0.05);
        assert!(!world.get_unit(unit_id).unwrap().has_idle_action());
    }
}
