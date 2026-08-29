//! `VanillaLoader` - parses .triggerscript XMB files into core trigger types.
//!
//! This module handles loading vanilla Halo Wars trigger scripts from their
//! binary XMB format into the internal representation used by the trigger engine.

use num_traits::ToPrimitive;
use pipeline::database::hw1::Database;
use pipeline::xmb::{Document as XmbDocument, Reader as XmbReader};
use std::collections::HashMap;

use super::value::{Color, Cost, Vec3};
use super::{
    Condition, ConditionMode, ConditionType, Effect, EffectType, Trigger, TriggerScript,
    TriggerValue, TriggerVar, VarBinding, VarId, VarType,
};
use crate::EntityId;

mod value_types;

/// Errors that can occur when loading trigger scripts.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("XMB parse error: {0}")]
    XmbError(#[from] pipeline::xmb::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Missing required attribute: {0}")]
    MissingAttribute(String),

    #[error("Missing required element: {0}")]
    MissingElement(String),

    #[error("Invalid value for {field}: {value}")]
    InvalidValue { field: String, value: String },

    #[error("Unknown variable type: {0}")]
    UnknownVarType(String),

    #[error("Unknown condition type: {0}")]
    UnknownConditionType(i32),

    #[error("Unknown effect type: {0}")]
    UnknownEffectType(i32),
}

/// Result type for loader operations.
pub type LoadResult<T> = Result<T, LoadError>;

/// Runtime data used to resolve authored trigger variable values.
#[derive(Debug, Clone, Copy, Default)]
pub struct TriggerLoadContext<'a> {
    /// Scenario object IDs mapped to current generational simulation IDs.
    pub scenario_entities: Option<&'a HashMap<i32, EntityId>>,
    /// Active scenario-layered database used to resolve prototype names.
    pub database: Option<&'a Database>,
}

/// Loads vanilla .triggerscript XMB files into `TriggerScript` instances.
pub struct VanillaLoader;

impl VanillaLoader {
    /// Load a trigger script from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are not valid XMB or the script is malformed.
    pub fn load_bytes(data: &[u8]) -> LoadResult<TriggerScript> {
        let xmb = XmbReader::read(data)?;
        Self::from_xmb(&xmb)
    }

    /// Convert parsed XMB data into a `TriggerScript`.
    ///
    /// # Errors
    ///
    /// Returns an error if required script elements or attributes are missing or invalid.
    pub fn from_xmb(xmb: &XmbDocument) -> LoadResult<TriggerScript> {
        let root = xmb
            .root()
            .ok_or_else(|| LoadError::MissingElement("TriggerScript root".into()))?;

        Self::from_node(root)
    }

    /// Convert a `TriggerSystem` or standalone trigger-script root node.
    ///
    /// # Errors
    ///
    /// Returns an error when required authored fields are missing or invalid.
    pub fn from_node(root: &pipeline::xmb::Node) -> LoadResult<TriggerScript> {
        Self::from_node_with_context(root, TriggerLoadContext::default())
    }

    /// Convert a trigger root while resolving scenario and database references.
    ///
    /// # Errors
    ///
    /// Returns an error when required authored fields are missing or invalid.
    pub fn from_node_with_context(
        root: &pipeline::xmb::Node,
        context: TriggerLoadContext<'_>,
    ) -> LoadResult<TriggerScript> {
        let mut script = TriggerScript::default();

        // Parse script attributes
        if let Some(name_attr) = root.get_attribute("Name") {
            script.name = name_attr.value_string();
        }

        if let Some(type_attr) = root.get_attribute("Type") {
            script.script_type = parse_script_type(&type_attr.value_string());
        }

        // Parse TriggerVars
        if let Some(vars_node) = root.children.iter().find(|n| n.name == "TriggerVars") {
            for var_node in &vars_node.children {
                if var_node.name == "TriggerVar" {
                    let var = Self::parse_trigger_var(var_node, context)?;
                    if script.get_variable(var.id).is_some() {
                        return Err(LoadError::InvalidValue {
                            field: "TriggerVar.ID".into(),
                            value: var.id.to_string(),
                        });
                    }
                    script.add_variable(var);
                }
            }
        }

        // Parse Triggers
        if let Some(triggers_node) = root.children.iter().find(|n| n.name == "Triggers") {
            for trigger_node in &triggers_node.children {
                if trigger_node.name == "Trigger" {
                    let runtime_id = u32::try_from(script.triggers.len()).map_err(|_| {
                        LoadError::InvalidValue {
                            field: "Triggers".into(),
                            value: "too many triggers".into(),
                        }
                    })?;
                    let trigger = Self::parse_trigger(trigger_node, runtime_id)?;
                    script.add_trigger(trigger);
                }
            }
        }

        remap_trigger_editor_ids(&mut script);
        Ok(script)
    }

