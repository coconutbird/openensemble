//! Persistent bomb-drop physics, terrain contact, and detonation handoff.

use super::World;
use crate::entities::BombPhase;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{BombActionProfile, GameplayCatalog};
use crate::physics::{BoxCollider, PhysicsBody, PhysicsMaterial};
use glam::Vec3;

impl World {
    pub(super) fn update_bombs(&mut self, gameplay: &GameplayCatalog) {
        self.connect_bomb_actions(gameplay);
        let unit_ids = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| {
                (unit.bomb_phase() == BombPhase::Working).then_some(unit_id)
            })
            .collect::<Vec<_>>();
        for unit_id in unit_ids {
            self.update_bomb_action(unit_id, gameplay);
        }
    }

    fn connect_bomb_actions(&mut self, gameplay: &GameplayCatalog) {
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            let Some(profile) = self.enabled_bomb_profile(unit_id, gameplay) else {
                continue;
            };
            let rolls = self.sim_rng.range_float(0.0, 1.0) <= profile.roll_chance();
            self.initialize_bomb_body(unit_id, &profile, rolls);
        }
    }

    fn enabled_bomb_profile(
        &self,
        unit_id: EntityId,
        gameplay: &GameplayCatalog,
    ) -> Option<BombActionProfile> {
        let unit = self
            .units
            .get(unit_id)
            .filter(|unit| unit.is_alive() && unit.bomb_phase() == BombPhase::Inactive)?;
        gameplay
            .bomb_actions(&unit.proto_object_name)
            .iter()
            .find(|profile| self.bomb_action_enabled(unit_id, profile))
            .cloned()
    }

    fn bomb_action_enabled(&self, unit_id: EntityId, profile: &BombActionProfile) -> bool {
        let Some(unit) = self.units.get(unit_id) else {
            return false;
        };
        let authored_enabled = !profile.starts_disabled();
        let player_enabled =
            self.get_player(unit.base.player_id)
                .map_or(authored_enabled, |player| {
                    player.technologies.action_enabled(
                        &unit.proto_object_name,
                        profile.action_name(),
                        authored_enabled,
                    )
                });
        unit.actions
            .is_enabled(profile.action_name(), !player_enabled)
    }

    fn initialize_bomb_body(
        &mut self,
        unit_id: EntityId,
        profile: &BombActionProfile,
        rolls: bool,
    ) {
        let Some((position, fallback_material, fallback_collider, original_physics)) =
            self.units.get(unit_id).map(|unit| {
                let fallback_collider = unit.physics.as_ref().map_or_else(
                    || {
                        BoxCollider::new(
                            unit.obstruction_half_extents.max(Vec3::splat(0.01)),
                            Vec3::ZERO,
                        )
                    },
                    PhysicsBody::collider,
                );
                (
                    unit.base.position,
                    unit.physics
                        .as_ref()
                        .map_or_else(PhysicsMaterial::default, PhysicsBody::material),
                    fallback_collider,
                    profile
                        .release_physics_on_completion()
                        .then(|| unit.physics.clone())
                        .flatten(),
                )
            })
        else {
            return;
        };
        let ground_height = self.terrain_height(position, true).unwrap_or_default();
        let (material, collider) = profile
            .physics_body()
            .map_or((fallback_material, fallback_collider), |body| {
                (body.material(), body.collider())
            });
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        if !unit.begin_bomb_action(
            profile.action_name(),
            rolls,
            profile.release_physics_on_completion(),
            original_physics,
        ) {
            return;
        }
        unit.physics = Some(PhysicsBody::dynamic_replacement(
            material,
            collider,
            ground_height,
            position.y,
        ));
        if rolls {
            // Retail multiplies 20 by an uninitialized `mDir`. Keep the branch
            // deterministic without inventing a direction absent from source.
            let (physics, base) = (&mut unit.physics, &mut unit.base);
            let _set = physics
                .as_mut()
                .is_some_and(|body| body.set_linear_velocity(base, Vec3::ZERO));
        }
    }

    fn update_bomb_action(&mut self, unit_id: EntityId, gameplay: &GameplayCatalog) {
        let Some((impact_speed, rolls, collided, position)) = self.units.get(unit_id).map(|unit| {
            (
                unit.physics
                    .as_ref()
                    .map_or(0.0, PhysicsBody::ground_impact_speed_this_step),
                unit.bomb_rolls(),
                unit.bomb_has_collided(),
                unit.base.position,
            )
        }) else {
            return;
        };
        let collided = if impact_speed > 0.0 {
            self.units
                .get_mut(unit_id)
                .is_some_and(crate::entities::Unit::notify_bomb_ground_collision)
        } else {
            collided
        };
        if impact_speed > 0.0 && !rolls {
            if !self.force_active_unit_detonation(unit_id, gameplay) {
                let _killed = self.kill_unit(unit_id, false);
            }
            return;
        }
        let ground_height = self.terrain_height(position, true).unwrap_or_default();
        if collided && position.y - ground_height < 1.0 {
            self.finish_bomb_physics(unit_id);
        }
    }

    fn finish_bomb_physics(&mut self, unit_id: EntityId) {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        let Some((restore_physics, original_physics)) = unit.finish_bomb_action() else {
            return;
        };
        if restore_physics {
            unit.physics = original_physics;
            unit.base.velocity = Vec3::ZERO;
        }
    }
}

#[cfg(test)]
mod tests;
