//! Retail trigger effect 71 (`CarpetBomb`) versions 3 and 4.

use super::support::{float_at, integer_at, vector_at};
use super::{EffectOutcome, value_at};
use crate::trigger::{Effect, EffectType, TriggerScript, TriggerValue};
use crate::{EntityId, World};
use glam::Vec3;
use pipeline::database::hw1::{Database, ProtoObject};

const DEFAULT_ATTACK_DISTANCE: f32 = 20.0;
const DEFAULT_ATTACK_COUNT: i32 = 1;

pub(super) fn execute(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> Option<EffectOutcome> {
    (effect.effect_type == EffectType::CarpetBomb).then(|| apply(effect, script, world, database))
}

pub(super) fn apply(
    effect: &Effect,
    script: &TriggerScript,
    world: &mut World,
    database: Option<&Database>,
) -> EffectOutcome {
    if !matches!(effect.version, 3 | 4) {
        return EffectOutcome::Unsupported(effect.raw_type);
    }
    let Some(database) = database else {
        return EffectOutcome::Skipped;
    };
    let Some(target_position) =
        vector_at(effect, script, 2).filter(|position| position.is_finite())
    else {
        return EffectOutcome::Skipped;
    };
    let attack_distance = float_at(effect, script, 4).unwrap_or(DEFAULT_ATTACK_DISTANCE);
    if !attack_distance.is_finite() {
        return EffectOutcome::Skipped;
    }
    let attack_count = integer_at(effect, script, 5).unwrap_or(DEFAULT_ATTACK_COUNT);
    let source = if effect.version == 3 {
        version_three_source(effect, script, world, database)
    } else {
        version_four_source(effect, script, world, database)
    };
    let Some((squad_id, launch_position)) = source else {
        return EffectOutcome::Skipped;
    };
    if world.issue_squad_carpet_bomb(
        squad_id,
        target_position,
        launch_position,
        attack_distance,
        attack_count,
    ) {
        EffectOutcome::Applied
    } else {
        EffectOutcome::Skipped
    }
}

fn version_three_source(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
    database: &Database,
) -> Option<(EntityId, Option<Vec3>)> {
    let unit_id = entity_at(effect, script, 3)?;
    let unit = world.get_unit(unit_id)?;
    let prototype = find_object(database, &unit.proto_object_name)?;
    if !object_is_flying(prototype) {
        return None;
    }
    let squad = world.get_squad(unit.squad_id?)?;
    Some((squad.base.id, Some(squad.base.position)))
}

fn version_four_source(
    effect: &Effect,
    script: &TriggerScript,
    world: &World,
    database: &Database,
) -> Option<(EntityId, Option<Vec3>)> {
    let squad_id = entity_at(effect, script, 6)?;
    let squad = world.get_squad(squad_id)?;
    let prototype = database
        .squads
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(&squad.proto_squad_name))?;
    if !prototype
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("Flying"))
    {
        return None;
    }
    if squad.unit_ids.iter().any(|unit_id| {
        world
            .get_unit(*unit_id)
            .is_some_and(crate::entities::Unit::is_move_air_returning_to_base)
    }) {
        return None;
    }
    let launch_position = squad
        .unit_ids
        .first()
        .and_then(|unit_id| world.get_unit(*unit_id))
        .and_then(crate::entities::Unit::move_air_base_position);
    Some((squad_id, launch_position))
}

fn entity_at(effect: &Effect, script: &TriggerScript, signature_id: u16) -> Option<EntityId> {
    match value_at(effect, script, signature_id)? {
        TriggerValue::Unit(value) | TriggerValue::Squad(value) => Some(*value),
        _ => None,
    }
}

fn find_object<'database>(
    database: &'database Database,
    name: &str,
) -> Option<&'database ProtoObject> {
    database
        .objects
        .iter()
        .find(|prototype| prototype.name.eq_ignore_ascii_case(name))
}

