//! Persistent bomb-drop physics, terrain contact, and detonation handoff.

use super::World;
use crate::entities::BombPhase;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::{BombActionProfile, GameplayCatalog};
use crate::physics::{PhysicsBlueprintBody, PhysicsTemplate};
use glam::Vec3;
use pipeline::database::hw1::physics::PhysicsBlueprint;

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

    /// Connects the action: a unit without a body gets the body of the profile (or a box of
    /// its obstruction with the default material). The body goes at the end of the action only
    /// if the action made it from the `PhysicsReplacementInfo`.
    fn initialize_bomb_body(
        &mut self,
        unit_id: EntityId,
        profile: &BombActionProfile,
        rolls: bool,
    ) {
        let Some((has_body, half_extents)) = self.units.get(unit_id).map(|unit| {
            (
                unit.physics_object.is_some(),
                unit.obstruction_half_extents.max(Vec3::splat(0.01)),
            )
        }) else {
            return;
        };
        let release = !has_body && profile.release_physics_on_completion();
        let Some(unit) = self.units.get_mut(unit_id) else {
            return;
        };
        if !unit.begin_bomb_action(profile.action_name(), rolls, release) {
            return;
        }
        if !has_body {
            let template = profile.physics_body().cloned().unwrap_or_else(|| {
                let body = PhysicsBlueprintBody::with_box(
                    "bomb",
                    &PhysicsBlueprint::default(),
                    half_extents,
                );
                std::sync::Arc::new(PhysicsTemplate::simple("bomb", body, Vec3::ZERO))
            });
            let _created = self.create_physics_object(unit_id, &template, false);
        }
        if rolls {
            // Retail multiplies 20 by an uninitialized `mDir`. Keep the branch
            // deterministic without inventing a direction absent from source.
            let _set = self.set_unit_body_linear_velocity(unit_id, Vec3::ZERO);
        }
    }

    fn update_bomb_action(&mut self, unit_id: EntityId, gameplay: &GameplayCatalog) {
        let Some((landed, rolls, collided, position)) = self.units.get(unit_id).map(|unit| {
            (
                unit.physics_object
                    .as_ref()
                    .is_some_and(|state| state.contacts.landed),
                unit.bomb_rolls(),
                unit.bomb_has_collided(),
                unit.base.position,
            )
        }) else {
            return;
        };
        let collided = if landed {
            self.units
                .get_mut(unit_id)
                .is_some_and(crate::entities::Unit::notify_bomb_ground_collision)
        } else {
            collided
        };
        if landed && !rolls {
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
        let Some(release) = self
            .units
            .get_mut(unit_id)
            .and_then(crate::entities::Unit::finish_bomb_action)
        else {
            return;
        };
        if release {
            self.release_physics_object(unit_id);
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.base.velocity = Vec3::ZERO;
            }
        }
    }
}

#[cfg(test)]
mod tests;
