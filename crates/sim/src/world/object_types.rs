//! Retail object and squad prototype identity projected into the sim.

use super::World;
use crate::entity_id::{EntityClass, EntityId};
use crate::gameplay::GameplayCatalog;
use crate::prototypes::{proto_id, proto_squads};
use crate::sync::SyncChecksum;
use pipeline::database::hw1::{Database, ObjectType, ObjectTypeIndex, ProtoObject};

impl World {
    /// Project the proto object types and the engine proto squad table (with
    /// merged and implicit squads) into the world, keyed by proto ID.
    ///
    /// Like `BProtoObject_load` and `BDatabase_setupDecomposedObjectTypes`, a proto keeps only
    /// the `<ObjectType>` names that `objecttypes.xml` declares. An undeclared name (such as
    /// `Icon`) or an object name does not become a type of the proto.
    pub fn configure_prototype_catalogs(&mut self, database: &Database) {
        let index = database.object_type_index();
        self.prototype_object_types = database
            .objects
            .iter()
            .enumerate()
            .map(|(position, prototype)| {
                let names = object_type_names(prototype, &index, &database.object_types);
                (proto_id(position), names)
            })
            .collect();
        self.prototype_squads = proto_squads(database)
            .map(|(id, entry)| (id, (entry.name().trim().to_owned(), entry.size(database))))
            .collect();
    }

    pub(crate) fn configure_prototype_damage_profiles(&mut self, gameplay: &GameplayCatalog) {
        self.prototype_shield_coverages = gameplay
            .shield_coverages()
            .map(|(name, coverage)| (name.to_owned(), coverage))
            .collect();
    }

    pub(crate) fn configure_prototype_vehicle_physics(&mut self, gameplay: &GameplayCatalog) {
        self.prototype_ground_vehicle_physics = gameplay
            .ground_vehicle_physics_profiles()
            .map(|(name, profile)| (name.to_owned(), profile.clone()))
            .collect();
        self.prototype_flight_controllers = gameplay
            .flight_controller_profiles()
            .map(|(name, profile)| (name.to_owned(), profile))
            .collect();
    }

    /// Return the scenario-layered shield coverage used by future unit spawns.
    #[must_use]
    pub fn prototype_shield_coverage(
        &self,
        proto_object_name: &str,
    ) -> Option<crate::entities::ShieldCoverage> {
        self.prototype_shield_coverages
            .get(&proto_object_name.to_ascii_lowercase())
            .copied()
    }

    pub(crate) fn prototype_ground_vehicle_physics(
        &self,
        proto_object_name: &str,
    ) -> Option<&crate::gameplay::GroundVehiclePhysicsProfile> {
        self.prototype_ground_vehicle_physics
            .get(&proto_object_name.to_ascii_lowercase())
    }

    pub(crate) fn prototype_flight_controller(
        &self,
        proto_object_name: &str,
    ) -> crate::entities::FlightControllerKind {
        self.prototype_flight_controllers
            .get(&proto_object_name.to_ascii_lowercase())
            .map_or(crate::entities::FlightControllerKind::Direct, |profile| {
                profile.resolve(self.is_config_defined("EnableFlight"))
            })
    }

    /// Test whether a proto-object has a concrete or abstract object type.
    #[must_use]
    pub fn prototype_is_object_type(&self, prototype_id: i32, object_type: &str) -> bool {
        self.prototype_object_type_match(prototype_id, object_type)
            .unwrap_or(false)
    }

    pub(crate) fn prototype_object_type_match(
        &self,
        prototype_id: i32,
        object_type: &str,
    ) -> Option<bool> {
        self.prototype_object_types
            .get(&prototype_id)
            .map(|types| contains_type(types, object_type))
    }

