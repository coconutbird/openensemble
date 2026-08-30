//! Formation targets for retail ground-unit squad movement.

use super::World;
use crate::EntityId;
use crate::entities::squads::formation_offset_to_world;
use crate::entity::Entity;
use glam::Vec3;

const ACTION_COMPLETE_EPSILON: f32 = 0.1;
const DIRECTION_EPSILON_SQUARED: f32 = 0.000_001;

#[derive(Debug, Clone)]
struct SquadGroundMoveSnapshot {
    id: EntityId,
    position: Vec3,
    forward: Vec3,
    interim_target: Option<Vec3>,
    action_velocity: f32,
    enabled: bool,
    unit_ids: Vec<EntityId>,
}

impl World {
    pub(in crate::world) fn prepare_squad_ground_moves(&mut self) {
        self.snap_ground_squad_heights();
        let snapshots = self
            .squads
            .iter()
            .map(|(id, squad)| SquadGroundMoveSnapshot {
                id,
                position: squad.base.position,
                forward: squad.base.forward,
                interim_target: squad.move_target,
                action_velocity: planar(squad.base.velocity).length(),
                enabled: squad.is_alive()
                    && squad.base.is_mobile()
                    && !squad.garrison.is_garrisoned()
                    && !squad.is_cryo_frozen()
                    && !squad.is_being_pulled()
                    && !squad.is_jumping(),
                unit_ids: squad.unit_ids.clone(),
            })
            .collect::<Vec<_>>();
        for snapshot in snapshots {
            self.prepare_squad_ground_move(&snapshot);
        }
    }

    pub(in crate::world) fn snap_squad_ground_move_units_to_terrain(&mut self) {
        let heights = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                if unit.ground_move_owns_squad_transform()
                    && !unit.flying
                    && !unit.is_physics_driven()
                    && !unit.uses_move_air()
                {
                    self.terrain_height(unit.base.position, true)
                        .map(|height| (unit_id, height))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (unit_id, height) in heights {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.base.position.y = height;
                unit.base.velocity.y = 0.0;
            }
        }
    }

    fn prepare_squad_ground_move(&mut self, snapshot: &SquadGroundMoveSnapshot) {
        let action_forward =
            interim_forward(snapshot.position, snapshot.forward, snapshot.interim_target);
        for &unit_id in &snapshot.unit_ids {
            let Some(unit) = self.units.get_mut(unit_id) else {
                continue;
            };
            if !snapshot.enabled
                || unit.squad_id != Some(snapshot.id)
                || unit.is_building()
                || !unit.base.is_mobile()
                || !unit.is_operational()
                || unit.is_garrisoned()
                || unit.is_thrown()
                || unit.is_undergoing_infection()
                || unit.is_cryo_frozen()
                || unit.flying
                || unit.is_physics_driven()
                || unit.uses_move_air()
                || unit.is_jumping()
            {
                unit.cancel_squad_ground_move();
                continue;
            }
            let target = child_target(snapshot, action_forward, unit.formation_offset);
            let distance = planar(target - unit.base.position).length();
            let squad_action_working = snapshot.interim_target.is_some();
            if squad_action_working
                || unit.ground_move_owns_squad_transform()
                || distance >= ACTION_COMPLETE_EPSILON
            {
                unit.prepare_squad_ground_move(
                    target,
                    snapshot.action_velocity,
                    squad_action_working,
                );
            } else {
                unit.cancel_squad_ground_move();
            }
        }
    }

    fn snap_ground_squad_heights(&mut self) {
        let heights = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                let has_ground_member = squad.unit_ids.iter().any(|&unit_id| {
                    self.units.get(unit_id).is_some_and(|unit| {
                        !unit.flying && !unit.is_physics_driven() && !unit.uses_move_air()
                    })
                });
                if has_ground_member {
                    self.terrain_height(squad.base.position, true)
                        .map(|height| (squad_id, height))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (squad_id, height) in heights {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.base.position.y = height;
                squad.base.velocity.y = 0.0;
            }
        }
    }
}

fn child_target(
    snapshot: &SquadGroundMoveSnapshot,
    action_forward: Vec3,
    formation_offset: Vec3,
) -> Vec3 {
    let origin = snapshot.interim_target.unwrap_or(snapshot.position);
    origin + formation_offset_to_world(action_forward, formation_offset)
}

