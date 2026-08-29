//! Independent retail `Down` and `IsHibernating` unit state.

use crate::gameplay::{HeroRevivalProfile, ReviveActionProfile, UnitRevivalProfile};
use crate::sync::SyncChecksum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
enum RevivePhase {
    #[default]
    Working,
    Waiting,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ReviveRuntime {
    profile: ReviveActionProfile,
    phase: RevivePhase,
    revive_remaining: f32,
    hibernate_remaining: f32,
    hibernating: bool,
    dies_at_zero: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RevivalMode {
    Mortal,
    Hero {
        profile: HeroRevivalProfile,
        down: bool,
    },
    Revive(ReviveRuntime),
}

/// Persistent revive action and hero-down state for one unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct UnitRevival {
    mode: RevivalMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DamageDisposition {
    Active,
    Incapacitated,
    Mortal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RevivalAdvance {
    pub hitpoints: f32,
    pub hero_ready: bool,
}

impl Default for UnitRevival {
    fn default() -> Self {
        Self {
            mode: RevivalMode::Mortal,
        }
    }
}

impl UnitRevival {
    pub(crate) fn configure(&mut self, profile: UnitRevivalProfile) {
        if self.profile() == Some(profile) {
            return;
        }
        self.mode = match profile {
            UnitRevivalProfile::Hero(profile) => RevivalMode::Hero {
                profile,
                down: false,
            },
            UnitRevivalProfile::Revive(profile) => RevivalMode::Revive(ReviveRuntime {
                profile,
                phase: RevivePhase::Working,
                revive_remaining: 0.0,
                hibernate_remaining: 0.0,
                hibernating: false,
                dies_at_zero: false,
            }),
        };
    }

    pub(crate) const fn profile(self) -> Option<UnitRevivalProfile> {
        match self.mode {
            RevivalMode::Mortal => None,
            RevivalMode::Hero { profile, .. } => Some(UnitRevivalProfile::Hero(profile)),
            RevivalMode::Revive(runtime) => Some(UnitRevivalProfile::Revive(runtime.profile)),
        }
    }

    pub(crate) const fn is_hero(self) -> bool {
        matches!(self.mode, RevivalMode::Hero { .. })
    }

    pub(crate) const fn is_down(self) -> bool {
        matches!(self.mode, RevivalMode::Hero { down: true, .. })
    }

    pub(crate) const fn is_hibernating(self) -> bool {
        matches!(
            self.mode,
            RevivalMode::Revive(ReviveRuntime {
                hibernating: true,
                ..
            })
        )
    }

    pub(crate) fn on_damage(
        &mut self,
        hitpoints: f32,
        maximum_hitpoints: f32,
    ) -> DamageDisposition {
        match &mut self.mode {
            RevivalMode::Mortal => mortal_disposition(hitpoints),
            RevivalMode::Hero { down, .. } => {
                if hitpoints <= 0.0 {
                    *down = true;
                    DamageDisposition::Incapacitated
                } else {
                    DamageDisposition::Active
                }
            }
            RevivalMode::Revive(runtime) => runtime.on_damage(hitpoints, maximum_hitpoints),
        }
    }

    pub(crate) fn down_hero(&mut self) -> bool {
        let RevivalMode::Hero { down, .. } = &mut self.mode else {
            return false;
        };
        *down = true;
        true
    }

    pub(crate) fn override_at_zero(&mut self, hitpoints: f32) -> bool {
        let RevivalMode::Revive(runtime) = &mut self.mode else {
            return false;
        };
        if hitpoints > 0.0 {
            return false;
        }
        runtime.dies_at_zero = true;
        true
    }

    pub(crate) const fn should_die_at_zero(self, hitpoints: f32) -> bool {
        hitpoints <= 0.0
            && matches!(
                self.mode,
                RevivalMode::Revive(ReviveRuntime {
                    dies_at_zero: true,
                    ..
                })
            )
    }

    pub(crate) fn advance(
        &mut self,
        dt: f32,
        hitpoints: f32,
        maximum_hitpoints: f32,
    ) -> RevivalAdvance {
        if !dt.is_finite() || dt <= 0.0 {
            return RevivalAdvance {
                hitpoints,
                hero_ready: false,
            };
        }
        match &mut self.mode {
            RevivalMode::Mortal => RevivalAdvance {
                hitpoints,
                hero_ready: false,
            },
            RevivalMode::Hero { profile, down } => {
                advance_hero(*profile, *down, dt, hitpoints, maximum_hitpoints)
            }
            RevivalMode::Revive(runtime) => runtime.advance(dt, hitpoints, maximum_hitpoints),
        }
    }

    pub(crate) fn finish_hero_revival(&mut self) -> bool {
        let RevivalMode::Hero { down, .. } = &mut self.mode else {
            return false;
        };
        let was_down = *down;
        *down = false;
        was_down
    }

    pub(crate) fn clear_incapacitation(&mut self) {
        match &mut self.mode {
            RevivalMode::Hero { down, .. } => *down = false,
            RevivalMode::Revive(runtime) => {
                runtime.hibernating = false;
                runtime.phase = RevivePhase::Working;
                runtime.dies_at_zero = false;
            }
            RevivalMode::Mortal => {}
        }
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        match self.mode {
            RevivalMode::Mortal => checksum.hash_u32(0),
            RevivalMode::Hero { profile, down } => {
                checksum.hash_u32(1);
                checksum.hash_f32(profile.hp_regen_time);
                checksum.hash_f32(profile.revival_distance);
                checksum.hash_f32(profile.hitpoint_threshold);
                checksum.hash_u32(u32::from(down));
            }
            RevivalMode::Revive(runtime) => runtime.hash_state(checksum),
        }
    }
}

impl ReviveRuntime {
    fn on_damage(&mut self, hitpoints: f32, maximum_hitpoints: f32) -> DamageDisposition {
        if hitpoints < maximum_hitpoints {
            self.revive_remaining = self.profile.revive_delay;
            if hitpoints <= 0.0 {
                self.hibernate_remaining = self.profile.hibernate_delay;
                self.hibernating = true;
            }
            self.phase = RevivePhase::Waiting;
        }
        if self.hibernating {
            DamageDisposition::Incapacitated
        } else {
            DamageDisposition::Active
        }
    }

    fn advance(&mut self, dt: f32, hitpoints: f32, maximum_hitpoints: f32) -> RevivalAdvance {
        let hitpoints = match self.phase {
            RevivePhase::Working => {
                (hitpoints + self.profile.revive_rate * dt).min(maximum_hitpoints)
            }
            RevivePhase::Waiting => {
                self.advance_wait(dt);
                hitpoints
            }
        };
        RevivalAdvance {
            hitpoints,
            hero_ready: false,
        }
    }

    fn advance_wait(&mut self, dt: f32) {
        if self.profile.revive_delay > 0.0 {
            self.revive_remaining -= dt;
        }
        if self.profile.hibernate_delay > 0.0 {
            self.hibernate_remaining -= dt;
        }
        let done_hibernating =
            self.hibernate_remaining <= 0.0 && self.profile.hibernate_delay > 0.0;
        if self.hibernating && done_hibernating {
            self.hibernating = false;
        }
        if self.revive_remaining <= 0.0 && done_hibernating && self.profile.revive_delay > 0.0 {
            self.phase = RevivePhase::Working;
        }
    }

    fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(2);
        checksum.hash_f32(self.profile.revive_delay);
        checksum.hash_f32(self.profile.hibernate_delay);
        checksum.hash_f32(self.profile.revive_rate);
        checksum.hash_u32(u32::from(self.phase as u8));
        checksum.hash_f32(self.revive_remaining);
        checksum.hash_f32(self.hibernate_remaining);
        checksum.hash_u32(u32::from(self.hibernating));
        checksum.hash_u32(u32::from(self.dies_at_zero));
    }
}

fn mortal_disposition(hitpoints: f32) -> DamageDisposition {
    if hitpoints <= 0.0 {
        DamageDisposition::Mortal
    } else {
        DamageDisposition::Active
    }
}

fn advance_hero(
    profile: HeroRevivalProfile,
    down: bool,
    dt: f32,
    hitpoints: f32,
    maximum_hitpoints: f32,
) -> RevivalAdvance {
    if !down {
        return RevivalAdvance {
            hitpoints,
            hero_ready: false,
        };
    }
    let percentage = if maximum_hitpoints > 0.0 {
        hitpoints / maximum_hitpoints
    } else {
        0.0
    };
    let regen_rate = if profile.hp_regen_time > 0.0 {
        maximum_hitpoints / profile.hp_regen_time
    } else {
        0.0
    };
    RevivalAdvance {
        hitpoints: (hitpoints + regen_rate * dt).min(maximum_hitpoints),
        hero_ready: percentage >= profile.hitpoint_threshold,
    }
}