    /// Parse a `TriggerVar` node.
    fn parse_trigger_var(
        node: &pipeline::xmb::Node,
        context: TriggerLoadContext<'_>,
    ) -> LoadResult<TriggerVar> {
        let id: VarId = get_attr_u32(node, "ID")?;

        let type_str = get_attr_string(node, "Type")?;
        let var_type =
            parse_var_type(&type_str).ok_or_else(|| LoadError::UnknownVarType(type_str.clone()))?;

        let mut var = TriggerVar::new(id, var_type);

        if let Some(name_attr) = node.get_attribute("Name") {
            var.name = name_attr.value_string();
        }

        // Check if null - leave value as default
        if let Some(is_null_attr) = node.get_attribute("IsNull")
            && is_null_attr.value_string().eq_ignore_ascii_case("true")
        {
            var.is_null = true;
            return Ok(var);
        }

        // Parse even empty text so non-null containers and outputs retain their
        // authored runtime type instead of falling back to `Bool(false)`.
        let text = node.text_string();
        var.value = parse_var_value(&text, var_type, context);

        Ok(var)
    }

    /// Parse a Trigger node.
    fn parse_trigger(node: &pipeline::xmb::Node, runtime_id: u32) -> LoadResult<Trigger> {
        let editor_id = get_attr_u32(node, "ID")?;
        let mut trigger = Trigger::new(runtime_id);
        trigger.editor_id = editor_id;

        // Parse attributes
        if let Some(name_attr) = node.get_attribute("Name") {
            trigger.name = name_attr.value_string();
        }

        if let Some(active_attr) = node.get_attribute("Active") {
            trigger.start_active = active_attr.value_string().eq_ignore_ascii_case("true");
        }

        if let Some(freq_attr) = node.get_attribute("EvaluateFrequency") {
            trigger.evaluate_frequency = freq_attr.value_string().parse().unwrap_or(0);
        }

        if let Some(limit_attr) = node.get_attribute("EvalLimit") {
            trigger.evaluate_limit = limit_attr.value_string().parse().unwrap_or(0);
        }

        if let Some(cond_attr) = node.get_attribute("ConditionalTrigger") {
            trigger.is_conditional = cond_attr.value_string().eq_ignore_ascii_case("true");
        }

        if let Some(group_attr) = node.get_attribute("GroupID") {
            trigger.group_id = group_attr.value_string().parse().unwrap_or(-1);
        }

        // Parse conditions
        if let Some(conds_node) = node.children.iter().find(|n| n.name == "TriggerConditions") {
            Self::parse_conditions(conds_node, &mut trigger)?;
        }

        // Parse effects on true
        if let Some(effects_node) = node
            .children
            .iter()
            .find(|n| n.name == "TriggerEffectsOnTrue")
        {
            for effect_node in &effects_node.children {
                if effect_node.name == "Effect" {
                    let effect = Self::parse_effect(effect_node)?;
                    trigger.effects_on_true.push(effect);
                }
            }
        }

        // Parse effects on false
        if let Some(effects_node) = node
            .children
            .iter()
            .find(|n| n.name == "TriggerEffectsOnFalse")
        {
            for effect_node in &effects_node.children {
                if effect_node.name == "Effect" {
                    let effect = Self::parse_effect(effect_node)?;
                    trigger.effects_on_false.push(effect);
                }
            }
        }

        Ok(trigger)
    }

    /// Parse `TriggerConditions` node (contains And or Or wrapper).
    fn parse_conditions(node: &pipeline::xmb::Node, trigger: &mut Trigger) -> LoadResult<()> {
        // Find the And or Or wrapper node
        for child in &node.children {
            if child.name == "Or" {
                trigger.condition_mode = ConditionMode::Any;
                for cond_node in &child.children {
                    if cond_node.name == "Condition" {
                        let condition = Self::parse_condition(cond_node)?;
                        trigger.conditions.push(condition);
                    }
                }
                break;
            } else if child.name == "And" {
                trigger.condition_mode = ConditionMode::All;
                for cond_node in &child.children {
                    if cond_node.name == "Condition" {
                        let condition = Self::parse_condition(cond_node)?;
                        trigger.conditions.push(condition);
                    }
                }
                break;
            }
        }
        Ok(())
    }