fn object_is_flying(object: &ProtoObject) -> bool {
    object
        .movement_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Air"))
        || object
            .object_types
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case("Flying"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{FlightControllerKind, SquadCarpetBombPhase};
    use crate::trigger::{EffectType, TriggerVar, TriggerVec3, VarType};
    use pipeline::database::hw1::{ProtoObject, Squad as ProtoSquad};

    const LOCATION_VAR: u32 = 1;
    const SOURCE_VAR: u32 = 2;
    const DISTANCE_VAR: u32 = 3;
    const COUNT_VAR: u32 = 4;

    #[test]
    fn version_three_uses_the_parent_squad_position_and_retail_defaults() {
        let (mut world, database, squad_id, unit_id) = flying_world(true);
        let target = Vec3::new(20.0, 2.0, 0.0);
        let script = script_with_source(TriggerValue::Unit(unit_id), target);
        let effect = effect(3, 3, false);

        assert_eq!(
            apply(&effect, &script, &mut world, Some(&database)),
            EffectOutcome::Applied
        );
        assert_eq!(
            world.get_squad(squad_id).unwrap().carpet_bomb_phase(),
            SquadCarpetBombPhase::Preparing
        );
        world.prepare_squad_carpet_bombs();
        assert_eq!(
            world
                .get_squad(squad_id)
                .unwrap()
                .carpet_bomb_attack_position(),
            Some(target)
        );
        assert!(world.get_squad(squad_id).unwrap().ignores_leash());
    }

    #[test]
    fn version_four_uses_the_first_move_air_child_base_and_authored_spacing() {
        let (mut world, database, squad_id, _) = flying_world(true);
        let target = Vec3::X * 20.0;
        let mut script = script_with_source(TriggerValue::Squad(squad_id), target);
        script.add_variable(
            TriggerVar::new(DISTANCE_VAR, VarType::Float).with_value(TriggerValue::Float(10.0)),
        );
        script.add_variable(
            TriggerVar::new(COUNT_VAR, VarType::Integer).with_value(TriggerValue::Int(3)),
        );
        let effect = effect(4, 6, true);

        assert_eq!(
            apply(&effect, &script, &mut world, Some(&database)),
            EffectOutcome::Applied
        );
        world.prepare_squad_carpet_bombs();
        assert_eq!(
            world
                .get_squad(squad_id)
                .unwrap()
                .carpet_bomb_attack_position(),
            Some(Vec3::X * 15.0)
        );
    }

    #[test]
    fn version_four_preserves_lead_only_base_lookup_and_rejects_returning_children() {
        let (mut world, database, squad_id, move_air_id) = flying_world(false);
        let script = script_with_source(TriggerValue::Squad(squad_id), Vec3::X * 20.0);
        let effect = effect(4, 6, false);

        assert_eq!(
            apply(&effect, &script, &mut world, Some(&database)),
            EffectOutcome::Applied
        );
        world.prepare_squad_carpet_bombs();
        assert_eq!(
            world.get_squad(squad_id).unwrap().carpet_bomb_phase(),
            SquadCarpetBombPhase::Inactive
        );

        world
            .get_unit_mut(move_air_id)
            .unwrap()
            .request_move_air_return();
        assert_eq!(
            apply(&effect, &script, &mut world, Some(&database)),
            EffectOutcome::Skipped
        );
    }

    #[test]
    fn exact_versions_and_flying_requirements_are_enforced() {
        let (mut world, mut database, squad_id, unit_id) = flying_world(true);
        let unit_script = script_with_source(TriggerValue::Unit(unit_id), Vec3::X);
        let squad_script = script_with_source(TriggerValue::Squad(squad_id), Vec3::X);
        let mut obsolete = effect(2, 3, false);
        assert_eq!(
            apply(&obsolete, &unit_script, &mut world, Some(&database)),
            EffectOutcome::Unsupported(EffectType::CarpetBomb as u16)
        );

        database.objects[0].movement_type = Some("Land".to_owned());
        database.objects[0].object_types.clear();
        assert_eq!(
            apply(
                &effect(3, 3, false),
                &unit_script,
                &mut world,
                Some(&database),
            ),
            EffectOutcome::Skipped
        );
        database.squads[0].flags.clear();
        assert_eq!(
            apply(
                &effect(4, 6, false),
                &squad_script,
                &mut world,
                Some(&database),
            ),
            EffectOutcome::Skipped
        );
        obsolete.version = 5;
        assert_eq!(
            apply(&obsolete, &unit_script, &mut world, Some(&database)),
            EffectOutcome::Unsupported(EffectType::CarpetBomb as u16)
        );
    }

    fn flying_world(lead_uses_move_air: bool) -> (World, Database, EntityId, EntityId) {
        let mut database = Database::new();
        database.objects.push(ProtoObject {
            name: "bomber".to_owned(),
            movement_type: Some("Air".to_owned()),
            object_types: vec!["Flying".to_owned()],
            ..ProtoObject::default()
        });
        database.squads.push(ProtoSquad {
            name: "bomber_squad".to_owned(),
            flags: vec!["Flying".to_owned()],
            ..ProtoSquad::default()
        });
        let mut world = World::new();
        world.init_players(1);
        let squad_id = world.create_squad_at(1, Vec3::ZERO);
        world.get_squad_mut(squad_id).unwrap().proto_squad_name = "bomber_squad".to_owned();
        let first_id = world.create_unit_at(1, Vec3::ZERO);
        world.get_unit_mut(first_id).unwrap().proto_object_name = "bomber".to_owned();
        if lead_uses_move_air {
            world
                .get_unit_mut(first_id)
                .unwrap()
                .configure_flight_controller(FlightControllerKind::MoveAir, 0.0);
        }
        assert!(world.attach_unit_to_squad(first_id, squad_id));
        let move_air_id = if lead_uses_move_air {
            first_id
        } else {
            let unit_id = world.create_unit_at(1, Vec3::ZERO);
            let unit = world.get_unit_mut(unit_id).unwrap();
            unit.proto_object_name = "bomber".to_owned();
            unit.configure_flight_controller(FlightControllerKind::MoveAir, 0.0);
            assert!(world.attach_unit_to_squad(unit_id, squad_id));
            unit_id
        };
        world.update_entities(0.05);
        (world, database, squad_id, move_air_id)
    }

    fn script_with_source(source: TriggerValue, location: Vec3) -> TriggerScript {
        let mut script = TriggerScript::default();
        script.add_variable(TriggerVar::new(LOCATION_VAR, VarType::Vector).with_value(
            TriggerValue::Location(TriggerVec3::new(location.x, location.y, location.z)),
        ));
        let source_type = if matches!(source, TriggerValue::Unit(_)) {
            VarType::Unit
        } else {
            VarType::Squad
        };
        script.add_variable(TriggerVar::new(SOURCE_VAR, source_type).with_value(source));
        script
    }

    fn effect(version: u8, source_slot: u16, authored_run: bool) -> Effect {
        let mut effect = Effect::new(1, EffectType::CarpetBomb)
            .with_input_at(2, LOCATION_VAR)
            .with_input_at(source_slot, SOURCE_VAR);
        if authored_run {
            effect = effect
                .with_input_at(4, DISTANCE_VAR)
                .with_input_at(5, COUNT_VAR);
        }
        effect.version = version;
        effect
    }
}
