//! Retail shield recharge action scheduling.

use super::World;
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;
use crate::player::PlayerId;
use glam::Vec3;

impl World {
    /// Apply already-modified damage and emit the unit/squad damage event.
    ///
    /// Weapon, armor, veterancy, and height modifiers belong at the caller;
    /// this method owns shield overflow, hit-point mutation, and recharge delay.
    pub fn damage_unit(&mut self, unit_id: EntityId, damage: f32) -> bool {
        self.damage_unit_oriented(unit_id, damage, None)
    }

    pub(super) fn damage_unit_directional(
        &mut self,
        unit_id: EntityId,
        damage: f32,
        direction: Vec3,
    ) -> bool {
        self.damage_unit_oriented(unit_id, damage, Some(direction))
    }

    fn damage_unit_oriented(
        &mut self,
        unit_id: EntityId,
        damage: f32,
        direction: Option<Vec3>,
    ) -> bool {
        let unit_id = self.resolve_damage_target(unit_id);
        self.damage_resolved_unit_oriented(unit_id, damage, direction)
    }

    fn damage_resolved_unit_oriented(
        &mut self,
        unit_id: EntityId,
        damage: f32,
        direction: Option<Vec3>,
    ) -> bool {
        let damaged = self
            .units
            .get_mut(unit_id)
            .is_some_and(|unit| match direction {
                Some(direction) => unit.damage_directional(damage, direction),
                None => unit.damage(damage),
            });
        if damaged {
            self.notify_unit_damaged(unit_id);
            if self
                .units
                .get(unit_id)
                .is_some_and(crate::entities::Unit::is_incapacitated)
            {
                self.cancel_incapacitated_squad_orders(unit_id);
            }
        }
        damaged
    }

    pub(super) fn damage_unit_directional_with_gameplay(
        &mut self,
        unit_id: EntityId,
        damage: f32,
        direction: Vec3,
        gameplay: &GameplayCatalog,
    ) -> bool {
        let unit_id = self.resolve_damage_target(unit_id);
        let _configured = self.configure_unit_revival(unit_id, gameplay);
        self.damage_resolved_unit_oriented(unit_id, damage, Some(direction))
    }

    /// Configure scenario-layered revival behavior before applying combat damage.
    pub fn damage_unit_with_gameplay(
        &mut self,
        unit_id: EntityId,
        damage: f32,
        gameplay: &GameplayCatalog,
    ) -> bool {
        self.damage_unit_with_gameplay_override(unit_id, damage, gameplay, false)
    }

    /// Apply combat damage with retail's optional revive-action override.
    pub fn damage_unit_with_override(
        &mut self,
        unit_id: EntityId,
        damage: f32,
        override_revive: bool,
    ) -> bool {
        let unit_id = self.resolve_damage_target(unit_id);
        if override_revive
            && self
                .units
                .get_mut(unit_id)
                .is_some_and(crate::entities::Unit::override_revival_at_zero)
        {
            return true;
        }
        self.damage_resolved_unit_oriented(unit_id, damage, None)
    }

    /// Configure layered revival behavior, then apply optional override damage.
    pub fn damage_unit_with_gameplay_override(
        &mut self,
        unit_id: EntityId,
        damage: f32,
        gameplay: &GameplayCatalog,
        override_revive: bool,
    ) -> bool {
        let unit_id = self.resolve_damage_target(unit_id);
        let _configured = self.configure_unit_revival(unit_id, gameplay);
        if override_revive
            && self
                .units
                .get_mut(unit_id)
                .is_some_and(crate::entities::Unit::override_revival_at_zero)
        {
            return true;
        }
        self.damage_resolved_unit_oriented(unit_id, damage, None)
    }

    pub(super) fn update_shields(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        self.start_squad_recharges(gameplay);
        self.start_standalone_recharges(gameplay);
        self.advance_shield_recharges(dt, gameplay);
        self.advance_shield_damage_clocks(dt);
    }

