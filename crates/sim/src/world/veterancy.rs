//! Retail squad experience banking, level upgrades, and Board propagation.

use super::World;
use crate::entities::{Squad, SquadBoardState};
use crate::entity::Entity;
use crate::entity_id::EntityId;
use crate::gameplay::GameplayCatalog;

impl World {
    /// Add and immediately apply authored veterancy XP to a live squad.
    pub fn add_squad_experience(
        &mut self,
        squad_id: EntityId,
        experience: f32,
        gameplay: &GameplayCatalog,
    ) -> bool {
        if !self.bank_squad_experience(squad_id, experience, gameplay) {
            return false;
        }
        self.apply_squad_experience_bank(squad_id, gameplay);
        true
    }

    /// Return earned squad veterancy plus a contained Board source's override.
    #[must_use]
    pub fn effective_squad_veterancy_level(&self, squad_id: EntityId) -> Option<i32> {
        let squad = self.squads.get(squad_id)?;
        if !self.veterancy_enabled() {
            return Some(0);
        }
        let board_bonus = self
            .boarded_veterancy_source(squad_id)
            .and_then(|source_id| self.squads.get(source_id))
            .and_then(Squad::board_state)
            .map_or(0, SquadBoardState::effective_veterancy_bonus);
        Some(squad.veterancy_level().saturating_add(board_bonus))
    }

    pub(in crate::world) fn bank_combat_experience(
        &mut self,
        attacker_unit_id: EntityId,
        target_proto_object: &str,
        hitpoint_damage: f32,
        gameplay: &GameplayCatalog,
    ) {
        let Some(squad_id) = self
            .units
            .get(attacker_unit_id)
            .and_then(|unit| unit.squad_id)
        else {
            return;
        };
        let experience = gameplay.bounty_experience(target_proto_object, hitpoint_damage);
        if experience > 0.0 {
            self.bank_squad_experience(squad_id, experience, gameplay);
        }
    }

    pub(in crate::world) fn bank_squad_experience(
        &mut self,
        squad_id: EntityId,
        experience: f32,
        gameplay: &GameplayCatalog,
    ) -> bool {
        if !self.veterancy_enabled() || !experience.is_finite() || !self.squads.contains(squad_id) {
            return false;
        }
        let split = self.boarded_experience_split(squad_id, experience, gameplay);
        if let Some((source_id, source_experience, target_experience)) = split {
            let source_banked = self
                .squads
                .get_mut(source_id)
                .is_some_and(|source| source.bank_experience(source_experience));
            let target_banked = self
                .squads
                .get_mut(squad_id)
                .is_some_and(|target| target.bank_experience(target_experience));
            return source_banked || target_banked;
        }
        self.squads
            .get_mut(squad_id)
            .is_some_and(|squad| squad.bank_experience(experience))
    }

    pub(in crate::world) fn apply_squad_experience_bank(
        &mut self,
        squad_id: EntityId,
        gameplay: &GameplayCatalog,
    ) {
        if !self.veterancy_enabled() {
            return;
        }
        self.apply_one_squad_experience_bank(squad_id, gameplay);
        if let Some(source_id) = self.boarded_veterancy_source(squad_id) {
            self.apply_one_squad_experience_bank(source_id, gameplay);
        }
    }

    fn apply_one_squad_experience_bank(&mut self, squad_id: EntityId, gameplay: &GameplayCatalog) {
        let Some(proto_squad) = self.effective_proto_squad_name(squad_id) else {
            return;
        };
        let Some((current_level, experience)) = self
            .squads
            .get_mut(squad_id)
            .map(|squad| (squad.veterancy_level(), squad.apply_experience_bank()))
        else {
            return;
        };
        let target_level =
            gameplay.squad_veterancy_level_for_experience(&proto_squad, current_level, experience);
        for level in current_level.saturating_add(1)..=target_level {
            self.apply_squad_veterancy_level(squad_id, level, gameplay);
        }
    }

