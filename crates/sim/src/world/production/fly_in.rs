//! Source-backed aircraft and civilization-transport `FlyIn` births.

use super::World;
use super::placement::SquadBirthPlacement;
use crate::entities::SquadState;
use crate::entities::squads::{SquadTrainedAirBirth, SquadTransportPlan};
use crate::scenario::create_unit_squad_from_prototype;
use crate::{EntityId, PlayerId};
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

impl World {
    pub(super) fn start_trained_squad_fly_in(
        &mut self,
        database: &Database,
        player_id: PlayerId,
        squad_id: EntityId,
        placement: SquadBirthPlacement,
        rally_point: Option<Vec3>,
    ) -> bool {
        let flying = self
            .get_squad(squad_id)
            .and_then(|squad| squad.unit_ids.first())
            .and_then(|leader_id| self.get_unit(*leader_id))
            .is_some_and(|leader| leader.flying);
        if flying {
            self.start_trained_air_birth(
                squad_id,
                placement.position,
                placement.forward,
                rally_point,
            )
        } else {
            self.start_trained_ground_birth(
                database,
                player_id,
                squad_id,
                placement.position,
                placement.forward,
                rally_point,
            )
        }
    }

    pub(super) fn start_trained_air_birth(
        &mut self,
        squad_id: EntityId,
        landing_position: Vec3,
        landing_forward: Vec3,
        rally_point: Option<Vec3>,
    ) -> bool {
        let Some((leader_id, speed)) = self.get_squad(squad_id).and_then(|squad| {
            let leader_id = *squad.unit_ids.first()?;
            let leader = self.get_unit(leader_id)?;
            leader.flying.then_some((leader_id, leader.speed))
        }) else {
            return false;
        };
        let forward = planar_forward(landing_forward);
        let action = SquadTrainedAirBirth::new(leader_id, landing_position, rally_point, speed);
        let start_position = action.current_position();
        if let Some(squad) = self.get_squad_mut(squad_id) {
            squad.remove_all_orders();
            squad.base.set_forward(forward);
            squad.state = SquadState::Working;
            squad.trained_air_birth = Some(action);
        }
        if let Some(leader) = self.get_unit_mut(leader_id) {
            leader.base.position = start_position;
            leader.base.forward = forward;
            leader.base.velocity = Vec3::ZERO;
        }
        true
    }

    pub(super) fn start_trained_ground_birth(
        &mut self,
        database: &Database,
        player_id: PlayerId,
        passenger_squad_id: EntityId,
        dropoff_position: Vec3,
        forward: Vec3,
        rally_point: Option<Vec3>,
    ) -> bool {
        let Some((transport_name, _transport)) = player_transport(self, database, player_id) else {
            return false;
        };
        let direction = planar_forward(forward);
        let settings = FlyInSettings::from_database(database);
        let start_position = dropoff_position - direction * settings.incoming_offset
            + Vec3::Y * settings.incoming_height;
        let incoming_target = dropoff_position + Vec3::Y * settings.dropoff_height;
        let outgoing_target = dropoff_position
            + direction * settings.outgoing_offset
            + Vec3::Y * settings.outgoing_height;
        let Some((carrier_id, carrier_unit_id)) = create_unit_squad_from_prototype(
            self,
            player_id,
            start_position,
            direction,
            transport_name,
            database,
        ) else {
            return false;
        };
        if let Some(carrier) = self.get_unit_mut(carrier_unit_id) {
            carrier.physics = None;
        }
        let plan = SquadTransportPlan {
            passenger_squad_ids: vec![passenger_squad_id],
            start_position,
            dropoff_position,
            incoming_target,
            outgoing_target,
            rally_point,
            attack_move: false,
            facing: Some(direction),
        };
        if self.start_transport_fly_in(carrier_id, plan) {
            return true;
        }
        let _destroyed = self.kill_squad(carrier_id, true);
        false
    }

    pub(in crate::world) fn update_trained_air_births(&mut self, dt: f32) {
        let active = self
            .squads
            .iter()
            .filter_map(|(squad_id, squad)| squad.trained_air_birth.is_some().then_some(squad_id))
            .collect::<Vec<_>>();
        for squad_id in active {
            self.update_trained_air_birth(squad_id, dt);
        }
    }

