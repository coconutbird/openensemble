//! Authoritative towing and trailer relationships.

use super::World;
use crate::entity::Entity;
use crate::{EntityId, PlayerId};
use thiserror::Error;

/// Failure to accept a player-authored hitch lifecycle command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HitchError {
    /// The towing squad no longer exists.
    #[error("towing squad does not exist")]
    SquadNotFound,
    /// The command player does not own the towing squad.
    #[error("towing squad is not owned by the command player")]
    NotOwned,
    /// The towing squad cannot accept work.
    #[error("towing squad cannot execute a hitch order")]
    SquadUnavailable,
    /// The target does not resolve to a distinct live squad.
    #[error("hitch target is not a live trailer squad")]
    InvalidTarget,
    /// Either side is already participating in a hitch relationship.
    #[error("a squad is already hitched")]
    AlreadyHitched,
    /// The towing squad has no attached trailer.
    #[error("towing squad has no hitched trailer")]
    NotHitched,
}

impl World {
    /// Attach a trailer squad to one player-owned towing squad.
    ///
    /// `target` may identify either the trailer squad or one of its member
    /// units, matching the retail work-order target convention.
    ///
    /// # Errors
    ///
    /// Returns an error when either squad is unavailable, ownership is wrong,
    /// the target is invalid, or either side is already hitched.
    pub fn issue_hitch_order(
        &mut self,
        player_id: PlayerId,
        towing_squad_id: EntityId,
        target: EntityId,
    ) -> Result<EntityId, HitchError> {
        self.validate_towing_squad(player_id, towing_squad_id)?;
        let trailer_squad_id = self
            .resolve_hitch_target(target)
            .filter(|target_id| *target_id != towing_squad_id)
            .ok_or(HitchError::InvalidTarget)?;
        let towing = self
            .squads
            .get(towing_squad_id)
            .ok_or(HitchError::SquadNotFound)?;
        let trailer = self
            .squads
            .get(trailer_squad_id)
            .ok_or(HitchError::InvalidTarget)?;
        if towing.towing_partner.is_some()
            || towing.trailer_partner.is_some()
            || trailer.towing_partner.is_some()
            || trailer.trailer_partner.is_some()
        {
            return Err(HitchError::AlreadyHitched);
        }

        let towing = self
            .squads
            .get_mut(towing_squad_id)
            .ok_or(HitchError::SquadNotFound)?;
        towing.clear_attack_order();
        towing.stop();
        towing.trailer_partner = Some(trailer_squad_id);

        let trailer = self
            .squads
            .get_mut(trailer_squad_id)
            .ok_or(HitchError::InvalidTarget)?;
        trailer.clear_attack_order();
        trailer.stop();
        trailer.towing_partner = Some(towing_squad_id);
        Ok(trailer_squad_id)
    }

    /// Detach the trailer currently attached to a player-owned towing squad.
    ///
    /// An invalid target means "detach the current trailer". A concrete unit
    /// or squad target must resolve to that same trailer.
    ///
    /// # Errors
    ///
    /// Returns an error when the towing squad is unavailable or unowned, has
    /// no trailer, or the concrete target does not identify that trailer.
    pub fn issue_unhitch_order(
        &mut self,
        player_id: PlayerId,
        towing_squad_id: EntityId,
        target: EntityId,
    ) -> Result<EntityId, HitchError> {
        self.validate_towing_squad(player_id, towing_squad_id)?;
        let trailer_squad_id = self
            .squads
            .get(towing_squad_id)
            .and_then(|squad| squad.trailer_partner)
            .ok_or(HitchError::NotHitched)?;
        if !target.is_invalid() && self.resolve_hitch_target(target) != Some(trailer_squad_id) {
            return Err(HitchError::InvalidTarget);
        }
        self.detach_squad_hitch(towing_squad_id);
        Ok(trailer_squad_id)
    }

    pub(super) fn detach_squad_hitch(&mut self, squad_id: EntityId) {
        let Some((towing_id, trailer_id)) = self
            .squads
            .get(squad_id)
            .map(|squad| (squad.towing_partner, squad.trailer_partner))
        else {
            return;
        };
        if let Some(towing_id) = towing_id
            && let Some(towing) = self.squads.get_mut(towing_id)
            && towing.trailer_partner == Some(squad_id)
        {
            towing.trailer_partner = None;
        }
        if let Some(trailer_id) = trailer_id
            && let Some(trailer) = self.squads.get_mut(trailer_id)
            && trailer.towing_partner == Some(squad_id)
        {
            trailer.towing_partner = None;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.towing_partner = None;
            squad.trailer_partner = None;
        }
    }

    fn validate_towing_squad(
        &self,
        player_id: PlayerId,
        squad_id: EntityId,
    ) -> Result<(), HitchError> {
        let squad = self.squads.get(squad_id).ok_or(HitchError::SquadNotFound)?;
        if squad.base.player_id != player_id {
            return Err(HitchError::NotOwned);
        }
        if !squad.is_alive() || squad.garrison.is_garrisoned() {
            return Err(HitchError::SquadUnavailable);
        }
        Ok(())
    }

    fn resolve_hitch_target(&self, target: EntityId) -> Option<EntityId> {
        if self
            .squads
            .get(target)
            .is_some_and(|squad| squad.is_alive() && !squad.garrison.is_garrisoned())
        {
            return Some(target);
        }
        self.units
            .get(target)
            .and_then(|unit| unit.squad_id)
            .filter(|squad_id| {
                self.squads
                    .get(*squad_id)
                    .is_some_and(|squad| squad.is_alive() && !squad.garrison.is_garrisoned())
            })
    }
}

#[cfg(test)]
mod tests;
