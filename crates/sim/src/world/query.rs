//! Deterministic retail-style live entity selection.

use super::World;
use crate::entity::Entity;
use crate::{EntityId, PlayerId};
use glam::Vec3;

impl World {
    /// Select live class-1 units using the filters exposed by retail
    /// `GetUnits` v3.
    ///
    /// An area is a circular X/Z query. The Y coordinate is deliberately
    /// ignored, matching `BUnitQuery` point-radius queries.
    #[must_use]
    pub fn find_live_units(
        &self,
        player_id: Option<PlayerId>,
        object_type: Option<&str>,
        area: Option<(Vec3, f32)>,
    ) -> Vec<EntityId> {
        let mut results: Vec<_> = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (player_id.is_none_or(|expected| unit.base.player_id == expected)
                    && object_type.is_none_or(|expected| unit.is_object_type(expected))
                    && (area.is_some() || self.get_player(unit.base.player_id).is_some())
                    && (area.is_none() || unit.is_alive())
                    && area.is_none_or(|(center, radius)| {
                        entity_is_in_area(
                            unit.base.position,
                            unit.obstruction_half_extents,
                            unit.is_building(),
                            center,
                            radius,
                        )
                    }))
                .then_some(unit_id)
            })
            .collect();
        if area.is_none() && player_id.is_none() {
            results.sort_by_key(|unit_id| {
                let owner = self
                    .units
                    .get(*unit_id)
                    .map_or(PlayerId::MAX, |unit| unit.base.player_id);
                (owner, unit_id.pool_index())
            });
        }
        if area.is_none() {
            remove_unordered(&mut results, |unit_id| {
                self.units.get(*unit_id).is_some_and(Entity::is_alive)
            });
        }
        results
    }

    /// Select live squads using the filters exposed by retail `GetSquads` v4.
    ///
    /// The optional object type must match every current child. Empty squads
    /// therefore pass this filter in a global query, as they do in retail.
    /// Area queries require a live leader/member because retail spatial queries
    /// test the squad through its leader obstruction.
    #[must_use]
    pub fn find_live_squads(
        &self,
        player_id: Option<PlayerId>,
        prototype_id: Option<i32>,
        object_type: Option<&str>,
        area: Option<(Vec3, f32)>,
    ) -> Vec<EntityId> {
        let mut results: Vec<_> = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| {
                let leader = squad
                    .unit_ids
                    .iter()
                    .find_map(|unit_id| self.units.get(*unit_id));
                let in_area = area.is_none_or(|(center, radius)| {
                    leader.is_some_and(|unit| {
                        entity_is_in_area(
                            unit.base.position,
                            unit.obstruction_half_extents,
                            unit.is_building(),
                            center,
                            radius,
                        )
                    })
                });
                let all_children_match = object_type.is_none_or(|expected| {
                    squad.unit_ids.iter().all(|unit_id| {
                        self.units
                            .get(*unit_id)
                            .is_some_and(|unit| unit.is_object_type(expected))
                    })
                });
                (player_id.is_none_or(|expected| squad.base.player_id == expected)
                    && prototype_id.is_none_or(|expected| squad.proto_squad_id == expected)
                    && (area.is_some() || self.get_player(squad.base.player_id).is_some())
                    && (area.is_none() || squad.is_alive())
                    && in_area
                    && (area.is_none() || all_children_match))
                    .then_some(squad_id)
            })
            .collect();
        if area.is_none() && player_id.is_none() {
            results.sort_by_key(|squad_id| {
                let owner = self
                    .squads
                    .get(*squad_id)
                    .map_or(PlayerId::MAX, |squad| squad.base.player_id);
                (owner, squad_id.pool_index())
            });
        }
        if area.is_none() {
            remove_unordered(&mut results, |squad_id| {
                self.squads.get(*squad_id).is_some_and(Entity::is_alive)
            });
            if let Some(expected) = object_type {
                remove_unordered(&mut results, |squad_id| {
                    self.squads.get(*squad_id).is_some_and(|squad| {
                        squad.unit_ids.iter().all(|unit_id| {
                            self.units
                                .get(*unit_id)
                                .is_some_and(|unit| unit.is_object_type(expected))
                        })
                    })
                });
            }
        }
        results
    }
}

fn remove_unordered<T>(values: &mut Vec<T>, keep: impl Fn(&T) -> bool) {
    let mut index = 0;
    while index < values.len() {
        if keep(&values[index]) {
            index += 1;
        } else {
            values.swap_remove(index);
        }
    }
}

fn entity_is_in_area(
    position: Vec3,
    half_extents: Vec3,
    uses_box_obstruction: bool,
    center: Vec3,
    radius: f32,
) -> bool {
    if !radius.is_finite() || radius < 0.0 {
        return false;
    }
    let delta = Vec3::new(
        (position.x - center.x).abs(),
        0.0,
        (position.z - center.z).abs(),
    );
    let distance = if uses_box_obstruction {
        let outside = (delta - half_extents.abs()).max(Vec3::ZERO);
        outside.x.hypot(outside.z)
    } else {
        let obstruction_radius = half_extents.x.abs().max(half_extents.z.abs());
        (delta.x.hypot(delta.z) - obstruction_radius).max(0.0)
    };
    distance <= radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::UnitState;

    #[test]
    fn unit_queries_are_live_xz_inclusive_and_player_ordered() {
        let mut world = World::new();
        world.init_players(2);
        let second_player = world.create_unit_at(2, Vec3::new(3.0, 100.0, 4.0));
        let first_player = world.create_unit_at(1, Vec3::ZERO);
        world.get_unit_mut(second_player).unwrap().proto_object_name = "Marine".to_owned();
        world.get_unit_mut(first_player).unwrap().proto_object_name = "Marine".to_owned();

        assert_eq!(
            world.find_live_units(None, Some("marine"), None),
            vec![first_player, second_player]
        );
        assert_eq!(
            world.find_live_units(None, None, Some((Vec3::ZERO, 5.0))),
            vec![second_player, first_player]
        );

        world.get_unit_mut(first_player).unwrap().state = UnitState::Dead;
        assert_eq!(world.find_live_units(None, None, None), vec![second_player]);
    }

    #[test]
    fn squad_object_type_requires_every_current_child() {
        let mut world = World::new();
        world.init_players(1);
        let matching = world.create_squad(1);
        let mixed = world.create_squad(1);
        let empty = world.create_squad(1);
        let marine = world.create_unit(1);
        let vehicle = world.create_unit(1);
        world.get_unit_mut(marine).unwrap().object_types = vec!["Infantry".to_owned()];
        world.get_unit_mut(vehicle).unwrap().object_types = vec!["Vehicle".to_owned()];
        assert!(world.attach_unit_to_squad(marine, matching));
        assert!(world.attach_unit_to_squad(vehicle, mixed));

        assert_eq!(
            world.find_live_squads(None, None, Some("Infantry"), None),
            vec![matching, empty]
        );
        assert_eq!(
            world.find_live_squads(None, None, None, Some((Vec3::ZERO, 0.0))),
            vec![matching, mixed]
        );
    }
}