    fn update_trained_air_birth(&mut self, squad_id: EntityId, dt: f32) {
        let Some((player_id, forward, leader_id, previous, position, arrived)) =
            self.get_squad_mut(squad_id).and_then(|squad| {
                let player_id = squad.base.player_id;
                let forward = squad.base.forward;
                let action = squad.trained_air_birth.as_mut()?;
                let leader_id = action.leader_unit_id();
                let previous = action.current_position();
                let arrived = action.advance(dt);
                Some((
                    player_id,
                    forward,
                    leader_id,
                    previous,
                    action.current_position(),
                    arrived,
                ))
            })
        else {
            return;
        };
        if let Some(leader) = self.get_unit_mut(leader_id) {
            leader.base.position = position;
            leader.base.forward = forward;
            leader.base.velocity = if arrived {
                Vec3::ZERO
            } else {
                (position - previous) / dt
            };
        } else {
            self.finish_trained_air_birth(squad_id, player_id, forward, false);
            return;
        }
        if arrived {
            self.finish_trained_air_birth(squad_id, player_id, forward, true);
        }
    }

    fn finish_trained_air_birth(
        &mut self,
        squad_id: EntityId,
        player_id: PlayerId,
        forward: Vec3,
        place_members: bool,
    ) {
        let finished = self.get_squad_mut(squad_id).and_then(|squad| {
            squad.state = SquadState::Idle;
            squad.trained_air_birth.take()
        });
        let Some(finished) = finished else {
            return;
        };
        if place_members {
            self.place_squad_members(squad_id, finished.landing_position(), forward, true);
        }
        if let Some(rally_point) = finished.rally_point() {
            let _issued = self.issue_move_order(player_id, squad_id, rally_point);
        }
    }
}

struct FlyInSettings {
    incoming_height: f32,
    incoming_offset: f32,
    outgoing_height: f32,
    outgoing_offset: f32,
    dropoff_height: f32,
}

impl FlyInSettings {
    fn from_database(database: &Database) -> Self {
        let game_data = database.game_data.as_ref();
        Self {
            incoming_height: setting(
                game_data.and_then(|data| data.transport_incoming_height),
                60.0,
            ),
            incoming_offset: setting(
                game_data.and_then(|data| data.transport_incoming_offset),
                40.0,
            ),
            outgoing_height: setting(
                game_data.and_then(|data| data.transport_outgoing_height),
                60.0,
            ),
            outgoing_offset: setting(
                game_data.and_then(|data| data.transport_outgoing_offset),
                40.0,
            ),
            dropoff_height: setting(
                game_data.and_then(|data| data.transport_dropoff_height),
                12.0,
            ),
        }
    }
}

fn player_transport<'database>(
    world: &World,
    database: &'database Database,
    player_id: PlayerId,
) -> Option<(&'database str, &'database ProtoObject)> {
    let civ_id = world.get_player(player_id)?.civ_id;
    let civ = usize::try_from(civ_id)
        .ok()
        .and_then(|index| database.civs.get(index))?;
    let name = civ.transport.as_deref()?.trim();
    let prototype = database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name))?;
    Some((name, prototype))
}

fn setting(value: Option<f32>, fallback: f32) -> f32 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

fn planar_forward(forward: Vec3) -> Vec3 {
    Vec3::new(forward.x, 0.0, forward.z)
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_birth_moves_from_source_height_then_issues_rally() {
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        let leader_id = world.create_unit_at(1, Vec3::ZERO);
        let leader = world.get_unit_mut(leader_id).unwrap();
        leader.flying = true;
        leader.speed = 50.0;
        assert!(world.attach_unit_to_squad(leader_id, squad_id));

        assert!(
            world.start_trained_air_birth(squad_id, Vec3::ZERO, Vec3::Z, Some(Vec3::X * 10.0),)
        );
        assert!((world.get_unit(leader_id).unwrap().base.position.y - 100.0).abs() <= f32::EPSILON);
        assert!(!world.issue_move_order(1, squad_id, Vec3::Z));

        world.update_entities(1.0);
        assert!((world.get_unit(leader_id).unwrap().base.position.y - 50.0).abs() <= f32::EPSILON);
        world.update_entities(1.0);
        assert!(
            world
                .get_squad(squad_id)
                .unwrap()
                .trained_air_birth()
                .is_none()
        );
        assert_eq!(
            world.get_squad(squad_id).unwrap().move_target,
            Some(Vec3::X * 10.0)
        );
    }
}