    /// Return the declared object types of the named proto object, without its own name.
    ///
    /// A unit uses the types of its effective (player-transformed) proto, like the type test of
    /// the engine on the player proto object.
    pub(crate) fn prototype_declared_object_types(&self, prototype_name: &str) -> Vec<String> {
        self.prototype_object_types
            .values()
            .find(|names| {
                names
                    .first()
                    .is_some_and(|name| name.eq_ignore_ascii_case(prototype_name.trim()))
            })
            .map(|names| names.iter().skip(1).cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn prototype_name_is_object_type(
        &self,
        prototype_name: &str,
        object_type: &str,
    ) -> bool {
        let prototype_name = prototype_name.trim();
        prototype_name.eq_ignore_ascii_case(object_type.trim())
            || self.prototype_object_types.values().any(|types| {
                types.first().is_some_and(|concrete_name| {
                    concrete_name.eq_ignore_ascii_case(prototype_name)
                        && contains_type(types, object_type)
                })
            })
    }

    /// Test whether a live squad currently has its prototype's authored child count.
    #[must_use]
    pub fn squad_is_at_max_size(&self, squad_id: EntityId) -> bool {
        self.get_squad(squad_id).is_some_and(|squad| {
            let logical_name = if squad.proto_squad_name.trim().is_empty() {
                self.prototype_squads
                    .get(&squad.proto_squad_id)
                    .map_or("", |(name, _)| name.as_str())
            } else {
                squad.proto_squad_name.trim()
            };
            let effective_name = self
                .get_player(squad.base.player_id)
                .map_or(logical_name, |player| {
                    player.technologies.resolved_squad_prototype(logical_name)
                });
            self.prototype_squads
                .values()
                .find(|(name, _)| name.eq_ignore_ascii_case(effective_name))
                .or_else(|| self.prototype_squads.get(&squad.proto_squad_id))
                .is_some_and(|(_, maximum)| {
                    u32::try_from(squad.unit_ids.len()).unwrap_or(u32::MAX) == *maximum
                })
        })
    }

    pub(crate) fn unit_object_type_match(
        &self,
        unit_id: EntityId,
        object_type: &str,
    ) -> Option<bool> {
        let unit = self.get_unit(unit_id)?;
        (!unit.proto_object_name.is_empty() || !unit.object_types.is_empty())
            .then(|| unit.is_object_type(object_type))
    }

    pub(crate) fn squad_object_type_match(
        &self,
        squad_id: EntityId,
        object_type: &str,
    ) -> Option<bool> {
        let squad = self.get_squad(squad_id)?;
        squad
            .unit_ids
            .iter()
            .find_map(|unit_id| self.unit_object_type_match(*unit_id, object_type))
    }

    pub(crate) fn entity_object_type_match(
        &self,
        entity_id: EntityId,
        object_type: &str,
    ) -> Option<bool> {
        match entity_id.class() {
            Some(EntityClass::Object) => self.get_object(entity_id).and_then(|value| {
                self.prototype_object_type_match(value.proto_object_id, object_type)
            }),
            Some(EntityClass::Unit) => self.unit_object_type_match(entity_id, object_type),
            Some(EntityClass::Squad) => self.squad_object_type_match(entity_id, object_type),
            Some(EntityClass::Projectile) => self.get_projectile(entity_id).and_then(|value| {
                self.prototype_object_type_match(value.proto_object_id, object_type)
            }),
            _ => None,
        }
    }

    pub(super) fn hash_prototype_catalogs(&self, checksum: &mut SyncChecksum) {
        checksum.hash_u32(u32::try_from(self.prototype_object_types.len()).unwrap_or(u32::MAX));
        for (prototype_id, object_types) in &self.prototype_object_types {
            checksum.hash_i32(*prototype_id);
            checksum.hash_u32(u32::try_from(object_types.len()).unwrap_or(u32::MAX));
            for object_type in object_types {
                checksum.hash_string(object_type);
            }
        }
        checksum.hash_u32(u32::try_from(self.prototype_squads.len()).unwrap_or(u32::MAX));
        for (prototype_id, (prototype_name, maximum_size)) in &self.prototype_squads {
            checksum.hash_i32(*prototype_id);
            checksum.hash_string(prototype_name);
            checksum.hash_u32(*maximum_size);
        }
        checksum.hash_u32(u32::try_from(self.prototype_shield_coverages.len()).unwrap_or(u32::MAX));
        for (prototype_name, coverage) in &self.prototype_shield_coverages {
            checksum.hash_string(prototype_name);
            checksum.hash_u32(*coverage as u32);
        }
        checksum.hash_u32(
            u32::try_from(self.prototype_ground_vehicle_physics.len()).unwrap_or(u32::MAX),
        );
        for (prototype_name, profile) in &self.prototype_ground_vehicle_physics {
            checksum.hash_string(prototype_name);
            profile.hash_state(checksum);
        }
        checksum
            .hash_u32(u32::try_from(self.prototype_flight_controllers.len()).unwrap_or(u32::MAX));
        for (prototype_name, profile) in &self.prototype_flight_controllers {
            checksum.hash_string(prototype_name);
            profile.hash_state(checksum);
        }
    }
}

/// Return the proto name followed by its declared object types in type ID order.
fn object_type_names(
    prototype: &ProtoObject,
    index: &ObjectTypeIndex,
    object_types: &[ObjectType],
) -> Vec<String> {
    let mut names = Vec::with_capacity(prototype.object_types.len() + 1);
    add_type(&mut names, &prototype.name);
    for type_index in index.types_of(prototype) {
        add_type(&mut names, &object_types[type_index].name);
    }
    names
}

fn add_type(names: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.is_empty() || contains_type(names, value) {
        return;
    }
    names.push(value.to_owned());
}

fn contains_type(names: &[String], expected: &str) -> bool {
    names
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(expected.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeline::database::hw1::Squad as ProtoSquad;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};

    #[test]
    fn catalog_includes_concrete_and_abstract_types_in_the_world_checksum() {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "unsc_inf_marine_01".to_owned(),
            dbid: Some(17),
            object_class: Some("Unit".to_owned()),
            object_types: vec!["Infantry".to_owned(), "UNSC".to_owned(), "Icon".to_owned()],
            ..ProtoObject::default()
        });
        database.squads.push(ProtoSquad {
            name: "unsc_marine_squad".to_owned(),
            dbid: Some(70),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "unsc_inf_marine_01".to_owned(),
                    count: 2,
                    ..UnitEntry::default()
                }],
            }),
            ..ProtoSquad::default()
        });
        // `objecttypes.xml` declares Infantry and UNSC but not Icon.
        for name in ["Infantry", "UNSC"] {
            database.object_types.push(ObjectType {
                name: name.to_owned(),
            });
        }
        let mut world = World::new();
        let empty_checksum = world.checksum();
        world.configure_prototype_catalogs(&database);

        // Proto IDs are table indices; the dbid values are not IDs.
        assert!(world.prototype_is_object_type(0, "UNSC_INF_MARINE_01"));
        assert!(world.prototype_is_object_type(0, "infantry"));
        assert!(!world.prototype_is_object_type(0, "Vehicle"));
        assert!(!world.prototype_is_object_type(17, "infantry"));
        // `BProtoObject_load` drops an undeclared type name.
        assert!(!world.prototype_is_object_type(0, "Icon"));
        assert_eq!(
            world.prototype_declared_object_types("unsc_inf_marine_01"),
            ["Infantry", "UNSC"]
        );
        world.init_players(1);
        let squad_id = world.create_squad(1);
        world.get_squad_mut(squad_id).unwrap().proto_squad_id = 0;
        let first = world.create_unit(1);
        assert!(world.attach_unit_to_squad(first, squad_id));
        assert!(!world.squad_is_at_max_size(squad_id));
        let second = world.create_unit(1);
        assert!(world.attach_unit_to_squad(second, squad_id));
        assert!(world.squad_is_at_max_size(squad_id));
        assert_ne!(world.checksum(), empty_checksum);
    }

    #[test]
    fn implicit_proto_squads_hold_one_unit() {
        let database = Database {
            objects: vec![
                ProtoObject {
                    name: "unsc_inf_marine_01".to_owned(),
                    object_class: Some("Unit".to_owned()),
                    ..ProtoObject::default()
                },
                ProtoObject {
                    name: "unsc_bldg_barracks_01".to_owned(),
                    object_class: Some("Building".to_owned()),
                    ..ProtoObject::default()
                },
            ],
            squads: vec![ProtoSquad {
                name: "unsc_marine_squad".to_owned(),
                ..ProtoSquad::default()
            }],
            ..Database::default()
        };
        let mut world = World::new();
        world.configure_prototype_catalogs(&database);
        world.init_players(1);
        let squad_id = world.create_squad(1);
        let squad = world.get_squad_mut(squad_id).unwrap();
        // Squad 0 is the record; 1 and 2 are the implicit squads of objects 0 and 1.
        squad.proto_squad_id = 2;
        "unsc_bldg_barracks_01".clone_into(&mut squad.proto_squad_name);
        let building = world.create_building(1);
        assert!(world.attach_unit_to_squad(building, squad_id));
        assert!(world.squad_is_at_max_size(squad_id));
    }
}
