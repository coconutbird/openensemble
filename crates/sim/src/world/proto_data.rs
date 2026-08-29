//! Live reconciliation for player-owned trigger prototype changes.

use super::World;
use crate::entity_id::EntityId;
use crate::player::{Player, PlayerId, ProtoDataModification, ProtoDataType};
use crate::scenario::refresh_squad_member_settings;
use pipeline::database::hw1::ProtoObject;
use std::collections::BTreeSet;

impl World {
    pub(crate) fn modify_player_proto_data(
        &mut self,
        player_id: PlayerId,
        prototype: &ProtoObject,
        modification: &ProtoDataModification,
        database: &pipeline::database::hw1::Database,
    ) -> bool {
        let data_type = modification.data_type;
        let amount = modification.amount;
        let previous = self
            .get_player(player_id)
            .and_then(|player| live_scalar(player, prototype, data_type));
        let Some(player) = self.get_player_mut(player_id) else {
            return false;
        };
        player
            .technologies
            .modify_proto_data(&prototype.name, modification);
        let next = self
            .get_player(player_id)
            .and_then(|player| live_scalar(player, prototype, data_type));

        match data_type {
            ProtoDataType::Hitpoints => {
                if let (Some(previous), Some(next)) = (previous, next) {
                    self.reconcile_hitpoints(player_id, &prototype.name, previous, next);
                }
            }
            ProtoDataType::Shieldpoints => {
                if let (Some(previous), Some(next)) = (previous, next) {
                    self.reconcile_shieldpoints(player_id, &prototype.name, previous, next);
                }
            }
            ProtoDataType::MaximumVelocity => {
                if let Some(next) = next {
                    self.reconcile_velocity(player_id, &prototype.name, next);
                }
            }
            ProtoDataType::ShieldRegenDelay => {
                self.assign_unit_shield_delay(player_id, &prototype.name, amount);
            }
            ProtoDataType::AmmoMax | ProtoDataType::AmmoRegenRate => {
                self.reconcile_ammunition_profile(player_id, prototype);
                self.refresh_player_squad_ammunition(player_id, database);
            }
            _ => {}
        }
        true
    }

    fn reconcile_hitpoints(
        &mut self,
        player_id: PlayerId,
        proto_object: &str,
        previous: f32,
        next: f32,
    ) {
        if floats_equal(previous, next)
            || previous.classify() == std::num::FpCategory::Zero
            || !next.is_finite()
        {
            return;
        }
        let ratio = next / previous;
        for (_, unit) in self.units.iter_mut().filter(|(_, unit)| {
            unit.base.player_id == player_id
                && unit.proto_object_name.eq_ignore_ascii_case(proto_object)
        }) {
            unit.hitpoints = (unit.hitpoints * ratio).clamp(0.0, next.max(0.0));
            unit.max_hitpoints = next.max(0.0);
        }
    }

    fn reconcile_shieldpoints(
        &mut self,
        player_id: PlayerId,
        proto_object: &str,
        previous: f32,
        next: f32,
    ) {
        if floats_equal(previous, next) || !next.is_finite() {
            return;
        }
        let unit_ids = matching_unit_ids(self, player_id, proto_object);
        let mut recharge = Vec::new();
        for unit_id in unit_ids {
            if self
                .units
                .get_mut(unit_id)
                .is_some_and(|unit| unit.shields.set_maximum(next))
            {
                recharge.push(unit_id);
            }
        }
        for unit_id in recharge {
            self.request_unit_shield_recharge(unit_id);
        }
    }

    fn reconcile_velocity(&mut self, player_id: PlayerId, proto_object: &str, next: f32) {
        let unit_ids = matching_unit_ids(self, player_id, proto_object);
        let mut squads = BTreeSet::new();
        for unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.set_prototype_movement_speed(next);
                squads.extend(unit.squad_id);
            }
        }
        for squad_id in squads {
            refresh_squad_member_settings(self, squad_id);
        }
    }

    fn assign_unit_shield_delay(&mut self, player_id: PlayerId, proto_object: &str, amount: f32) {
        for unit_id in matching_unit_ids(self, player_id, proto_object) {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.shields.set_regen_delay_scalar(amount);
            }
        }
    }

    fn reconcile_ammunition_profile(&mut self, player_id: PlayerId, prototype: &ProtoObject) {
        let base_maximum = finite_or_zero(prototype.ammo_max);
        let base_rate = finite_or_zero(prototype.ammo_regen_rate);
        let Some(player) = self.get_player(player_id) else {
            return;
        };
        let maximum = player
            .technologies
            .ammunition_maximum(&prototype.name, base_maximum);
        let rate = player
            .technologies
            .ammunition_regeneration_rate(&prototype.name, base_rate);
        for unit_id in matching_unit_ids(self, player_id, &prototype.name) {
            if let Some(unit) = self.units.get_mut(unit_id) {
                unit.ammunition.reconcile_profile(maximum, rate);
            }
        }
    }
}

fn matching_unit_ids(world: &World, player_id: PlayerId, proto_object: &str) -> Vec<EntityId> {
    world
        .units
        .iter()
        .filter_map(|(unit_id, unit)| {
            (unit.base.player_id == player_id
                && unit.proto_object_name.eq_ignore_ascii_case(proto_object))
            .then_some(unit_id)
        })
        .collect()
}

