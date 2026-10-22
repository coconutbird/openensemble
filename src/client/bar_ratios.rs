//! The fill ratios of the HP bars.
//!
//! `BHPBar_getSquadBarRatios` (`0x140042C20`) divides the live sums of the squad members by the
//! full sums of the protosquad: for each member entry of the protosquad, the count times the
//! maximum of the member proto. So dead members lower the bar. The shield sum counts only units
//! with a shield (`unit+1103 & 2`) and the ammunition sum only units with ammunition
//! (`unit+1103 & 8`). A full sum below 1e-6 gives a ratio of 0.

use sim::{Squad, Unit, World};

use super::catalog::ClientCatalog;

/// The smallest full sum that gives a ratio.
const MINIMUM_FULL_SUM: f32 = 0.000_001;

/// The fill ratios of a squad bar, 0 to 1 when no technology raised the maxima.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SquadBarRatios {
    /// Hit points.
    pub(crate) hitpoints: f32,
    /// Shield points.
    pub(crate) shield: f32,
    /// Ammunition.
    pub(crate) ammo: f32,
}

/// Returns the bar ratios of `squad`, like `BHPBar_getSquadBarRatios`.
///
/// The protosquad is the catalog protosquad with the proto name of the squad: a database squad
/// or the implicit protosquad of an object (one member). Returns `None` without a protosquad.
pub(crate) fn squad_bar_ratios(
    world: &World,
    catalog: &ClientCatalog,
    squad: &Squad,
) -> Option<SquadBarRatios> {
    let proto = catalog.squad(&squad.proto_squad_name)?;
    let members = proto
        .members
        .iter()
        .map(|(name, count)| (name.as_str(), *count));
    Some(ratios(world, squad, members, |name| {
        catalog.object(name).map(|object| object.maxima)
    }))
}

/// Returns the bar ratios of one unit, like `BHPBar_getUnitBarRatios` (`0x140043220`): hit
/// points over the maximum, and shield points over the maximum for a unit with a shield.
pub(crate) fn unit_bar_ratios(unit: &Unit) -> SquadBarRatios {
    let ratio = |value: f32, full: f32| {
        if full >= MINIMUM_FULL_SUM {
            value / full
        } else {
            0.0
        }
    };
    SquadBarRatios {
        hitpoints: ratio(unit.hitpoints, unit.max_hitpoints),
        shield: ratio(unit.shields.current, unit.shields.maximum),
        ammo: unit.ammunition.percentage(),
    }
}

/// Divides the live member sums by the full protosquad sums.
///
/// The maximum of a member proto is the live maximum of a member unit of that proto, which has
/// the technology changes of the player proto, or else `object_maxima` (the database values).
fn ratios<'a>(
    world: &World,
    squad: &Squad,
    members: impl Iterator<Item = (&'a str, i32)>,
    object_maxima: impl Fn(&str) -> Option<[f32; 3]>,
) -> SquadBarRatios {
    let units = || {
        squad
            .unit_ids
            .iter()
            .filter_map(|&unit_id| world.units.get(unit_id))
            .filter(|unit| unit.base.is_alive())
    };
    let mut full = [0.0_f32; 3];
    for (proto, count) in members {
        let maxima = units()
            .find(|unit| unit.proto_object_name == proto)
            .map(|unit| {
                [
                    unit.max_hitpoints,
                    unit.shields.maximum,
                    unit.ammunition.maximum(),
                ]
            })
            .or_else(|| object_maxima(proto))
            .unwrap_or_default();
        let count = f32::from(i16::try_from(count).unwrap_or(i16::MAX));
        for (sum, maximum) in full.iter_mut().zip(maxima) {
            *sum += count * maximum;
        }
    }
    let mut live = [0.0_f32; 3];
    for unit in units() {
        live[0] += unit.hitpoints;
        if unit.shields.maximum > 0.0 {
            live[1] += unit.shields.current;
        }
        if unit.ammunition.is_enabled() {
            live[2] += unit.ammunition.current();
        }
    }
    let ratio = |index: usize| {
        if full[index] >= MINIMUM_FULL_SUM {
            live[index] / full[index]
        } else {
            0.0
        }
    };
    SquadBarRatios {
        hitpoints: ratio(0),
        shield: ratio(1),
        ammo: ratio(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use pipeline::database::hw1::squads::{UnitEntry, UnitsWrapper};
    use pipeline::database::hw1::{Database, ProtoObject, Squad as ProtoSquad};
    use sim::EntityId;

    fn database() -> Database {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "marine".to_owned(),
            hitpoints: Some(100.0),
            ..ProtoObject::default()
        });
        database.objects.push(ProtoObject {
            name: "barracks".to_owned(),
            hitpoints: Some(1000.0),
            ..ProtoObject::default()
        });
        database.squads.push(ProtoSquad {
            name: "marines".to_owned(),
            units: Some(UnitsWrapper {
                entries: vec![UnitEntry {
                    proto_object: "marine".to_owned(),
                    count: 4,
                    role: None,
                }],
            }),
            ..ProtoSquad::default()
        });
        database
    }

    fn squad(world: &mut World, proto_squad: &str, proto: &str, hitpoints: &[f32]) -> EntityId {
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        proto_squad.clone_into(&mut world.get_squad_mut(squad_id).unwrap().proto_squad_name);
        for &value in hitpoints {
            let unit_id = world.create_unit_at(1, Vec3::ZERO);
            assert!(world.attach_unit_to_squad(unit_id, squad_id));
            let unit = world.get_unit_mut(unit_id).unwrap();
            proto.clone_into(&mut unit.proto_object_name);
            unit.max_hitpoints = 100.0;
            unit.hitpoints = value;
        }
        squad_id
    }

    fn ratios_of(world: &World, database: &Database, squad_id: EntityId) -> Option<SquadBarRatios> {
        let catalog = ClientCatalog::new(database);
        squad_bar_ratios(world, &catalog, world.squads.get(squad_id)?)
    }

    #[test]
    fn dead_members_lower_the_squad_bar() {
        let database = database();
        let mut world = World::new();
        world.init_players(1);
        // Two of four marines live, one at full and one at half health: 150 / 400.
        let squad_id = squad(&mut world, "marines", "marine", &[100.0, 50.0]);
        let ratios = ratios_of(&world, &database, squad_id).unwrap();
        assert!((ratios.hitpoints - 0.375).abs() < 1e-6);
        assert!(ratios.shield.abs() < f32::EPSILON);
    }

    #[test]
    fn an_object_squad_uses_the_implicit_protosquad() {
        let database = database();
        let mut world = World::new();
        world.init_players(1);
        let squad_id = squad(&mut world, "barracks", "barracks", &[50.0]);
        // The live member maximum (100) wins over the database value (1000).
        let ratios = ratios_of(&world, &database, squad_id).unwrap();
        assert!((ratios.hitpoints - 0.5).abs() < 1e-6);

        let unknown = squad(&mut world, "unknown", "unknown", &[50.0]);
        assert_eq!(ratios_of(&world, &database, unknown), None);
    }
}
