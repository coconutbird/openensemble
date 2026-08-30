//! Unit-side targeting state owned by a squad `JumpPull` flight.

use super::Unit;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum JumpPullTargetState {
    #[default]
    Attackable,
    Untargetable,
}

impl Unit {
    pub(crate) const fn is_jump_pull_untargetable(&self) -> bool {
        matches!(
            self.jump_pull_target_state,
            JumpPullTargetState::Untargetable
        )
    }

    pub(crate) fn set_jump_pull_untargetable(&mut self, untargetable: bool) {
        self.jump_pull_target_state = if untargetable {
            JumpPullTargetState::Untargetable
        } else {
            JumpPullTargetState::Attackable
        };
    }
}