    fn apply_squad_veterancy_level(
        &mut self,
        squad_id: EntityId,
        level: i32,
        gameplay: &GameplayCatalog,
    ) {
        if !self.veterancy_enabled() {
            return;
        }
        let Some((previous_level, unit_ids)) = self
            .squads
            .get(squad_id)
            .map(|squad| (squad.veterancy_level(), squad.unit_ids.clone()))
        else {
            return;
        };
        if level != previous_level.saturating_add(1) {
            return;
        }
        if let Some(squad) = self.squads.get_mut(squad_id) {
            squad.set_veterancy_level(level);
        }
        for unit_id in unit_ids {
            let Some(proto_object) = self
                .units
                .get(unit_id)
                .filter(|unit| unit.is_alive())
                .map(|unit| unit.proto_object_name.clone())
            else {
                continue;
            };
            gameplay
                .object_veterancy_modifiers(&proto_object, previous_level, level)
                .apply(self.units.get_mut(unit_id).unwrap());
        }
        self.propagate_boarded_source_level(squad_id, level, gameplay);
    }

    fn propagate_boarded_source_level(
        &mut self,
        source_squad_id: EntityId,
        level: i32,
        gameplay: &GameplayCatalog,
    ) {
        let Some((target_unit_id, previous_level, proto_object)) =
            self.squads.get(source_squad_id).and_then(|source| {
                let board = source
                    .board_state()
                    .filter(|board| board.is_complete() && board.veterancy_override())?;
                let proto_object = source
                    .unit_ids
                    .first()
                    .and_then(|unit_id| self.units.get(*unit_id))?
                    .proto_object_name
                    .clone();
                Some((
                    board.target_unit_id(),
                    board.source_veterancy_level(),
                    proto_object,
                ))
            })
        else {
            return;
        };
        let modifiers = gameplay.object_veterancy_modifiers(&proto_object, previous_level, level);
        let Some(target) = self
            .units
            .get_mut(target_unit_id)
            .filter(|target| target.is_alive())
        else {
            return;
        };
        modifiers.apply(target);
        if let Some(board) = self
            .squads
            .get_mut(source_squad_id)
            .and_then(Squad::board_state_mut)
        {
            board.advance_source_veterancy(level, modifiers);
        }
    }

    fn boarded_experience_split(
        &self,
        target_squad_id: EntityId,
        experience: f32,
        gameplay: &GameplayCatalog,
    ) -> Option<(EntityId, f32, f32)> {
        let source_squad_id = self.boarded_veterancy_source(target_squad_id)?;
        let target_proto = self.effective_proto_squad_name(target_squad_id)?;
        let source_proto = self.effective_proto_squad_name(source_squad_id)?;
        let target_value = gameplay.squad_combat_value(&target_proto).max(0.0);
        let source_value = gameplay.squad_combat_value(&source_proto).max(0.0);
        let total_value = target_value + source_value;
        if !total_value.is_finite() || total_value <= 0.0 || source_value <= 0.0 {
            return None;
        }
        let source_experience = experience * (source_value / total_value);
        Some((
            source_squad_id,
            source_experience,
            experience - source_experience,
        ))
    }

    fn boarded_veterancy_source(&self, target_squad_id: EntityId) -> Option<EntityId> {
        let target = self.squads.get(target_squad_id)?;
        target
            .garrison
            .contained_squad_ids()
            .iter()
            .copied()
            .find(|source_id| {
                self.squads
                    .get(*source_id)
                    .and_then(Squad::board_state)
                    .is_some_and(|board| {
                        board.is_complete()
                            && board.veterancy_override()
                            && target.contains_unit(board.target_unit_id())
                    })
            })
    }

