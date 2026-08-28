use super::*;
use pipeline::database::hw1::gamedata::PlayerStatesWrapper;
use pipeline::database::hw1::{Civ, GameData, Leader, ProtoObject, Squad, Tech};
use pipeline::xmb::Document;

#[test]
fn retail_variable_type_aliases_resolve_to_runtime_types() {
    for (name, expected) in [
        ("CommandType", VarType::TechDataCommandType),
        ("Diplomacy", VarType::RelationType),
        ("Direction", VarType::Vector),
        ("LocationList", VarType::VectorList),
    ] {
        assert_eq!(parse_var_type(name), Some(expected));
    }
}

#[test]
fn command_types_use_retail_proto_object_command_ordinals() {
    for (name, expected) in [
        ("Research", 0),
        ("TrainSquad", 3),
        ("BuildOther", 13),
        ("reversehotdrop", 20),
        ("Unknown", -1),
    ] {
        assert_eq!(
            parse_var_value(
                name,
                VarType::TechDataCommandType,
                TriggerLoadContext::default(),
            ),
            TriggerValue::Int(expected),
        );
    }
}

#[test]
fn list_positions_use_retail_ordinals_and_default_to_first() {
    for (name, expected) in [
        ("First", 0),
        ("last", 1),
        ("Random", 2),
        ("2", 2),
        ("Unknown", 0),
    ] {
        assert_eq!(
            parse_var_value(name, VarType::ListPosition, TriggerLoadContext::default()),
            TriggerValue::Int(expected),
        );
    }
}

#[test]
fn vector_lists_load_the_retail_pipe_delimited_format() {
    assert_eq!(
        parse_var_value(
            "1,2,3|-4,5.5,6|",
            VarType::VectorList,
            TriggerLoadContext::default(),
        ),
        TriggerValue::VectorList(vec![Vec3::new(1.0, 2.0, 3.0), Vec3::new(-4.0, 5.5, 6.0),]),
    );
    assert_eq!(
        parse_var_value("", VarType::VectorList, TriggerLoadContext::default()),
        TriggerValue::VectorList(Vec::new()),
    );
}

#[test]
fn object_type_values_keep_retail_authored_names() {
    assert_eq!(
        parse_var_value(
            " Infantry ",
            VarType::ObjectType,
            TriggerLoadContext::default()
        ),
        TriggerValue::ObjectType("Infantry".to_owned())
    );
    assert_eq!(
        parse_var_value(
            "Infantry, unsc_inf_marine_01, infantry, ",
            VarType::ObjectTypeList,
            TriggerLoadContext::default(),
        ),
        TriggerValue::ObjectTypeList(vec!["Infantry".to_owned(), "unsc_inf_marine_01".to_owned(),])
    );
}

#[test]
fn runtime_container_variables_load_as_typed_empty_state() {
    assert_eq!(
        parse_var_value("", VarType::Iterator, TriggerLoadContext::default()),
        TriggerValue::Iterator(super::super::TriggerIterator::default())
    );
    assert_eq!(
        parse_var_value("", VarType::EntityFilterSet, TriggerLoadContext::default()),
        TriggerValue::EntityFilterSet(super::super::EntityFilterSet::default())
    );
    assert_eq!(
        parse_var_value("", VarType::PlayerList, TriggerLoadContext::default()),
        TriggerValue::PlayerList(Vec::new())
    );
    assert_eq!(
        parse_var_value("", VarType::TeamList, TriggerLoadContext::default()),
        TriggerValue::TeamList(Vec::new())
    );
    assert_eq!(
        parse_var_value("", VarType::IntegerList, TriggerLoadContext::default()),
        TriggerValue::IntegerList(Vec::new())
    );
    assert_eq!(
        parse_var_value("", VarType::Cost, TriggerLoadContext::default()),
        TriggerValue::Cost(Cost::default())
    );
    assert_eq!(
        parse_var_value("", VarType::ProtoSquadList, TriggerLoadContext::default()),
        TriggerValue::ProtoSquadList(Vec::new())
    );
    assert_eq!(
        parse_var_value(
            "Temp",
            VarType::AISquadAnalysis,
            TriggerLoadContext::default()
        ),
        TriggerValue::AISquadAnalysis(super::super::AISquadAnalysis::default())
    );
    assert_eq!(
        parse_var_value(
            "CVStarsMediumAir",
            VarType::AISquadAnalysisComponent,
            TriggerLoadContext::default(),
        ),
        TriggerValue::AISquadAnalysisComponent(
            super::super::AISquadAnalysisComponent::CVStarsMediumAir
        )
    );
}

