//! Authoritative tower-wall links and target-facing transforms.

use super::World;
use crate::entities::TowerWallAction;
use crate::entity_id::EntityId;
use glam::Vec3;

impl World {
    /// Assign a source tower action to a destination squad.
    ///
    /// Action availability is validated against layered tactic data by the
    /// caller. This method applies the synchronous state changes made by
    /// retail's `BUnitActionTowerWall::setTarget` and its entity reference.
    pub(crate) fn set_tower_wall_destination(
        &mut self,
        source_squad_id: EntityId,
        target_squad_id: EntityId,
    ) -> bool {
        let Some((source_unit_id, source_position)) = self.tower_leader(source_squad_id) else {
            return false;
        };
        let Some((target_unit_id, target_position)) = self.tower_leader(target_squad_id) else {
            return false;
        };
        let source_to_target = planar_direction(target_position - source_position);
        let target_to_source = planar_direction(source_position - target_position);
        if source_to_target == Vec3::ZERO || target_to_source == Vec3::ZERO {
            return false;
        }

        let Some(source_unit) = self.units.get_mut(source_unit_id) else {
            return false;
        };
        source_unit.base.set_forward(target_to_source);
        source_unit.tower_wall = Some(TowerWallAction::new(
            target_squad_id,
            source_position,
            target_position,
        ));
        let Some(target_unit) = self.units.get_mut(target_unit_id) else {
            return false;
        };
        target_unit.base.set_forward(source_to_target);

        if let Some(source_squad) = self.squads.get_mut(source_squad_id) {
            source_squad.base.set_forward(target_to_source);
            source_squad.add_associated_wall_tower(target_squad_id);
        }
        if let Some(target_squad) = self.squads.get_mut(target_squad_id) {
            target_squad.base.set_forward(source_to_target);
        }
        true
    }

    fn tower_leader(&self, squad_id: EntityId) -> Option<(EntityId, Vec3)> {
        let squad = self.squads.get(squad_id)?;
        let unit_id = *squad.unit_ids.first()?;
        let unit = self.units.get(unit_id)?;
        Some((unit_id, unit.base.position))
    }
}

fn planar_direction(direction: Vec3) -> Vec3 {
    Vec3::new(direction.x, 0.0, direction.z).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_state_orients_leaders_away_and_preserves_entity_ref_order() {
        let mut world = World::new();
        let source = world.create_squad_at(0, Vec3::new(10.0, 2.0, 20.0));
        let source_unit = world.create_building_at(0, Vec3::new(10.0, 2.0, 20.0));
        assert!(world.attach_unit_to_squad(source_unit, source));
        let target = world.create_squad_at(0, Vec3::new(40.0, 4.0, 60.0));
        let target_unit = world.create_building_at(0, Vec3::new(40.0, 4.0, 60.0));
        assert!(world.attach_unit_to_squad(target_unit, target));

        assert!(world.set_tower_wall_destination(source, target));
        let source_unit = world.get_unit(source_unit).unwrap();
        let target_unit = world.get_unit(target_unit).unwrap();
        let expected = Vec3::new(0.6, 0.0, 0.8);
        assert!(
            source_unit
                .base
                .forward
                .abs_diff_eq(-expected, f32::EPSILON)
        );
        assert!(target_unit.base.forward.abs_diff_eq(expected, f32::EPSILON));
        let action = source_unit.tower_wall.expect("source action state");
        assert_eq!(action.target_squad_id(), target);
        assert_eq!(action.beam_start_position(), Vec3::new(10.0, 2.0, 20.0));
        assert_eq!(action.beam_end_position(), Vec3::new(40.0, 4.0, 60.0));
        assert_eq!(
            world.get_squad(source).unwrap().associated_wall_towers(),
            &[target]
        );

        assert!(world.set_tower_wall_destination(source, target));
        assert_eq!(
            world.get_squad(source).unwrap().associated_wall_towers(),
            &[target, target]
        );
    }
}
