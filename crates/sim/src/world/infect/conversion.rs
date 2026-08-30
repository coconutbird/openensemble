//! Victim-side death, prototype transformation, and final ownership transfer.

use super::{World, infection_mapping};
use crate::entities::{InfectionPhase, SquadMode, UnitKind};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::player::{GAIA_PLAYER, PlayerId};
use crate::scenario::{
    PlacedUnitKind, classify_proto_object, configure_unit_from_proto,
    create_empty_squad_from_prototype, refresh_squad_member_settings,
};
use glam::Vec3;
use pipeline::database::hw1::Database;

#[derive(Debug, Clone)]
struct ConversionRequest {
    infection_player_id: PlayerId,
    old_squad_id: EntityId,
    position: Vec3,
    forward: Vec3,
    infected_proto_name: String,
    infected_proto_index: usize,
    infected_squad_name: String,
}

impl World {
    pub(super) fn resolve_infection_lifecycles(&mut self, database: &Database) {
        let transforming = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (unit.infection_phase() == InfectionPhase::Transforming).then_some(id)
            })
            .collect::<Vec<_>>();
        for unit_id in transforming {
            self.finish_infection_transform(unit_id);
        }
        let marked = self
            .units
            .iter()
            .filter_map(|(id, unit)| {
                (unit.is_alive() && unit.infection_phase() == InfectionPhase::Marked).then_some(id)
            })
            .collect::<Vec<_>>();
        for unit_id in marked {
            self.resolve_marked_infection(unit_id, database);
        }
    }

    fn finish_infection_transform(&mut self, unit_id: EntityId) {
        let Some((squad_id, player_id)) = self
            .units
            .get(unit_id)
            .and_then(|unit| Some((unit.squad_id?, unit.infection_player_id()?)))
        else {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.clear_infection_victim();
            }
            return;
        };
        let _changed = self.change_squad_owner(squad_id, player_id);
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.clear_infection_victim();
        }
    }

    fn resolve_marked_infection(&mut self, unit_id: EntityId, database: &Database) {
        let Some(request) = self.infection_conversion_request(unit_id, database) else {
            self.kill_failed_infection(unit_id);
            return;
        };
        let in_cover = self
            .squads
            .get(request.old_squad_id)
            .is_some_and(|squad| squad.mode == SquadMode::Cover);
        if in_cover {
            self.kill_failed_infection(unit_id);
            return;
        }
        self.execute_infection_conversion(unit_id, &request, database);
    }

    fn infection_conversion_request(
        &self,
        unit_id: EntityId,
        database: &Database,
    ) -> Option<ConversionRequest> {
        let unit = self.units.get(unit_id)?;
        let infection_player_id = unit.infection_player_id()?;
        self.get_player(infection_player_id)?;
        let old_squad_id = unit.squad_id?;
        let mapping = infection_mapping(database, unit)?;
        let (infected_proto_index, infected_proto) = database
            .objects
            .iter()
            .enumerate()
            .find(|(_, prototype)| prototype.name.eq_ignore_ascii_case(&mapping.infected))?;
        (classify_proto_object(infected_proto) == Some(PlacedUnitKind::Mobile)).then_some(())?;
        database
            .squads
            .iter()
            .any(|squad| squad.name.eq_ignore_ascii_case(&mapping.infected_squad))
            .then_some(())?;
        Some(ConversionRequest {
            infection_player_id,
            old_squad_id,
            position: unit.base.position,
            forward: unit.base.forward,
            infected_proto_name: infected_proto.name.clone(),
            infected_proto_index,
            infected_squad_name: mapping.infected_squad.clone(),
        })
    }

    fn execute_infection_conversion(
        &mut self,
        unit_id: EntityId,
        request: &ConversionRequest,
        database: &Database,
    ) {
        self.disconnect_unit_infect_action(unit_id);
        self.remove_owned_attachments(unit_id);
        let _detached = self.detach_unit_from_squad(unit_id);
        self.remove_squad_if_empty(request.old_squad_id);
        let infected_squad_id = create_empty_squad_from_prototype(
            self,
            GAIA_PLAYER,
            request.position,
            request.forward,
            &request.infected_squad_name,
            database,
        );
        if !self.change_unit_owner(unit_id, GAIA_PLAYER) {
            let _removed = self.remove_squad(infected_squad_id);
            self.kill_failed_infection(unit_id);
            return;
        }
        let now_ms = self.game_time_ms;
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.reset_for_prototype_transform(UnitKind::Mobile, now_ms);
        }
        configure_unit_from_proto(
            self,
            unit_id,
            &request.infected_proto_name,
            request.infected_proto_index,
            &database.objects[request.infected_proto_index],
        );
        if !self.attach_unit_to_squad(unit_id, infected_squad_id) {
            let _removed = self.remove_squad(infected_squad_id);
            self.kill_failed_infection(unit_id);
            return;
        }
        refresh_squad_member_settings(self, infected_squad_id);
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.begin_infection_transform(request.infection_player_id);
        }
    }

    fn remove_squad_if_empty(&mut self, squad_id: EntityId) {
        if self
            .squads
            .get(squad_id)
            .is_some_and(|squad| squad.unit_ids.is_empty())
        {
            let _removed = self.remove_squad(squad_id);
        }
    }

    fn kill_failed_infection(&mut self, unit_id: EntityId) {
        self.disconnect_unit_infect_action(unit_id);
        if let Some(unit) = self.units.get_mut(unit_id) {
            unit.clear_infection_victim();
        }
        let _killed = self.kill_unit(unit_id, false);
    }
}