    /// Parse a Condition node.
    fn parse_condition(node: &pipeline::xmb::Node) -> LoadResult<Condition> {
        let dbid = get_attr_i32(node, "DBID")?;
        let raw_type = u16::try_from(dbid).map_err(|_| LoadError::UnknownConditionType(dbid))?;
        let condition_type = ConditionType::from_u16(raw_type).unwrap_or(ConditionType::Custom);

        let id = node
            .get_attribute("ID")
            .map_or(0, |a| a.value_string().parse().unwrap_or(0));

        let mut condition = Condition::new(id, condition_type);
        condition.raw_type = raw_type;

        if let Some(ver_attr) = node.get_attribute("Version") {
            condition.version = ver_attr.value_string().parse().unwrap_or(0);
        }

        if let Some(invert_attr) = node.get_attribute("Invert") {
            condition.invert = invert_attr.value_string().eq_ignore_ascii_case("true");
        }

        if let Some(async_attr) = node.get_attribute("Async") {
            condition.is_async = async_attr.value_string().eq_ignore_ascii_case("true");
        }

        // Parse Var children - these reference script variables
        Self::parse_var_refs(node, &mut condition.inputs, &mut condition.outputs);

        Ok(condition)
    }

    /// Parse an Effect node.
    fn parse_effect(node: &pipeline::xmb::Node) -> LoadResult<Effect> {
        let dbid = get_attr_i32(node, "DBID")?;
        let raw_type = u16::try_from(dbid).map_err(|_| LoadError::UnknownEffectType(dbid))?;
        let effect_type = EffectType::from_u16(raw_type).unwrap_or(EffectType::Custom);

        let id = node
            .get_attribute("ID")
            .map_or(0, |a| a.value_string().parse().unwrap_or(0));

        let mut effect = Effect::new(id, effect_type);
        effect.raw_type = raw_type;

        if let Some(ver_attr) = node.get_attribute("Version") {
            effect.version = ver_attr.value_string().parse().unwrap_or(0);
        }

        // Parse Var children
        Self::parse_var_refs(node, &mut effect.inputs, &mut effect.outputs);

        Ok(effect)
    }

    /// Parse authored signature bindings for inputs and outputs.
    fn parse_var_refs(
        node: &pipeline::xmb::Node,
        inputs: &mut Vec<VarBinding>,
        outputs: &mut Vec<VarBinding>,
    ) {
        for child in &node.children {
            if !matches!(child.name.as_str(), "Input" | "Output" | "Var") {
                continue;
            }
            let Ok(var_id) = child.text_string().parse::<VarId>() else {
                continue;
            };
            let Some(signature_id) = child
                .get_attribute("SigID")
                .and_then(|attribute| attribute.value_string().parse::<u16>().ok())
            else {
                continue;
            };
            let is_output = child.name == "Output"
                || child
                    .get_attribute("Output")
                    .is_some_and(|attribute| attribute.value_string().eq_ignore_ascii_case("true"));
            let binding = VarBinding::new(signature_id, var_id);
            if is_output {
                outputs.push(binding);
            } else {
                inputs.push(binding);
            }
        }
        inputs.sort_by_key(|binding| binding.signature_id);
        outputs.sort_by_key(|binding| binding.signature_id);
    }
}