fn floats_equal(left: f32, right: f32) -> bool {
    left.partial_cmp(&right) == Some(std::cmp::Ordering::Equal)
}

fn live_scalar(player: &Player, prototype: &ProtoObject, data_type: ProtoDataType) -> Option<f32> {
    let technologies = &player.technologies;
    match data_type {
        ProtoDataType::Hitpoints => prototype
            .hitpoints
            .map(|base| technologies.hitpoints(&prototype.name, base)),
        ProtoDataType::Shieldpoints => Some(
            technologies.shieldpoints(&prototype.name, prototype.shieldpoints.unwrap_or_default()),
        ),
        ProtoDataType::MaximumVelocity => Some(
            technologies.maximum_velocity(
                &prototype.name,
                prototype
                    .max_velocity
                    .or(prototype.velocity)
                    .unwrap_or_default(),
            ),
        ),
        ProtoDataType::AmmoMax => Some(
            technologies.ammunition_maximum(&prototype.name, finite_or_zero(prototype.ammo_max)),
        ),
        ProtoDataType::AmmoRegenRate => Some(technologies.ammunition_regeneration_rate(
            &prototype.name,
            finite_or_zero(prototype.ammo_regen_rate),
        )),
        _ => None,
    }
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::ShieldCoverage;
    use crate::player::ProtoDataRelativity;
    use crate::scenario::create_squad_from_prototype;
    use glam::Vec3;
    use pipeline::database::hw1::Squad as ProtoSquad;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    fn modification(data_type: ProtoDataType, amount: f32) -> ProtoDataModification {
        ProtoDataModification {
            data_type,
            amount,
            relativity: ProtoDataRelativity::Percent,
            all_actions: true,
            name: None,
            invert: false,
            command_type: None,
            command_data: None,
        }
    }

    #[test]
    fn hitpoints_shields_and_velocity_reconcile_existing_units() {
        let mut world = World::new();
        world.init_players(1);
        let unit_id = world.create_unit(1);
        let unit = world.get_unit_mut(unit_id).unwrap();
        unit.proto_object_name = "test".to_owned();
        unit.hitpoints = 25.0;
        unit.max_hitpoints = 100.0;
        unit.speed = 10.0;
        unit.shields.configure(ShieldCoverage::Full, 20.0);
        let prototype = ProtoObject {
            name: "test".to_owned(),
            hitpoints: Some(100.0),
            shieldpoints: Some(20.0),
            max_velocity: Some(10.0),
            ..ProtoObject::default()
        };
        let mut database = pipeline::database::hw1::Database::new();
        database.objects.push(prototype.clone());

        assert!(world.modify_player_proto_data(
            1,
            &prototype,
            &modification(ProtoDataType::Hitpoints, 2.0),
            &database,
        ));
        assert!(world.modify_player_proto_data(
            1,
            &prototype,
            &modification(ProtoDataType::Shieldpoints, 2.0),
            &database,
        ));
        assert!(world.modify_player_proto_data(
            1,
            &prototype,
            &modification(ProtoDataType::MaximumVelocity, 0.5),
            &database,
        ));

        let unit = world.get_unit(unit_id).unwrap();
        assert_close(unit.hitpoints, 50.0);
        assert_close(unit.max_hitpoints, 200.0);
        assert_close(unit.shields.maximum, 40.0);
        assert_close(unit.speed, 5.0);
    }

    #[test]
    fn ammunition_proto_changes_scale_units_and_refresh_squad_maximums() {
        let prototype = ProtoObject {
            name: "ammo_unit".to_owned(),
            object_class: Some("Unit".to_owned()),
            ammo_max: Some(100.0),
            ammo_regen_rate: Some(2.0),
            ..ProtoObject::default()
        };
        let mut database = pipeline::database::hw1::Database::new();
        database.objects.push(prototype.clone());
        database.squads.push(ProtoSquad {
            name: "ammo_squad".to_owned(),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: prototype.name.clone(),
                    count: 1,
                    ..UnitEntry::default()
                }],
            }),
            ..ProtoSquad::default()
        });
        let mut world = World::new();
        world.init_players(1);
        let squad_id = create_squad_from_prototype(
            &mut world,
            1,
            Vec3::ZERO,
            Vec3::Z,
            "ammo_squad",
            &database,
        );
        let unit_id = world.get_squad(squad_id).unwrap().unit_ids[0];
        world
            .get_unit_mut(unit_id)
            .unwrap()
            .ammunition
            .set_current(50.0);

        assert!(world.modify_player_proto_data(
            1,
            &prototype,
            &modification(ProtoDataType::AmmoMax, 2.0),
            &database,
        ));
        let ammunition = world.unit_ammunition(unit_id).unwrap();
        assert_close(ammunition.current(), 100.0);
        assert_close(ammunition.maximum(), 200.0);
        assert_eq!(world.squad_ammunition(squad_id), Some((100.0, 200.0)));

        let mut rate = modification(ProtoDataType::AmmoRegenRate, 5.0);
        rate.relativity = ProtoDataRelativity::Assign;
        assert!(world.modify_player_proto_data(1, &prototype, &rate, &database));
        assert_close(
            world.unit_ammunition(unit_id).unwrap().regeneration_rate(),
            5.0,
        );
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= f32::EPSILON * expected.abs().max(1.0));
    }
}
