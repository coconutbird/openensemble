//! Runtime state owned by persistent projectile Dodge and Deflect actions.

use super::Unit;
use crate::entity_id::EntityId;
use crate::sync::SyncChecksum;
use num_traits::ToPrimitive;

#[derive(Debug, Clone)]
pub(crate) struct UnitProjectileDefense {
    dodgee_id: Option<EntityId>,
    dodge_cooldown_done_ms: u32,
    dodge_init_time_ms: u32,
    deflect_cooldown_done_ms: u32,
    deflected_damage: f32,
    last_damage_time_ms: u32,
    deflecting: bool,
}

impl Default for UnitProjectileDefense {
    fn default() -> Self {
        Self {
            dodgee_id: None,
            dodge_cooldown_done_ms: 0,
            dodge_init_time_ms: 0,
            deflect_cooldown_done_ms: 0,
            deflected_damage: 0.0,
            last_damage_time_ms: 0,
            deflecting: true,
        }
    }
}

impl UnitProjectileDefense {
    pub(crate) fn can_begin_dodge(&mut self, now_ms: u32) -> bool {
        if self.dodgee_id.is_some() && now_ms < self.dodge_cooldown_done_ms {
            return false;
        }
        self.dodgee_id = None;
        true
    }

    pub(crate) fn begin_dodge(&mut self, projectile_id: EntityId, now_ms: u32, cooldown: f32) {
        self.dodgee_id = Some(projectile_id);
        self.dodge_init_time_ms = now_ms;
        self.dodge_cooldown_done_ms = now_ms.wrapping_add(seconds_to_millis(cooldown));
    }

    pub(crate) const fn dodge_cooldown_active(&self, now_ms: u32) -> bool {
        self.dodgee_id.is_some() && now_ms < self.dodge_cooldown_done_ms
    }

    pub(crate) const fn deflect_cooldown_active(&self, now_ms: u32) -> bool {
        now_ms < self.deflect_cooldown_done_ms
    }

    pub(crate) const fn can_begin_deflect(&self, now_ms: u32) -> bool {
        self.deflecting && !self.deflect_cooldown_active(now_ms)
    }

    pub(crate) fn refresh_deflecting(
        &mut self,
        now_ms: u32,
        squad_last_damage_ms: u32,
        regen_delay_ms: u32,
        has_shield_visual: bool,
    ) {
        if self.deflecting || !has_shield_visual {
            return;
        }
        let last_damage = self.last_damage_time_ms.max(squad_last_damage_ms);
        if last_damage == 0 || now_ms.wrapping_sub(last_damage) > regen_delay_ms {
            self.deflecting = true;
        }
    }

    pub(crate) fn try_deflect(
        &mut self,
        now_ms: u32,
        cooldown: f32,
        damage: f32,
        maximum_damage: f32,
    ) -> bool {
        if !self.deflecting || self.deflect_cooldown_active(now_ms) {
            return false;
        }
        self.deflect_cooldown_done_ms = now_ms.wrapping_add(seconds_to_millis(cooldown));
        self.deflected_damage += damage.max(0.0);
        if maximum_damage > 0.0 && self.deflected_damage > maximum_damage {
            self.deflecting = false;
            return false;
        }
        true
    }

    pub(crate) fn notify_damaged(&mut self, now_ms: u32) {
        self.last_damage_time_ms = now_ms;
    }

    pub(crate) fn hash_state(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(self.dodgee_id.map_or(u32::MAX, EntityId::as_u32));
        checksum.hash_u32(self.dodge_cooldown_done_ms);
        checksum.hash_u32(self.dodge_init_time_ms);
        checksum.hash_u32(self.deflect_cooldown_done_ms);
        checksum.hash_f32(self.deflected_damage);
        checksum.hash_u32(self.last_damage_time_ms);
        checksum.hash_u32(u32::from(self.deflecting));
    }
}

impl Unit {
    pub(crate) fn reset_projectile_defense(&mut self) {
        self.projectile_defense = UnitProjectileDefense::default();
    }

    pub(crate) fn notify_projectile_defense_damaged(&mut self, now_ms: u32) {
        self.projectile_defense.notify_damaged(now_ms);
    }

    pub(crate) fn hash_projectile_defense_state(&self, checksum: &mut SyncChecksum) {
        self.projectile_defense.hash_state(checksum);
    }
}

fn seconds_to_millis(seconds: f32) -> u32 {
    if seconds.is_finite() && seconds > 0.0 {
        (seconds * 1_000.0).to_u32().unwrap_or(u32::MAX)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_id::EntityClass;

    #[test]
    fn cooldowns_and_damage_threshold_follow_retail_strict_comparisons() {
        let projectile_id = EntityId::new(EntityClass::Projectile, 1);
        let mut state = UnitProjectileDefense::default();
        state.begin_dodge(projectile_id, 1_000, 0.5);
        assert!(!state.can_begin_dodge(1_499));
        assert!(state.can_begin_dodge(1_500));

        assert!(state.try_deflect(2_000, 0.25, 10.0, 10.0));
        assert!(!state.try_deflect(2_100, 0.25, 1.0, 10.0));
        assert!(!state.try_deflect(2_250, 0.25, 1.0, 10.0));
        state.notify_damaged(2_250);
        state.refresh_deflecting(2_751, 0, 500, true);
        assert!(!state.try_deflect(2_751, 0.0, 0.0, 10.0));
    }
}