fn remap_trigger_editor_ids(script: &mut TriggerScript) {
    let editor_to_runtime = script
        .triggers
        .iter()
        .map(|trigger| (trigger.editor_id, trigger.id))
        .collect::<HashMap<_, _>>();
    for variable in script.variables.values_mut() {
        if let TriggerValue::Trigger(editor_id) = &mut variable.value
            && let Some(runtime_id) = editor_to_runtime.get(editor_id)
        {
            *editor_id = *runtime_id;
        }
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Get a required string attribute.
fn get_attr_string(node: &pipeline::xmb::Node, name: &str) -> LoadResult<String> {
    node.get_attribute(name)
        .map(pipeline::xmb::Attribute::value_string)
        .ok_or_else(|| LoadError::MissingAttribute(name.into()))
}

/// Get a required u32 attribute.
fn get_attr_u32(node: &pipeline::xmb::Node, name: &str) -> LoadResult<u32> {
    let s = get_attr_string(node, name)?;
    s.parse().map_err(|_| LoadError::InvalidValue {
        field: name.into(),
        value: s,
    })
}

/// Get a required i32 attribute.
fn get_attr_i32(node: &pipeline::xmb::Node, name: &str) -> LoadResult<i32> {
    let s = get_attr_string(node, name)?;
    s.parse().map_err(|_| LoadError::InvalidValue {
        field: name.into(),
        value: s,
    })
}

/// Parse script type from string.
fn parse_script_type(s: &str) -> super::script::ScriptType {
    use super::script::ScriptType;
    match s.to_lowercase().as_str() {
        "scenario" => ScriptType::Scenario,
        "power" => ScriptType::Power,
        "ability" => ScriptType::Ability,
        "triggerscript" => ScriptType::TriggerScript,
        _ => ScriptType::Invalid,
    }
}

/// Parse variable type from string.
fn parse_var_type(s: &str) -> Option<VarType> {
    // Match vanilla type strings to VarType enum
    Some(match s {
        "Tech" => VarType::Tech,
        "TechStatus" => VarType::TechStatus,
        "Operator" => VarType::Operator,
        "ProtoObject" => VarType::ProtoObject,
        "ObjectType" => VarType::ObjectType,
        "ProtoSquad" => VarType::ProtoSquad,
        "Sound" => VarType::Sound,
        "Entity" => VarType::Entity,
        "EntityList" => VarType::EntityList,
        "Trigger" => VarType::Trigger,
        "Time" => VarType::Time,
        "Player" => VarType::Player,
        "UILocation" => VarType::UILocation,
        "UIEntity" => VarType::UIEntity,
        "Cost" => VarType::Cost,
        "AnimType" => VarType::AnimType,
        "ActionStatus" => VarType::ActionStatus,
        "Power" => VarType::Power,
        "Bool" => VarType::Bool,
        "Float" => VarType::Float,
        "Iterator" => VarType::Iterator,
        "Team" => VarType::Team,
        "PlayerList" => VarType::PlayerList,
        "TeamList" => VarType::TeamList,
        "PlayerState" => VarType::PlayerState,
        "Objective" => VarType::Objective,
        "Unit" => VarType::Unit,
        "UnitList" => VarType::UnitList,
        "Squad" => VarType::Squad,
        "SquadList" => VarType::SquadList,
        "UIUnit" => VarType::UIUnit,
        "UISquad" => VarType::UISquad,
        "UISquadList" => VarType::UISquadList,
        "String" => VarType::String,
        "MessageIndex" => VarType::MessageIndex,
        "MessageJustify" => VarType::MessageJustify,
        "MessagePoint" => VarType::MessagePoint,
        "Color" => VarType::Color,
        "ProtoObjectList" => VarType::ProtoObjectList,
        "ObjectTypeList" => VarType::ObjectTypeList,
        "ProtoSquadList" => VarType::ProtoSquadList,
        "TechList" => VarType::TechList,
        "MathOperator" => VarType::MathOperator,
        "ObjectDataType" => VarType::ObjectDataType,
        "ObjectDataRelative" => VarType::ObjectDataRelative,
        "Civ" => VarType::Civ,
        "ProtoObjectCollection" => VarType::ProtoObjectCollection,
        "Object" => VarType::Object,
        "ObjectList" => VarType::ObjectList,
        "Group" => VarType::Group,
        "LocationList" => VarType::VectorList,
        "RefCountType" => VarType::RefCountType,
        "UnitFlag" => VarType::UnitFlag,
        "LOSType" => VarType::LOSType,
        "EntityFilterSet" => VarType::EntityFilterSet,
        "PopBucket" => VarType::PopBucket,
        "ListPosition" => VarType::ListPosition,
        "RelationType" | "Diplomacy" => VarType::RelationType,
        "ExposedAction" => VarType::ExposedAction,
        "SquadMode" => VarType::SquadMode,
        "ExposedScript" => VarType::ExposedScript,
        _ => return parse_extended_var_type(s),
    })
}

fn parse_extended_var_type(s: &str) -> Option<VarType> {
    Some(match s {
        "KBBase" => VarType::KBBase,
        "KBBaseList" => VarType::KBBaseList,
        "DataScalar" => VarType::DataScalar,
        "KBBaseQuery" => VarType::KBBaseQuery,
        "DesignLine" => VarType::DesignLine,
        "LocStringID" => VarType::LocStringID,
        "Leader" => VarType::Leader,
        "Cinematic" => VarType::Cinematic,
        "TalkingHead" => VarType::TalkingHead,
        "FlareType" => VarType::FlareType,
        "CinematicTag" => VarType::CinematicTag,
        "IconType" => VarType::IconType,
        "Difficulty" => VarType::Difficulty,
        "Integer" | "Count" => VarType::Integer,
        "HUDItem" => VarType::HUDItem,
        "FlashableUIItem" => VarType::FlashableUIItem,
        "ControlType" => VarType::ControlType,
        "UIButton" => VarType::UIButton,
        "MissionType" => VarType::MissionType,
        "MissionState" => VarType::MissionState,
        "MissionTargetType" => VarType::MissionTargetType,
        "IntegerList" => VarType::IntegerList,
        "BidType" => VarType::BidType,
        "BidState" => VarType::BidState,
        "BuildingCommandState" => VarType::BuildingCommandState,
        "Vector" | "Direction" => VarType::Vector,
        "VectorList" => VarType::VectorList,
        "PlacementRule" => VarType::PlacementRule,
        "KBSquad" => VarType::KBSquad,
        "KBSquadList" => VarType::KBSquadList,
        "KBSquadQuery" => VarType::KBSquadQuery,
        "AISquadAnalysis" => VarType::AISquadAnalysis,
        "AISquadAnalysisComponent" => VarType::AISquadAnalysisComponent,
        "KBSquadFilterSet" => VarType::KBSquadFilterSet,
        "ChatSpeaker" => VarType::ChatSpeaker,
        "RumbleType" => VarType::RumbleType,
        "RumbleMotor" => VarType::RumbleMotor,
        "TechDataCommandType" | "CommandType" => VarType::TechDataCommandType,
        "SquadDataType" => VarType::SquadDataType,
        "EventType" => VarType::EventType,
        "TimeList" => VarType::TimeList,
        "DesignLineList" => VarType::DesignLineList,
        "GameStatePredicate" => VarType::GameStatePredicate,
        "FloatList" => VarType::FloatList,
        "UILocationMinigame" => VarType::UILocationMinigame,
        "SquadFlag" => VarType::SquadFlag,
        "Concept" => VarType::Concept,
        "ConceptList" => VarType::ConceptList,
        "UserClassType" => VarType::UserClassType,
        "Location" => VarType::UILocation,
        "Distance" | "Percent" | "Hitpoints" => VarType::Float,
        _ => return None,
    })
}

/// Parse a value from text based on variable type.
fn parse_var_value(text: &str, var_type: VarType, context: TriggerLoadContext<'_>) -> TriggerValue {
    match var_type {
        VarType::Bool => TriggerValue::Bool(text.eq_ignore_ascii_case("true")),
        VarType::Integer => TriggerValue::Int(text.parse().unwrap_or(0)),
        VarType::IntegerList => TriggerValue::IntegerList(parse_i32_list(text)),
        VarType::Float => TriggerValue::Float(text.parse().unwrap_or(0.0)),
        VarType::Time => TriggerValue::Time(text.parse().unwrap_or(0)),
        VarType::Operator => TriggerValue::Int(parse_compare_operator(text)),
        VarType::MathOperator => TriggerValue::Int(parse_math_operator(text)),
        VarType::ListPosition => TriggerValue::Int(value_types::parse_list_position(text)),
        VarType::UILocation => parse_vec3_value(text, TriggerValue::Location),
        VarType::Vector => parse_vec3_value(text, TriggerValue::Vector),
        VarType::VectorList => TriggerValue::VectorList(value_types::parse_vector_list(text)),
        VarType::Color => parse_color_value(text),
        VarType::Cost => parse_cost_value(text),
        // Entity references
        VarType::Unit | VarType::UIUnit => {
            TriggerValue::Unit(parse_entity_reference(text, context))
        }
        VarType::Squad | VarType::UISquad => {
            TriggerValue::Squad(parse_entity_reference(text, context))
        }
        VarType::Object => TriggerValue::Object(parse_entity_reference(text, context)),
        VarType::Entity | VarType::UIEntity => {
            TriggerValue::Entity(parse_entity_reference(text, context))
        }
        VarType::UnitList => TriggerValue::UnitList(parse_entity_list(text, context)),
        VarType::SquadList | VarType::UISquadList => {
            TriggerValue::SquadList(parse_entity_list(text, context))
        }
        VarType::ObjectList => TriggerValue::ObjectList(parse_entity_list(text, context)),
        VarType::EntityList => TriggerValue::EntityList(parse_entity_list(text, context)),
        // Proto types
        VarType::ProtoObject => TriggerValue::ProtoObject(parse_proto_object(text, context)),
        VarType::ProtoSquad => TriggerValue::ProtoSquad(parse_proto_squad(text, context)),
        VarType::ProtoObjectList => TriggerValue::ProtoObjectList(parse_prototype_list(
            text,
            |token| parse_proto_object(token, context),
            false,
        )),
        VarType::ProtoSquadList => TriggerValue::ProtoSquadList(parse_prototype_list(
            text,
            |token| parse_proto_squad(token, context),
            false,
        )),
        VarType::ObjectType => TriggerValue::ObjectType(text.trim().to_owned()),
        VarType::ObjectTypeList => TriggerValue::ObjectTypeList(parse_name_list(text)),
        VarType::DesignLine => TriggerValue::DesignLine(text.trim().parse().unwrap_or(-1)),
        VarType::DesignLineList => TriggerValue::DesignLineList(parse_i32_list(text)),
        VarType::Tech => TriggerValue::Tech(parse_technology(text, context)),
        VarType::TechList => TriggerValue::TechList(parse_prototype_list(
            text,
            |token| parse_technology(token, context),
            true,
        )),
        // Player/team references
        VarType::Player => TriggerValue::Player(text.parse().unwrap_or(0)),
        VarType::PlayerList => TriggerValue::PlayerList(parse_i32_list(text)),
        VarType::Team => TriggerValue::Team(text.parse().unwrap_or(0)),
        VarType::TeamList => TriggerValue::TeamList(parse_i32_list(text)),
        VarType::PlayerState => TriggerValue::Int(parse_player_state(text, context)),
        VarType::Civ => TriggerValue::Int(parse_civilization(text, context)),
        VarType::Leader => TriggerValue::Int(parse_leader(text, context)),
        VarType::TechStatus => TriggerValue::Int(parse_tech_status(text)),
        VarType::RelationType => TriggerValue::Int(parse_relation_type(text)),
        VarType::SquadMode => TriggerValue::Int(parse_squad_mode(text)),
        VarType::Difficulty => TriggerValue::Int(parse_difficulty(text)),
        VarType::TechDataCommandType => TriggerValue::Int(parse_command_type(text)),
        VarType::EventType => TriggerValue::Int(parse_general_event_type(text)),
        VarType::LocStringID
        | VarType::Cinematic
        | VarType::CinematicTag
        | VarType::TalkingHead => TriggerValue::Int(text.trim().parse().unwrap_or(-1)),
        // Trigger references
        VarType::Trigger => TriggerValue::Trigger(text.parse().unwrap_or(0)),
        VarType::Objective => TriggerValue::Objective(text.parse().unwrap_or(0)),
        VarType::PopBucket => TriggerValue::Int(parse_population_bucket(text, context)),
        VarType::Iterator => TriggerValue::Iterator(super::TriggerIterator::default()),
        VarType::EntityFilterSet => {
            TriggerValue::EntityFilterSet(super::EntityFilterSet::default())
        }
        VarType::BuildingCommandState => {
            TriggerValue::BuildingCommandState(super::BuildingCommandState::default())
        }
        VarType::AISquadAnalysis => {
            TriggerValue::AISquadAnalysis(super::AISquadAnalysis::default())
        }
        VarType::AISquadAnalysisComponent => TriggerValue::AISquadAnalysisComponent(
            super::AISquadAnalysisComponent::from_name(text).unwrap_or_default(),
        ),
        // Default to storing as string for unhandled types
        _ => TriggerValue::String(text.to_string()),
    }
}

fn parse_entity_reference(text: &str, context: TriggerLoadContext<'_>) -> EntityId {
    let Ok(raw_id) = text.trim().parse::<u32>() else {
        return EntityId::INVALID;
    };
    if let Some(entities) = context.scenario_entities {
        return i32::try_from(raw_id)
            .ok()
            .and_then(|scenario_id| entities.get(&scenario_id).copied())
            .unwrap_or(EntityId::INVALID);
    }
    EntityId::from_u32(raw_id)
}

fn parse_entity_list(text: &str, context: TriggerLoadContext<'_>) -> Vec<EntityId> {
    let mut entities = text
        .split(',')
        .map(|token| parse_entity_reference(token, context))
        .filter(|entity_id| !entity_id.is_invalid())
        .collect::<Vec<_>>();
    entities.sort_unstable();
    entities.dedup();
    entities
}

fn parse_proto_object(text: &str, context: TriggerLoadContext<'_>) -> i32 {
    text.parse()
        .ok()
        .or_else(|| {
            context
                .database
                .and_then(|database| crate::spawn::object_prototype_id(database, text))
        })
        .unwrap_or(-1)
}

fn parse_proto_squad(text: &str, context: TriggerLoadContext<'_>) -> i32 {
    text.parse()
        .ok()
        .or_else(|| {
            context
                .database
                .and_then(|database| crate::spawn::squad_prototype_id(database, text))
        })
        .unwrap_or(-1)
}

fn parse_technology(text: &str, context: TriggerLoadContext<'_>) -> i32 {
    text.parse()
        .ok()
        .or_else(|| {
            context
                .database
                .and_then(|database| crate::world::technology_prototype_id(database, text))
        })
        .unwrap_or(-1)
}

fn parse_prototype_list(
    text: &str,
    mut resolve: impl FnMut(&str) -> i32,
    unique: bool,
) -> Vec<i32> {
    let mut values = Vec::new();
    for token in text
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let value = resolve(token);
        if !unique || !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

fn parse_population_bucket(text: &str, context: TriggerLoadContext<'_>) -> i32 {
    text.parse()
        .ok()
        .or_else(|| {
            context
                .database?
                .game_data
                .as_ref()?
                .pops
                .as_ref()?
                .entries
                .iter()
                .position(|name| name.eq_ignore_ascii_case(text))
                .and_then(|index| i32::try_from(index).ok())
        })
        .unwrap_or(-1)
}

fn parse_player_state(text: &str, context: TriggerLoadContext<'_>) -> i32 {
    parse_database_index(text, || {
        context
            .database?
            .game_data
            .as_ref()?
            .player_states
            .as_ref()?
            .entries
            .iter()
            .position(|name| name.eq_ignore_ascii_case(text.trim()))
    })
}

fn parse_civilization(text: &str, context: TriggerLoadContext<'_>) -> i32 {
    parse_database_index(text, || {
        context
            .database?
            .civs
            .iter()
            .position(|civilization| civilization.name.eq_ignore_ascii_case(text.trim()))
    })
}

fn parse_leader(text: &str, context: TriggerLoadContext<'_>) -> i32 {
    parse_database_index(text, || {
        context
            .database?
            .leaders
            .iter()
            .position(|leader| leader.name.eq_ignore_ascii_case(text.trim()))
    })
}

fn parse_database_index(text: &str, find: impl FnOnce() -> Option<usize>) -> i32 {
    text.trim()
        .parse()
        .ok()
        .or_else(|| find().and_then(|index| i32::try_from(index).ok()))
        .unwrap_or(-1)
}

fn parse_relation_type(text: &str) -> i32 {
    text.trim().parse().unwrap_or_else(|_| {
        if text.trim().eq_ignore_ascii_case("Self") {
            1
        } else if text.trim().eq_ignore_ascii_case("Ally") {
            2
        } else if text.trim().eq_ignore_ascii_case("Enemy") {
            3
        } else if text.trim().eq_ignore_ascii_case("Neutral") {
            4
        } else {
            0
        }
    })
}

fn parse_tech_status(text: &str) -> i32 {
    text.trim().parse().unwrap_or_else(|_| {
        [
            "Unobtainable",
            "Obtainable",
            "Available",
            "Researching",
            "Active",
            "Disabled",
            "CoopResearching",
        ]
        .iter()
        .position(|name| name.eq_ignore_ascii_case(text.trim()))
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(0)
    })
}

fn parse_squad_mode(text: &str) -> i32 {
    text.trim().parse().unwrap_or_else(|_| {
        (0..=12)
            .find(|value| {
                crate::entities::SquadMode::from_i32(*value)
                    .is_some_and(|mode| mode.as_str().eq_ignore_ascii_case(text.trim()))
            })
            .unwrap_or(-1)
    })
}

fn parse_difficulty(text: &str) -> i32 {
    text.trim().parse().unwrap_or_else(|_| {
        ["Easy", "Normal", "Hard", "Legendary", "Custom", "Automatic"]
            .iter()
            .position(|name| name.eq_ignore_ascii_case(text.trim()))
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    })
}

fn parse_command_type(text: &str) -> i32 {
    [
        "Research",
        "TrainUnit",
        "Build",
        "TrainSquad",
        "Unload",
        "Reinforce",
        "ChangeMode",
        "Ability",
        "Kill",
        "CancelKill",
        "Tribute",
        "CustomCommand",
        "Power",
        "BuildOther",
        "TrainLock",
        "TrainUnlock",
        "RallyPoint",
        "ClearRallyPoint",
        "DestroyBase",
        "CancelDestroyBase",
        "ReverseHotDrop",
    ]
    .iter()
    .position(|name| name.eq_ignore_ascii_case(text.trim()))
    .and_then(|index| i32::try_from(index).ok())
    .unwrap_or(-1)
}

fn parse_general_event_type(text: &str) -> i32 {
    text.trim().parse().unwrap_or_else(|_| {
        crate::world::GeneralEventType::from_name(text.trim())
            .map_or(-1, |event_type| i32::from(event_type as u16))
    })
}

fn parse_i32_list(text: &str) -> Vec<i32> {
    text.split(',')
        .filter_map(|token| token.trim().parse().ok())
        .collect()
}

fn parse_name_list(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for value in text
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if values
            .iter()
            .any(|candidate: &String| candidate.eq_ignore_ascii_case(value))
        {
            continue;
        }
        values.push(value.to_owned());
    }
    values
}

fn parse_vec3_value(text: &str, wrap: fn(Vec3) -> TriggerValue) -> TriggerValue {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() < 3 {
        return wrap(Vec3::default());
    }

    wrap(Vec3::new(
        parts[0].trim().parse().unwrap_or(0.0),
        parts[1].trim().parse().unwrap_or(0.0),
        parts[2].trim().parse().unwrap_or(0.0),
    ))
}

fn parse_color_value(text: &str) -> TriggerValue {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() < 3 {
        return TriggerValue::Color(Color::default());
    }

    let channel = |index: usize, default: f32| {
        parts
            .get(index)
            .and_then(|part| part.trim().parse::<f32>().ok())
            .unwrap_or(default)
    };
    TriggerValue::Color(Color::new(
        normalized_to_u8(channel(0, 0.0)),
        normalized_to_u8(channel(1, 0.0)),
        normalized_to_u8(channel(2, 0.0)),
        normalized_to_u8(channel(3, 1.0)),
    ))
}

fn normalized_to_u8(value: f32) -> u8 {
    if value.is_nan() {
        return 0;
    }

    (value.clamp(0.0, 1.0) * f32::from(u8::MAX))
        .trunc()
        .to_u8()
        .expect("a normalized color channel must fit in u8")
}

fn parse_cost_value(text: &str) -> TriggerValue {
    if text.contains('=') {
        let mut cost = Cost::default();
        for token in text.split(',') {
            let Some((resource, amount)) = token.split_once('=') else {
                continue;
            };
            let Ok(amount) = amount.trim().parse::<f32>() else {
                continue;
            };
            if let Ok(resource_id) = resource.trim().parse::<usize>() {
                cost.set(resource_id, amount);
            }
        }
        return TriggerValue::Cost(cost);
    }
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() < 3 {
        return TriggerValue::Cost(Cost::default());
    }

    let mut amounts = [0.0; super::value::COST_RESOURCE_SLOTS];
    for (amount, part) in amounts.iter_mut().zip(parts) {
        *amount = part.trim().parse().unwrap_or(0.0);
    }
    TriggerValue::Cost(Cost::from_amounts(amounts))
}

fn parse_compare_operator(text: &str) -> i32 {
    text.parse().unwrap_or_else(|_| match text.trim() {
        "NotEqualTo" => 0,
        "LessThan" => 1,
        "LessThanOrEqualTo" => 2,
        "EqualTo" => 3,
        "GreaterThanOrEqualTo" => 4,
        "GreaterThan" => 5,
        _ => -1,
    })
}

fn parse_math_operator(text: &str) -> i32 {
    text.parse().unwrap_or_else(|_| match text.trim() {
        "Add" => 0,
        "Subtract" => 1,
        "Multiply" => 2,
        "Divide" => 3,
        "Modulus" => 4,
        _ => -1,
    })
}

#[cfg(test)]
mod tests;