    fn effective_proto_squad_name(&self, squad_id: EntityId) -> Option<String> {
        let squad = self.squads.get(squad_id)?;
        Some(self.get_player(squad.base.player_id).map_or_else(
            || squad.proto_squad_name.clone(),
            |player| {
                player
                    .technologies
                    .resolved_squad_prototype(&squad.proto_squad_name)
                    .to_owned()
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::UnitDataScalar;
    use crate::scenario::create_squad_from_prototype;
    use glam::Vec3;
    use pipeline::database::hw1::objects::VeterancyLevel;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
    use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};

    #[test]
    fn strict_thresholds_upgrade_every_live_member_one_level_at_a_time() {
        let database = veterancy_database();
        let gameplay = GameplayCatalog::from_tactics(&database, []);
        let mut world = World::new();
        world.init_players(1);
        let squad_id = create_squad_from_prototype(
            &mut world,
            1,
            Vec3::ZERO,
            Vec3::Z,
            "marine_squad",
            &database,
        );

        assert_eq!(
            gameplay.squad_veterancy_thresholds("marine_squad"),
            Some(&[20.0, 40.0][..])
        );
        assert!(world.add_squad_experience(squad_id, 20.0, &gameplay));
        assert_eq!(world.get_squad(squad_id).unwrap().veterancy_level(), 0);

        assert!(world.add_squad_experience(squad_id, 20.01, &gameplay));
        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.veterancy_level(), 2);
        assert!(nearly_equal(squad.experience(), 40.01));
        assert!(nearly_equal(squad.banked_experience(), 0.0));
        for unit_id in &squad.unit_ids {
            let unit = world.get_unit(*unit_id).unwrap();
            assert!(nearly_equal(unit.data_scalar(UnitDataScalar::Damage), 1.5));
        }
    }

    #[test]
    fn disabled_gate_blocks_initial_levels_xp_scalars_and_changes_checksum() {
        let mut database = veterancy_database();
        database.squads[0].level = Some(1);
        let gameplay = GameplayCatalog::from_tactics(&database, []);
        let mut world = World::new();
        world.set_veterancy_enabled(false);
        world.init_players(1);
        let squad_id = create_squad_from_prototype(
            &mut world,
            1,
            Vec3::ZERO,
            Vec3::Z,
            "marine_squad",
            &database,
        );

        let squad = world.get_squad(squad_id).unwrap();
        assert_eq!(squad.veterancy_level(), 0);
        assert_eq!(world.effective_squad_veterancy_level(squad_id), Some(0));
        for unit_id in &squad.unit_ids {
            let unit = world.get_unit(*unit_id).unwrap();
            assert!(nearly_equal(unit.data_scalar(UnitDataScalar::Damage), 1.0));
        }
        assert!(!world.add_squad_experience(squad_id, 100.0, &gameplay));
        assert!(nearly_equal(
            world.get_squad(squad_id).unwrap().experience(),
            0.0
        ));

        let disabled_checksum = world.checksum();
        world.set_veterancy_enabled(true);
        assert_ne!(world.checksum(), disabled_checksum);
        assert!(world.add_squad_experience(squad_id, 20.01, &gameplay));
        assert_eq!(world.get_squad(squad_id).unwrap().veterancy_level(), 1);
    }

    fn veterancy_database() -> Database {
        Database {
            objects: vec![ProtoObject {
                name: "marine".to_owned(),
                combat_value: Some(10.0),
                veterancy: vec![
                    VeterancyLevel {
                        level: 1,
                        xp: Some(10.0),
                        damage: Some(1.2),
                        ..VeterancyLevel::default()
                    },
                    VeterancyLevel {
                        level: 2,
                        xp: Some(20.0),
                        damage: Some(1.25),
                        ..VeterancyLevel::default()
                    },
                ],
                ..ProtoObject::default()
            }],
            squads: vec![ProtoSquad {
                name: "marine_squad".to_owned(),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "marine".to_owned(),
                        count: 2,
                        ..UnitEntry::default()
                    }],
                }),
                ..ProtoSquad::default()
            }],
            ..Database::default()
        }
    }

    fn nearly_equal(left: f32, right: f32) -> bool {
        (left - right).abs() <= f32::EPSILON * left.abs().max(right.abs()).max(1.0)
    }
}
