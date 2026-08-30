//! Authoritative player and unit rally-point operations.

use super::World;
use crate::entities::RallyPoint;
use crate::entity_id::EntityId;
use crate::player::{Player, PlayerId};
use glam::Vec3;

impl World {
    /// Set a player's global rally point and clear its base-specific points.
    pub fn set_player_rally_point(
        &mut self,
        player_id: PlayerId,
        position: Vec3,
        target_entity_id: Option<EntityId>,
    ) -> bool {
        let Some(rally_point) = self.validated_rally_point(position, target_entity_id) else {
            return false;
        };
        let Some(player) = self.get_player_mut(player_id) else {
            return false;
        };
        player.set_rally_point(rally_point);

        let base_anchors = self
            .bases
            .values()
            .filter(|base| base.player_id == player_id)
            .map(|base| base.anchor_building_id)
            .collect::<Vec<_>>();
        for anchor_id in base_anchors {
            if let Some(anchor) = self.units.get_mut(anchor_id) {
                anchor.clear_rally_point(player_id);
            }
        }
        true
    }

    /// Clear a player's global rally point.
    pub fn clear_player_rally_point(&mut self, player_id: PlayerId) -> bool {
        let Some(player) = self.get_player_mut(player_id) else {
            return false;
        };
        player.clear_rally_point();
        true
    }

    /// Return a player's stored global rally point.
    #[must_use]
    pub fn player_rally_point(&self, player_id: PlayerId) -> Option<RallyPoint> {
        self.get_player(player_id).and_then(Player::rally_point)
    }

    /// Set the primary or co-op rally point on a live unit.
    pub fn set_unit_rally_point(
        &mut self,
        unit_id: EntityId,
        player_id: PlayerId,
        position: Vec3,
        target_entity_id: Option<EntityId>,
    ) -> bool {
        let Some(rally_point) = self.validated_rally_point(position, target_entity_id) else {
            return false;
        };
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        unit.set_rally_point(player_id, rally_point);
        self.check_player_rally_point(player_id);
        true
    }

    /// Clear the primary or co-op rally point on a live unit.
    pub fn clear_unit_rally_point(&mut self, unit_id: EntityId, player_id: PlayerId) -> bool {
        let Some(unit) = self.units.get_mut(unit_id) else {
            return false;
        };
        unit.clear_rally_point(player_id);
        true
    }

    /// Return a unit's stored primary or co-op rally point.
    #[must_use]
    pub fn unit_rally_point(&self, unit_id: EntityId, player_id: PlayerId) -> Option<RallyPoint> {
        self.units
            .get(unit_id)
            .and_then(|unit| unit.rally_point(player_id))
    }

    /// Resolve a rally point against its current target entity position.
    #[must_use]
    pub fn resolve_rally_point(&self, rally_point: RallyPoint) -> Vec3 {
        rally_point
            .target_entity_id()
            .and_then(|entity_id| self.entity_position(entity_id))
            .unwrap_or_else(|| rally_point.position())
    }

    fn validated_rally_point(
        &self,
        position: Vec3,
        target_entity_id: Option<EntityId>,
    ) -> Option<RallyPoint> {
        if !position.is_finite() {
            return None;
        }
        let target = target_entity_id
            .filter(|entity_id| !entity_id.is_invalid())
            .and_then(|entity_id| {
                self.entity_position(entity_id)
                    .filter(|target_position| target_position.is_finite())
                    .map(|target_position| (entity_id, target_position))
            });
        Some(match target {
            Some((entity_id, target_position)) => RallyPoint::new(target_position, Some(entity_id)),
            None => RallyPoint::new(position, None),
        })
    }

    fn check_player_rally_point(&mut self, player_id: PlayerId) {
        if self.player_rally_point(player_id).is_none() {
            return;
        }
        let base_anchors = self
            .bases
            .values()
            .filter(|base| base.player_id == player_id)
            .map(|base| base.anchor_building_id)
            .collect::<Vec<_>>();
        if base_anchors.is_empty() {
            return;
        }
        let all_have_rally_points = base_anchors.iter().all(|anchor_id| {
            self.units
                .get(*anchor_id)
                .is_none_or(|anchor| anchor.rally_point(player_id).is_some())
        });
        if all_have_rally_points && let Some(player) = self.get_player_mut(player_id) {
            player.clear_rally_point();
        }
    }
}

#[cfg(test)]
mod tests;
