use super::*;
use crate::trigger::{ConditionType, TriggerVar, TriggerVec3, VarType};
use glam::Vec3;

#[test]
fn compare_vector_uses_component_epsilon_and_retail_default() {
    let mut script = TriggerScript::new(1);
    script.add_variable(
        TriggerVar::new(1, VarType::Vector)
            .with_value(TriggerValue::Vector(TriggerVec3::new(1.0, 2.0, 3.0))),
    );
    script.add_variable(
        TriggerVar::new(2, VarType::Vector).with_value(TriggerValue::Vector(TriggerVec3::new(
            1.0 + 0.000_000_5,
            2.0,
            3.0,
        ))),
    );
    script.add_variable(
        TriggerVar::new(3, VarType::Float).with_value(TriggerValue::Float(0.000_003)),
    );
    let default_epsilon = Condition::new(1, ConditionType::CompareVector)
        .with_input_at(1, 1)
        .with_input_at(2, 2);

    assert!(compare_vector(&default_epsilon, &script));

    script.get_variable_mut(2).unwrap().value =
        TriggerValue::Vector(TriggerVec3::new(1.0 + 0.000_002, 2.0, 3.0));
    assert!(!compare_vector(&default_epsilon, &script));

    let custom_epsilon = default_epsilon.clone().with_input_at(3, 3);
    assert!(compare_vector(&custom_epsilon, &script));
}

#[test]
fn squad_location_distance_is_euclidean_in_the_world_xz_plane() {
    let mut world = World::new();
    world.init_players(1);
    let squad_id = world.create_squad_at(1, Vec3::new(3.0, 100.0, 4.0));
    let mut script = TriggerScript::new(1);
    script
        .add_variable(TriggerVar::new(1, VarType::Squad).with_value(TriggerValue::Squad(squad_id)));
    script.add_variable(
        TriggerVar::new(2, VarType::UILocation)
            .with_value(TriggerValue::Location(TriggerVec3::new(0.0, -500.0, 0.0))),
    );
    script.add_variable(TriggerVar::new(3, VarType::Operator).with_value(TriggerValue::Int(3)));
    script.add_variable(TriggerVar::new(4, VarType::Float).with_value(TriggerValue::Float(5.0)));
    let condition = Condition::new(1, ConditionType::SquadLocationDistance)
        .with_input_at(1, 1)
        .with_input_at(2, 2)
        .with_input_at(3, 3)
        .with_input_at(4, 4);

    assert!(squad_location_distance(&condition, &script, &world));
}

fn design_sphere(type_name: &str, center: Vec3, radius: f32) -> crate::world::DesignSphere {
    crate::world::DesignSphere {
        type_name: type_name.to_owned(),
        center,
        radius,
    }
}

fn design_sphere_script(type_value: Option<&str>) -> TriggerScript {
    let mut script = TriggerScript::new(1);
    let type_var = TriggerVar::new(1, VarType::String).with_value(TriggerValue::String(
        type_value.unwrap_or_default().to_owned(),
    ));
    script.add_variable(if type_value.is_some() {
        type_var
    } else {
        type_var.null()
    });
    script.add_variable(
        TriggerVar::new(2, VarType::VectorList)
            .with_value(TriggerValue::VectorList(vec![TriggerVec3::zero()])),
    );
    script.add_variable(
        TriggerVar::new(3, VarType::FloatList).with_value(TriggerValue::FloatList(vec![9.0])),
    );
    script
}

/// `BTriggerCondition_tcCanGetDesignSpheres` matches the type without case and with the same
/// length, keeps the sphere order, and always overwrites both output lists.
#[test]
fn can_get_design_spheres_filters_by_type_and_overwrites_outputs() {
    let mut world = World::new();
    world.configure_design_objects(
        vec![
            design_sphere("Creeps", Vec3::new(1.0, 2.0, 3.0), 55.0),
            design_sphere("CreepsX", Vec3::new(4.0, 5.0, 6.0), 20.0),
            design_sphere("creeps", Vec3::new(7.0, 8.0, 9.0), 35.0),
        ],
        Vec::new(),
    );
    let condition = Condition::new(1, ConditionType::CanGetDesignSpheres)
        .with_input_at(1, 1)
        .with_output_at(2, 2)
        .with_output_at(3, 3);

    let mut script = design_sphere_script(Some("CREEPS"));
    assert!(can_get_design_spheres(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::VectorList(vec![
            TriggerVec3::new(1.0, 2.0, 3.0),
            TriggerVec3::new(7.0, 8.0, 9.0),
        ])
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::FloatList(vec![55.0, 35.0])
    );

    let mut script = design_sphere_script(None);
    assert!(can_get_design_spheres(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::FloatList(vec![55.0, 20.0, 35.0])
    );

    let mut script = design_sphere_script(Some("Spawn"));
    assert!(!can_get_design_spheres(&condition, &mut script, &world));
    assert_eq!(
        script.get_variable(2).unwrap().value,
        TriggerValue::VectorList(Vec::new())
    );
    assert_eq!(
        script.get_variable(3).unwrap().value,
        TriggerValue::FloatList(Vec::new())
    );
}
