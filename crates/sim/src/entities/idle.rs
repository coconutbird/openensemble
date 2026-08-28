//! Shared runtime state for retail's `BEntityActionIdle`.

use crate::sync::SyncChecksum;

/// Presence and elapsed time of an entity's current idle action.
///
/// Retail creates the action at the end of the first otherwise-idle entity
/// update. Its duration is initially zero and advances on later updates until
/// another action conflicts with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct EntityIdle {
    active: bool,
    duration_ms: u32,
}

impl EntityIdle {
    pub(crate) const fn is_active(self) -> bool {
        self.active
    }

    pub(crate) const fn duration_ms(self) -> u32 {
        self.duration_ms
    }

    pub(crate) fn cancel(&mut self) {
        self.active = false;
        self.duration_ms = 0;
    }

    pub(crate) fn reconcile(&mut self, should_be_idle: bool, elapsed_ms: u32) {
        if !should_be_idle {
            self.cancel();
        } else if self.active {
            self.duration_ms = self.duration_ms.wrapping_add(elapsed_ms);
        } else {
            self.active = true;
            self.duration_ms = 0;
        }
    }

    pub(crate) fn hash_state(self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::from(self.active));
        checksum.hash_u32(self.duration_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_starts_at_zero_advances_and_restarts_after_a_conflict() {
        let mut idle = EntityIdle::default();

        idle.reconcile(true, 50);
        assert!(idle.is_active());
        assert_eq!(idle.duration_ms(), 0);

        idle.reconcile(true, 50);
        assert_eq!(idle.duration_ms(), 50);
        idle.reconcile(false, 50);
        assert!(!idle.is_active());
        assert_eq!(idle.duration_ms(), 0);

        idle.reconcile(true, 50);
        assert!(idle.is_active());
        assert_eq!(idle.duration_ms(), 0);
    }
}
