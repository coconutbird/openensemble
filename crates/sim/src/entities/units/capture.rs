//! Authoritative per-unit `Capture` action and target progress state.

use super::Unit;
use crate::entity_id::EntityId;
use crate::player::{PlayerId, Resources};
use crate::sync::SyncChecksum;

const CAPTURE_EPSILON: f32 = 0.000_1;

/// Runtime phase of a unit or squad capture action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CapturePhase {
    /// No capture action is connected.
    #[default]
    None,
    /// The source is approaching its target.
    Moving,
    /// The source owns the action controllers and contributes work.
    Working,
    /// The target changed to the capturing owner.
    Done,
    /// The action lost or rejected its target.
    Failed,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitCapture {
    pub(crate) action: UnitCaptureAction,
    pub(crate) target: UnitCaptureTarget,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitCaptureAction {
    target_id: Option<EntityId>,
    action_name: String,
    phase: CapturePhase,
    used_second_approach: bool,
}

impl UnitCaptureAction {
    pub(crate) fn start(&mut self, target_id: EntityId, action_name: &str) {
        self.target_id = Some(target_id);
        action_name.clone_into(&mut self.action_name);
        self.phase = CapturePhase::Moving;
        self.used_second_approach = false;
    }

    pub(crate) fn cancel(&mut self) {
        self.target_id = None;
        self.action_name.clear();
        self.phase = CapturePhase::None;
        self.used_second_approach = false;
    }

    pub(crate) fn finish(&mut self, phase: CapturePhase) {
        self.phase = phase;
    }

    pub(crate) fn set_phase(&mut self, phase: CapturePhase) {
        self.phase = phase;
    }

    pub(crate) fn begin_second_approach(&mut self) -> bool {
        if self.used_second_approach {
            return false;
        }
        self.used_second_approach = true;
        self.phase = CapturePhase::Moving;
        true
    }

    pub(crate) const fn target_id(&self) -> Option<EntityId> {
        self.target_id
    }

    pub(crate) fn action_name(&self) -> &str {
        &self.action_name
    }

    pub(crate) const fn phase(&self) -> CapturePhase {
        self.phase
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(
            self.target_id
                .map_or(EntityId::INVALID.as_u32(), EntityId::as_u32),
        );
        checksum.hash_u32(self.phase as u32);
        checksum.hash_u32(u32::from(self.used_second_approach));
        checksum.hash_u32(u32::try_from(self.action_name.len()).unwrap_or(u32::MAX));
        checksum.hash_bytes(self.action_name.as_bytes());
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CaptureUnlink {
    pub(crate) cost: Resources,
    pub(crate) removed_last_player_link: bool,
}

#[derive(Debug, Clone)]
struct CaptureLink {
    player_id: PlayerId,
    squad_ids: Vec<EntityId>,
    cost: Resources,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnitCaptureTarget {
    capturable: bool,
    maximum_points: f32,
    points: f32,
    player_id: Option<PlayerId>,
    active_unit_ids: Vec<EntityId>,
    links: Vec<CaptureLink>,
}

impl UnitCaptureTarget {
    pub(crate) fn configure(&mut self, capturable: bool, maximum_points: Option<f32>) {
        self.capturable = capturable;
        self.maximum_points = maximum_points
            .filter(|points| points.is_finite() && *points >= 0.0)
            .unwrap_or_default();
        self.points = self.points.clamp(0.0, self.maximum_points);
        if !capturable {
            self.reset_progress();
            self.active_unit_ids.clear();
        }
    }

    pub(crate) const fn is_capturable(&self) -> bool {
        self.capturable
    }

    pub(crate) const fn points(&self) -> f32 {
        self.points
    }

    pub(crate) const fn maximum_points(&self) -> f32 {
        self.maximum_points
    }

    pub(crate) fn percent(&self) -> f32 {
        if self.maximum_points <= 0.0 {
            1.0
        } else {
            (self.points / self.maximum_points).clamp(0.0, 1.0)
        }
    }

    pub(crate) const fn player_id(&self) -> Option<PlayerId> {
        self.player_id
    }

    pub(crate) fn is_being_captured(&self) -> bool {
        !self.active_unit_ids.is_empty()
    }

    pub(crate) fn active_unit_ids(&self) -> &[EntityId] {
        &self.active_unit_ids
    }

    pub(crate) fn start_unit(&mut self, unit_id: EntityId) {
        if let Err(index) = self.active_unit_ids.binary_search(&unit_id) {
            self.active_unit_ids.insert(index, unit_id);
        }
    }

    pub(crate) fn stop_unit(&mut self, unit_id: EntityId) {
        if let Ok(index) = self.active_unit_ids.binary_search(&unit_id) {
            self.active_unit_ids.remove(index);
        }
    }

    pub(crate) fn clear_activity(&mut self) {
        self.active_unit_ids.clear();
    }

    pub(crate) fn has_player_link(&self, player_id: PlayerId) -> bool {
        self.links
            .binary_search_by_key(&player_id, |link| link.player_id)
            .is_ok()
    }

    pub(crate) fn connect_squad(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
        cost: Resources,
    ) {
        match self
            .links
            .binary_search_by_key(&player_id, |link| link.player_id)
        {
            Ok(index) => insert_sorted(&mut self.links[index].squad_ids, squad_id),
            Err(index) => self.links.insert(
                index,
                CaptureLink {
                    player_id,
                    squad_ids: vec![squad_id],
                    cost,
                },
            ),
        }
    }

    pub(crate) fn disconnect_squad(
        &mut self,
        player_id: PlayerId,
        squad_id: EntityId,
    ) -> Option<CaptureUnlink> {
        let index = self
            .links
            .binary_search_by_key(&player_id, |link| link.player_id)
            .ok()?;
        let squad_index = self.links[index].squad_ids.binary_search(&squad_id).ok()?;
        self.links[index].squad_ids.remove(squad_index);
        let cost = self.links[index].cost;
        let removed_last_player_link = self.links[index].squad_ids.is_empty();
        if removed_last_player_link {
            self.links.remove(index);
        }
        Some(CaptureUnlink {
            cost,
            removed_last_player_link,
        })
    }

    pub(crate) fn linked_squads(&self) -> Vec<(PlayerId, EntityId)> {
        self.links
            .iter()
            .flat_map(|link| {
                link.squad_ids
                    .iter()
                    .copied()
                    .map(move |squad_id| (link.player_id, squad_id))
            })
            .collect()
    }

    pub(crate) fn apply_work(&mut self, player_id: PlayerId, amount: f32) -> bool {
        if !amount.is_finite() || amount <= 0.0 {
            return self.points >= self.maximum_points;
        }
        if self.points < CAPTURE_EPSILON {
            self.points = 0.0;
            self.player_id = None;
        }
        if self.player_id.is_some_and(|owner| owner != player_id) {
            self.points = (self.points - amount).max(0.0);
            if self.points < CAPTURE_EPSILON {
                self.points = 0.0;
                self.player_id = None;
            }
            return false;
        }
        self.player_id = Some(player_id);
        self.points = (self.points + amount).min(self.maximum_points);
        self.points >= self.maximum_points
    }

    pub(crate) fn decay(&mut self, amount: f32) {
        if !amount.is_finite() || amount <= 0.0 || self.is_being_captured() {
            return;
        }
        self.points = (self.points - amount).max(0.0);
        if self.points < CAPTURE_EPSILON {
            self.reset_progress();
        }
    }

    pub(crate) fn reset_progress(&mut self) {
        self.points = 0.0;
        self.player_id = None;
    }

    fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.capturable));
        checksum.hash_f32(self.maximum_points);
        checksum.hash_f32(self.points);
        checksum.hash_u32(self.player_id.map_or(u32::MAX, u32::from));
        hash_entity_ids(checksum, &self.active_unit_ids);
        checksum.hash_u32(u32::try_from(self.links.len()).unwrap_or(u32::MAX));
        for link in &self.links {
            checksum.hash_u32(u32::from(link.player_id));
            hash_entity_ids(checksum, &link.squad_ids);
            for amount in link.cost.amounts {
                checksum.hash_f32(amount);
            }
        }
    }
}

impl Unit {
    pub(crate) fn configure_capture_target(
        &mut self,
        capturable: bool,
        maximum_points: Option<f32>,
    ) {
        self.capture.target.configure(capturable, maximum_points);
    }

    /// Return whether this prototype carries retail's `Capturable` flag.
    #[must_use]
    pub const fn is_capturable(&self) -> bool {
        self.capture.target.is_capturable()
    }

    /// Return accumulated capture points on this target.
    #[must_use]
    pub const fn capture_points(&self) -> f32 {
        self.capture.target.points()
    }

    /// Return the authored points needed to capture this target.
    #[must_use]
    pub const fn maximum_capture_points(&self) -> f32 {
        self.capture.target.maximum_points()
    }

    /// Return target capture completion in the inclusive range zero to one.
    #[must_use]
    pub fn capture_percent(&self) -> f32 {
        self.capture.target.percent()
    }

    /// Return the player currently owning positive capture progress.
    #[must_use]
    pub const fn capture_player_id(&self) -> Option<PlayerId> {
        self.capture.target.player_id()
    }

    /// Return whether one or more unit capture actions currently own this target.
    #[must_use]
    pub fn is_being_captured(&self) -> bool {
        self.capture.target.is_being_captured()
    }

    /// Return this unit's current capture-action phase.
    #[must_use]
    pub const fn capture_phase(&self) -> CapturePhase {
        self.capture.action.phase()
    }

    /// Return the target of this unit's capture action.
    #[must_use]
    pub const fn capture_target(&self) -> Option<EntityId> {
        self.capture.action.target_id()
    }

    /// Return the selected authored capture action name.
    #[must_use]
    pub fn capture_action_name(&self) -> Option<&str> {
        (!self.capture.action.action_name().is_empty()).then(|| self.capture.action.action_name())
    }

    /// Return whether this unit is actively contributing capture points.
    #[must_use]
    pub const fn is_capturing(&self) -> bool {
        matches!(self.capture.action.phase(), CapturePhase::Working)
    }

    pub(crate) fn cancel_capture_action(&mut self) {
        self.capture.action.cancel();
    }

    pub(crate) fn hash_capture_state(&self, checksum: &mut SyncChecksum) {
        self.capture.action.hash_state(checksum);
        self.capture.target.hash_state(checksum);
    }
}

fn insert_sorted(values: &mut Vec<EntityId>, value: EntityId) {
    if let Err(index) = values.binary_search(&value) {
        values.insert(index, value);
    }
}

fn hash_entity_ids(checksum: &mut SyncChecksum, ids: &[EntityId]) {
    checksum.hash_u32(u32::try_from(ids.len()).unwrap_or(u32::MAX));
    for id in ids {
        checksum.hash_u32(id.as_u32());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposing_work_removes_progress_before_changing_capture_player() {
        let mut target = UnitCaptureTarget::default();
        target.configure(true, Some(10.0));
        assert!(!target.apply_work(1, 6.0));
        assert_eq!(target.player_id(), Some(1));
        assert!(!target.apply_work(2, 6.0));
        assert_eq!(target.points().to_bits(), 0.0_f32.to_bits());
        assert_eq!(target.player_id(), None);
        assert!(!target.apply_work(2, 2.0));
        assert_eq!(target.player_id(), Some(2));
    }

    #[test]
    fn capture_cost_is_shared_by_same_player_squad_links() {
        let mut target = UnitCaptureTarget::default();
        let mut cost = Resources::new();
        cost.set(0, 100.0);
        let first = EntityId::from_u32(0x0300_0001);
        let second = EntityId::from_u32(0x0300_0002);
        target.connect_squad(1, first, cost);
        target.connect_squad(1, second, Resources::new());
        let unlink = target.disconnect_squad(1, first).unwrap();
        assert!(!unlink.removed_last_player_link);
        let unlink = target.disconnect_squad(1, second).unwrap();
        assert!(unlink.removed_last_player_link);
        assert_eq!(unlink.cost, cost);
    }
}