    pub(super) fn notify_unit_damaged(&mut self, unit_id: EntityId) {
        let squad_id = self.units.get(unit_id).and_then(|unit| unit.squad_id);
        if let Some(squad_id) = squad_id {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.shields.notify_damaged();
                squad.last_damaged_time = self.game_time_ms;
            }
        } else if let Some(unit) = self.units.get_mut(unit_id) {
            unit.shields.notify_damaged();
        }
    }

    pub(super) fn request_unit_shield_recharge(&mut self, unit_id: EntityId) {
        let squad_id = self.units.get(unit_id).and_then(|unit| unit.squad_id);
        if let Some(squad_id) = squad_id {
            if let Some(squad) = self.squads.get_mut(squad_id) {
                squad.shields.request_recharge();
            }
        } else if let Some(unit) = self.units.get_mut(unit_id) {
            unit.shields.request_recharge();
        }
    }

    fn start_squad_recharges(&mut self, gameplay: &GameplayCatalog) {
        let squad_ids = self.squads.ids().collect::<Vec<_>>();
        for squad_id in squad_ids {
            let Some((player_id, leader_id, unit_ids)) = self.squads.get(squad_id).map(|squad| {
                (
                    squad.base.player_id,
                    squad.unit_ids.first().copied(),
                    squad.unit_ids.clone(),
                )
            }) else {
                continue;
            };
            let leader_delay_scalar = leader_id
                .and_then(|unit_id| self.units.get(unit_id))
                .map_or(1.0, |unit| unit.shields.regen_delay_scalar());
            let delay = self.player_shield_delay(player_id, gameplay) * leader_delay_scalar;
            let ready = self
                .squads
                .get_mut(squad_id)
                .is_some_and(|squad| squad.shields.take_recharge_request(delay));
            if ready {
                self.start_unit_recharges(&unit_ids, gameplay.shield_regen_time());
            }
        }
    }

    fn start_standalone_recharges(&mut self, gameplay: &GameplayCatalog) {
        let unit_ids = self
            .units
            .iter()
            .filter_map(|(unit_id, unit)| unit.squad_id.is_none().then_some(unit_id))
            .collect::<Vec<_>>();
        for unit_id in unit_ids {
            let Some((player_id, delay_scalar)) = self
                .units
                .get(unit_id)
                .map(|unit| (unit.base.player_id, unit.shields.regen_delay_scalar()))
            else {
                continue;
            };
            let delay = self.player_shield_delay(player_id, gameplay) * delay_scalar;
            let ready = self
                .units
                .get_mut(unit_id)
                .is_some_and(|unit| unit.shields.take_recharge_request(delay));
            if ready {
                self.start_unit_recharges(&[unit_id], gameplay.shield_regen_time());
            }
        }
    }

    fn start_unit_recharges(&mut self, unit_ids: &[EntityId], duration: f32) {
        for &unit_id in unit_ids {
            if let Some(unit) = self.units.get_mut(unit_id)
                && unit.is_alive()
                && !unit.is_down()
                && unit.shields.is_enabled()
            {
                unit.shields.start_recharge(duration);
            }
        }
    }

    fn advance_shield_recharges(&mut self, dt: f32, gameplay: &GameplayCatalog) {
        let unit_ids = self.units.ids().collect::<Vec<_>>();
        for unit_id in unit_ids {
            let Some(player_id) = self.units.get(unit_id).map(|unit| unit.base.player_id) else {
                continue;
            };
            let player_rate =
                self.get_player(player_id)
                    .map_or(gameplay.shield_regen_rate(), |player| {
                        player
                            .technologies
                            .shield_regen_rate(gameplay.shield_regen_rate())
                    });
            if let Some(unit) = self.units.get_mut(unit_id).filter(|unit| !unit.is_down()) {
                unit.shields.advance_recharge(dt, player_rate);
            }
        }
    }

    fn advance_shield_damage_clocks(&mut self, dt: f32) {
        for (_, squad) in self.squads.iter_mut() {
            squad.shields.advance_damage_clock(dt);
        }
        for (_, unit) in self.units.iter_mut() {
            unit.shields.advance_damage_clock(dt);
        }
    }

    fn player_shield_delay(&self, player_id: PlayerId, gameplay: &GameplayCatalog) -> f32 {
        self.get_player(player_id)
            .map_or(gameplay.shield_regen_delay(), |player| {
                player
                    .technologies
                    .shield_regen_delay(gameplay.shield_regen_delay())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::ShieldCoverage;
    use pipeline::database::hw1::weapontypes::DamageModifier;
    use pipeline::database::hw1::{Database, GameData, ProtoObject, WeaponType};

    #[test]
    fn shielded_marker_is_not_used_as_an_armor_modifier() {
        let (gameplay, mut world, unit_id) = shield_world(2.0, 1.0, false);
        {
            let unit = world.get_unit_mut(unit_id).expect("shielded unit");
            unit.shields.set_current(10.0);
            unit.damage_taken_multiplier = 0.5;
        }

        world.apply_weapon_damage(1, unit_id, 30.0, Some("Plasma"), Some(&gameplay));

        let unit = world.get_unit(unit_id).expect("damaged unit");
        assert!(nearly_equal(unit.shields.current, 0.0));
        assert!(nearly_equal(unit.hitpoints, 95.0));
    }

    #[test]
    fn squad_recharge_is_immediate_at_birth_and_strictly_delayed_after_damage() {
        let (gameplay, mut world, unit_id) = shield_world(2.0, 1.0, true);

        world.update_entities_with_gameplay(1.0, &gameplay);
        assert!(nearly_equal(
            world.get_unit(unit_id).unwrap().shields.current,
            10.0
        ));

        assert!(world.damage_unit(unit_id, 4.0));
        world.update_entities_with_gameplay(2.0, &gameplay);
        assert!(nearly_equal(
            world.get_unit(unit_id).unwrap().shields.current,
            6.0
        ));

        world.update_entities_with_gameplay(0.1, &gameplay);
        assert!(world.get_unit(unit_id).unwrap().shields.current > 6.0);
    }

    fn shield_world(
        delay: f32,
        time: f32,
        attach_to_squad: bool,
    ) -> (GameplayCatalog, World, EntityId) {
        let mut database = Database::new();
        database.game_data = Some(GameData {
            shield_regen_delay: Some(delay),
            shield_regen_time: Some(time),
            ..GameData::default()
        });
        database.objects.push(ProtoObject {
            name: "shielded_target".to_owned(),
            damage_type: Some("Shielded".to_owned()),
            shieldpoints: Some(10.0),
            ..ProtoObject::default()
        });
        database.weapon_types.push(WeaponType {
            name: "Plasma".to_owned(),
            damage_modifiers: vec![DamageModifier {
                damage_type: "Shielded".to_owned(),
                modifier: 4.0,
                ..DamageModifier::default()
            }],
            ..WeaponType::default()
        });
        let gameplay = GameplayCatalog::from_tactics(&database, std::iter::empty());
        let mut world = World::new();
        world.init_players(1);
        let unit_id = world.create_unit(1);
        {
            let unit = world.get_unit_mut(unit_id).unwrap();
            unit.proto_object_name = "shielded_target".to_owned();
            unit.shields.configure(ShieldCoverage::Full, 10.0);
        }
        if attach_to_squad {
            let squad_id = world.create_squad(1);
            assert!(world.attach_unit_to_squad(unit_id, squad_id));
        }
        (gameplay, world, unit_id)
    }

    fn nearly_equal(left: f32, right: f32) -> bool {
        (left - right).abs() <= f32::EPSILON * left.abs().max(right.abs()).max(1.0) * 8.0
    }
}