fn interim_forward(position: Vec3, fallback: Vec3, target: Option<Vec3>) -> Vec3 {
    let direction = target.map_or(Vec3::ZERO, |target| planar(target - position));
    if direction.length_squared() > DIRECTION_EPSILON_SQUARED {
        direction.normalize()
    } else {
        normalized_planar_or(fallback, Vec3::Z)
    }
}

fn normalized_planar_or(vector: Vec3, fallback: Vec3) -> Vec3 {
    let direction = planar(vector).normalize_or_zero();
    if direction == Vec3::ZERO {
        planar(fallback).normalize_or_zero()
    } else {
        direction
    }
}

fn planar(vector: Vec3) -> Vec3 {
    Vec3::new(vector.x, 0.0, vector.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GroundMovePhase, World};

    fn moving_marine_like_world() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let unit_id = world.create_unit_at(1, Vec3::new(-1.25, 0.0, -1.25));
        {
            let squad = world.get_squad_mut(squad_id).unwrap();
            squad.base.set_forward(Vec3::Z);
            squad.speed = 10.0;
            squad.acceleration = 26.0;
            squad.turn_rate_degrees = 540.0;
        }
        {
            let unit = world.get_unit_mut(unit_id).unwrap();
            unit.base.set_forward(Vec3::Z);
            unit.speed = 10.0;
            unit.acceleration = 26.0;
            unit.turn_rate_degrees = 540.0;
        }
        assert!(world.attach_unit_to_squad(unit_id, squad_id));
        assert!(world.set_squad_member_formation_offset(unit_id, Vec3::new(-1.25, 0.0, -1.25),));
        world
            .get_squad_mut(squad_id)
            .unwrap()
            .move_to(Vec3::X * 30.0);
        (world, squad_id, unit_id)
    }

    #[test]
    fn member_owns_source_transformed_interim_target_without_snapping() {
        let (mut world, squad_id, unit_id) = moving_marine_like_world();
        world.update_entities(0.05);

        let squad = world.get_squad(squad_id).unwrap();
        let unit = world.get_unit(unit_id).unwrap();
        let target = unit.ground_move_target().unwrap();
        let action_forward =
            interim_forward(squad.base.position, squad.base.forward, squad.move_target);
        let expected_target = squad.move_target.unwrap()
            + formation_offset_to_world(action_forward, unit.formation_offset);
        assert_eq!(target, expected_target);
        assert_eq!(unit.ground_move_phase(), GroundMovePhase::Working);
        assert!(unit.has_active_move_action());
        let snapped = squad.base.position
            + formation_offset_to_world(squad.base.forward, unit.formation_offset);
        assert_ne!(unit.base.position, snapped);
        assert!(unit.base.velocity.length() > 0.0);
    }

    #[test]
    fn member_finishes_at_formation_target_deterministically() {
        let (mut first, first_squad, first_unit) = moving_marine_like_world();
        let (mut second, second_squad, second_unit) = moving_marine_like_world();
        for _ in 0..400 {
            first.update_entities(0.05);
            second.update_entities(0.05);
        }

        let squad = first.get_squad(first_squad).unwrap();
        let unit = first.get_unit(first_unit).unwrap();
        let expected = squad.base.position
            + formation_offset_to_world(squad.base.forward, unit.formation_offset);
        assert_eq!(squad.base.position, Vec3::X * 30.0);
        assert_eq!(unit.base.position, expected);
        assert_eq!(unit.ground_move_phase(), GroundMovePhase::Inactive);
        assert_eq!(unit.base.velocity, Vec3::ZERO);
        assert_eq!(first.checksum(), second.checksum());
        assert_eq!(
            second.get_unit(second_unit).unwrap().base.position,
            unit.base.position
        );
        assert_eq!(
            second.get_squad(second_squad).unwrap().base.position,
            squad.base.position
        );
    }

    #[test]
    fn squad_arrival_uses_planar_distance_across_terrain_height_changes() {
        let mut world = World::new();
        let squad_id = world.create_squad_at(1, Vec3::new(0.0, 8.0, 0.0));
        let target = Vec3::new(1.0, 3.0, 0.0);
        let squad = world.get_squad_mut(squad_id).unwrap();
        squad.speed = 10.0;
        squad.acceleration = 0.0;
        squad.move_to(target);

        assert!(squad.update_movement(0.2));
        assert_eq!(squad.base.position, target);
        assert_eq!(squad.base.velocity, Vec3::ZERO);
    }
}
