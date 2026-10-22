//! Attack-order context that survives automatic squad-mode transitions.

use super::World;
use crate::entities::SquadMode;
use crate::gameplay::GameplayCatalog;
use crate::{EntityId, PlayerId};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SquadAttackContext {
    pub(crate) range: f32,
    pub(crate) squad_mode: Option<SquadMode>,
    pub(crate) ability_id: Option<u8>,
}

impl World {
    /// Assign an attack while retaining the mode transition requested by an ability.
    pub(crate) fn issue_attack_order_with_mode_automation(
        &mut self,
        player_id: PlayerId,
        recipient_id: EntityId,
        requested_target_id: EntityId,
        context: SquadAttackContext,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let mode_order = self.requested_mode_order(
            recipient_id,
            context.squad_mode,
            context.ability_id,
            gameplay,
        );
        let direct_mode = context
            .squad_mode
            .filter(|mode| *mode != SquadMode::Lockdown);
        let accepted = self.issue_attack_order_with_context(
            player_id,
            recipient_id,
            requested_target_id,
            context.range,
            direct_mode,
            context.ability_id,
        );
        if accepted && let Some(squad) = self.squads.get_mut(recipient_id) {
            squad.set_attack_mode_order(mode_order);
            self.update_squad_mode_order_transition(recipient_id, gameplay);
        }
        accepted
    }
}