#[test]
fn prototype_lists_resolve_names_with_retail_multiplicity() {
    let mut database = Database::new();
    database.objects.push(ProtoObject {
        name: "marine".to_owned(),
        dbid: Some(101),
        ..ProtoObject::default()
    });
    database.squads.push(Squad {
        name: "marine_squad".to_owned(),
        dbid: Some(201),
        ..Squad::default()
    });
    database.techs.push(Tech {
        name: "upgrade_a".to_owned(),
        ..Tech::default()
    });
    let context = TriggerLoadContext {
        database: Some(&database),
        ..TriggerLoadContext::default()
    };

    assert_eq!(
        parse_var_value(
            "marine_squad, marine_squad",
            VarType::ProtoSquadList,
            context,
        ),
        TriggerValue::ProtoSquadList(vec![201, 201])
    );
    assert_eq!(
        parse_var_value("marine,marine", VarType::ProtoObjectList, context),
        TriggerValue::ProtoObjectList(vec![101, 101])
    );
    assert_eq!(
        parse_var_value("upgrade_a,UPGRADE_A", VarType::TechList, context),
        TriggerValue::TechList(vec![0])
    );
}

#[test]
fn loader_preserves_types_for_empty_non_null_variables() {
    let document = Document::from_xml(
        r#"<TriggerSystem Name="EmptyOutputs" Type="Scenario">
            <TriggerVars>
                <TriggerVar ID="1" Type="PlayerList"></TriggerVar>
                <TriggerVar ID="2" Type="TeamList"></TriggerVar>
                <TriggerVar ID="3" Type="Cost"></TriggerVar>
                <TriggerVar ID="4" Type="Vector"></TriggerVar>
            </TriggerVars>
            <Triggers />
        </TriggerSystem>"#,
    )
    .expect("valid trigger XML");
    let script = VanillaLoader::from_xmb(&document).expect("typed empty variables");

    assert_eq!(
        script.get_variable(1).map(|variable| &variable.value),
        Some(&TriggerValue::PlayerList(Vec::new()))
    );
    assert_eq!(
        script.get_variable(2).map(|variable| &variable.value),
        Some(&TriggerValue::TeamList(Vec::new()))
    );
    assert_eq!(
        script.get_variable(3).map(|variable| &variable.value),
        Some(&TriggerValue::Cost(Cost::default()))
    );
    assert_eq!(
        script.get_variable(4).map(|variable| &variable.value),
        Some(&TriggerValue::Vector(Vec3::default()))
    );
}

#[test]
fn database_and_enum_variables_resolve_to_retail_numeric_identities() {
    let mut database = Database::new();
    database.civs.push(Civ {
        name: "UNSC".to_owned(),
        ..Civ::default()
    });
    database.leaders.push(Leader {
        name: "Cutter".to_owned(),
        ..Leader::default()
    });
    database.game_data = Some(GameData {
        player_states: Some(PlayerStatesWrapper {
            entries: vec!["Playing".to_owned(), "Defeated".to_owned()],
        }),
        ..GameData::default()
    });
    let context = TriggerLoadContext {
        database: Some(&database),
        ..TriggerLoadContext::default()
    };

    for (text, var_type, expected) in [
        ("UNSC", VarType::Civ, 0),
        ("cutter", VarType::Leader, 0),
        ("Defeated", VarType::PlayerState, 1),
        ("Active", VarType::TechStatus, 4),
        ("Enemy", VarType::RelationType, 3),
        ("Cover", VarType::SquadMode, 6),
        ("Legendary", VarType::Difficulty, 3),
    ] {
        assert_eq!(
            parse_var_value(text, var_type, context),
            TriggerValue::Int(expected)
        );
    }
    assert_eq!(parse_relation_type("unknown"), 0);
}
