//! Source-backed economy state activated by `BUnit::onBuilt`.

use crate::entities::units::BuiltEconomyState;
use crate::entity_id::EntityId;
use crate::player::{MAX_RESOURCES, PlayerId};
use crate::world::{TriggerTrainingRequest, World, squad_runtime_id};
use pipeline::database::hw1::{Database, ProtoObject};

impl World {
    pub(crate) fn activate_unit_on_built(
        &mut self,
        unit_id: EntityId,
        database: &Database,
        prototype: &ProtoObject,
    ) {
        if !self.get_unit(unit_id).is_some_and(|unit| unit.built) {
            return;
        }
        let _economy_activated = self.activate_unit_built_economy(unit_id, database, prototype);
        self.recompute_unit_base_child_damage(unit_id);
        self.auto_train_unit_on_built(unit_id, database, prototype);
        crate::scenario::child_objects::materialize_authored_child_objects(
            self, unit_id, prototype, database,
        );
    }

    fn auto_train_unit_on_built(
        &mut self,
        unit_id: EntityId,
        database: &Database,
        prototype: &ProtoObject,
    ) {
        let Some(prototype_id) = prototype
            .auto_train_on_built
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .and_then(|name| squad_runtime_id(database, name))
        else {
            return;
        };
        let Some(player_id) = self.get_unit(unit_id).map(|unit| unit.base.player_id) else {
            return;
        };
        let _queued = self.queue_trigger_training(TriggerTrainingRequest {
            player_id,
            building_id: unit_id,
            database,
            prototype_id,
            count: 1,
            no_cost: true,
            trigger_state: None,
        });
    }

    pub(crate) fn activate_unit_built_economy(
        &mut self,
        unit_id: EntityId,
        database: &Database,
        prototype: &ProtoObject,
    ) -> bool {
        let Some((player_id, built, current)) = self
            .get_unit(unit_id)
            .map(|unit| (unit.base.player_id, unit.built, unit.built_economy))
        else {
            return false;
        };
        if !built || !current.is_empty() {
            return false;
        }
        let state = built_economy_state(self, player_id, database, prototype);
        if state.is_empty() {
            return false;
        }
        self.adjust_player_built_economy(player_id, state, 1.0);
        if let Some(unit) = self.get_unit_mut(unit_id) {
            unit.built_economy = state;
            return true;
        }
        self.adjust_player_built_economy(player_id, state, -1.0);
        false
    }

    pub(in crate::world) fn deactivate_unit_built_economy(&mut self, unit_id: EntityId) -> bool {
        let Some((player_id, state)) = self
            .get_unit_mut(unit_id)
            .map(|unit| (unit.base.player_id, unit.take_built_economy_state()))
        else {
            return false;
        };
        if state.is_empty() {
            return false;
        }
        self.adjust_player_built_economy(player_id, state, -1.0);
        true
    }

    pub(in crate::world) fn transfer_unit_built_economy(
        &mut self,
        old_owner: PlayerId,
        new_owner: PlayerId,
        state: BuiltEconomyState,
    ) {
        if old_owner == new_owner || state.is_empty() {
            return;
        }
        self.adjust_player_built_economy(old_owner, state, -1.0);
        self.adjust_player_built_economy(new_owner, state, 1.0);
    }

    fn adjust_player_built_economy(
        &mut self,
        player_id: PlayerId,
        state: BuiltEconomyState,
        scalar: f32,
    ) {
        let Some(player) = self.get_player_mut(player_id) else {
            return;
        };
        if let Some((resource_id, amount)) = state.resource() {
            player.add_resource(resource_id, amount * scalar);
        }
        if let Some((rate_id, amount)) = state.rate() {
            let _adjusted = player.add_rate_amount(rate_id, amount * scalar);
        }
    }
}

fn built_economy_state(
    world: &World,
    player_id: PlayerId,
    database: &Database,
    prototype: &ProtoObject,
) -> BuiltEconomyState {
    let Some(player) = world.get_player(player_id) else {
        return BuiltEconomyState::default();
    };
    let resource = prototype.add_resource.as_ref().and_then(|resource| {
        database
            .game_data
            .as_ref()?
            .resources
            .as_ref()?
            .entries
            .iter()
            .position(|entry| {
                entry
                    .name
                    .eq_ignore_ascii_case(resource.resource_type.trim())
            })
            .filter(|resource_id| *resource_id < MAX_RESOURCES)
            .map(|resource_id| (resource_id, resource.amount.unwrap_or_default()))
    });
    let rate = prototype.rate.as_ref().and_then(|rate| {
        let rate_name = rate.rate_type.as_deref()?.trim();
        database
            .game_data
            .as_ref()?
            .rates
            .as_ref()?
            .entries
            .iter()
            .position(|entry| entry.eq_ignore_ascii_case(rate_name))
            .filter(|rate_id| *rate_id < player.rate_slot_count())
            .map(|rate_id| (rate_id, rate.value))
    });
    BuiltEconomyState::new(resource, rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{configure_unit_from_proto, population};
    use pipeline::database::hw1::Squad as ProtoSquad;
    use pipeline::database::hw1::objects::ObjectCommand;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    #[test]
    fn auto_train_on_built_completes_one_squad_without_cost_or_population() {
        let database = auto_train_database();
        let mut world = World::new();
        world.init_players(1);
        let building_id = world.create_building(1);
        configure_unit_from_proto(&mut world, building_id, "hive", 0, &database.objects[0]);
        let resources_before = world.get_player(1).unwrap().resources;

        population::apply_object_population(
            &mut world,
            building_id,
            &database,
            &database.objects[0],
        );

        let squad_id = world
            .squads
            .iter()
            .find_map(|(id, squad)| (squad.proto_squad_name == "drone_squad").then_some(id))
            .expect("onBuilt should train the authored squad immediately");
        assert_eq!(
            world.get_squad(squad_id).unwrap().trained_by,
            Some(building_id)
        );
        assert_eq!(world.get_player(1).unwrap().resources, resources_before);
        assert!(
            world
                .get_player(1)
                .unwrap()
                .population
                .iter()
                .all(|population| population.count == 0.0)
        );
    }

    fn auto_train_database() -> Database {
        Database {
            objects: vec![
                ProtoObject {
                    name: "hive".to_owned(),
                    object_class: Some("Building".to_owned()),
                    auto_train_on_built: Some("drone_squad".to_owned()),
                    commands: vec![ObjectCommand {
                        target: "drone_squad".to_owned(),
                        command_type: Some("TrainSquad".to_owned()),
                        ..ObjectCommand::default()
                    }],
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "drone".to_owned(),
                    object_class: Some("Unit".to_owned()),
                    ..ProtoObject::default()
                },
            ],
            squads: vec![ProtoSquad {
                name: "drone_squad".to_owned(),
                build_points: Some(10.0),
                units: Some(UnitsWrapper {
                    entries: vec![UnitEntry {
                        proto_object: "drone".to_owned(),
                        count: 1,
                        ..UnitEntry::default()
                    }],
                }),
                ..ProtoSquad::default()
            }],
            ..Database::default()
        }
    }
}
