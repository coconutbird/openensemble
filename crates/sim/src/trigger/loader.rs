//! VanillaLoader - parses .triggerscript XMB files into core trigger types.
//!
//! This module handles loading vanilla Halo Wars trigger scripts from their
//! binary XMB format into the internal representation used by the trigger engine.

use pipeline::xmb::{Document as XmbDocument, Reader as XmbReader};

use super::{
    Condition, ConditionType, Effect, EffectType, Trigger, TriggerScript, TriggerValue, TriggerVar,
    VarId, VarType,
};

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

/// Loads vanilla .triggerscript XMB files into TriggerScript instances.
pub struct VanillaLoader;

impl VanillaLoader {
    /// Load a trigger script from raw bytes.
    pub fn load_bytes(data: &[u8]) -> LoadResult<TriggerScript> {
        let xmb = XmbReader::read(data)?;
        Self::from_xmb(&xmb)
    }

    /// Convert parsed XMB data into a TriggerScript.
    pub fn from_xmb(xmb: &XmbDocument) -> LoadResult<TriggerScript> {
        let root = xmb
            .root()
            .ok_or_else(|| LoadError::MissingElement("TriggerScript root".into()))?;

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
                    let var = Self::parse_trigger_var(var_node)?;
                    script.add_variable(var);
                }
            }
        }

        // Parse Triggers
        if let Some(triggers_node) = root.children.iter().find(|n| n.name == "Triggers") {
            for trigger_node in &triggers_node.children {
                if trigger_node.name == "Trigger" {
                    let trigger = Self::parse_trigger(trigger_node)?;
                    script.add_trigger(trigger);
                }
            }
        }

        Ok(script)
    }

    /// Parse a TriggerVar node.
    fn parse_trigger_var(node: &pipeline::xmb::Node) -> LoadResult<TriggerVar> {
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
            // Leave var.value as default
            return Ok(var);
        }

        // Parse the value from node text based on type
        let text = node.text_string();
        if !text.is_empty() {
            var.value = parse_var_value(&text, var_type);
        }

        Ok(var)
    }

    /// Parse a Trigger node.
    fn parse_trigger(node: &pipeline::xmb::Node) -> LoadResult<Trigger> {
        let id = get_attr_u32(node, "ID")?;
        let mut trigger = Trigger::new(id);

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

    /// Parse TriggerConditions node (contains And or Or wrapper).
    fn parse_conditions(node: &pipeline::xmb::Node, trigger: &mut Trigger) -> LoadResult<()> {
        // Find the And or Or wrapper node
        for child in &node.children {
            if child.name == "Or" {
                trigger.or_conditions = true;
                for cond_node in &child.children {
                    if cond_node.name == "Condition" {
                        let condition = Self::parse_condition(cond_node)?;
                        trigger.conditions.push(condition);
                    }
                }
                break;
            } else if child.name == "And" {
                trigger.or_conditions = false;
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
        let condition_type =
            ConditionType::from_u16(dbid as u16).ok_or(LoadError::UnknownConditionType(dbid))?;

        let id = node
            .get_attribute("ID")
            .map(|a| a.value_string().parse().unwrap_or(0))
            .unwrap_or(0);

        let mut condition = Condition::new(id, condition_type);

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
        Self::parse_var_refs(node, &mut condition.inputs, &mut condition.outputs)?;

        Ok(condition)
    }

    /// Parse an Effect node.
    fn parse_effect(node: &pipeline::xmb::Node) -> LoadResult<Effect> {
        let dbid = get_attr_i32(node, "DBID")?;
        let effect_type =
            EffectType::from_u16(dbid as u16).ok_or(LoadError::UnknownEffectType(dbid))?;

        let id = node
            .get_attribute("ID")
            .map(|a| a.value_string().parse().unwrap_or(0))
            .unwrap_or(0);

        let mut effect = Effect::new(id, effect_type);

        if let Some(ver_attr) = node.get_attribute("Version") {
            effect.version = ver_attr.value_string().parse().unwrap_or(0);
        }

        // Parse Var children
        Self::parse_var_refs(node, &mut effect.inputs, &mut effect.outputs)?;

        Ok(effect)
    }

    /// Parse Var children to extract input/output variable references.
    /// Vars with SigID are inputs, vars with Output="true" are outputs.
    fn parse_var_refs(
        node: &pipeline::xmb::Node,
        inputs: &mut Vec<VarId>,
        outputs: &mut Vec<VarId>,
    ) -> LoadResult<()> {
        for child in &node.children {
            if child.name == "Var" {
                let var_id: VarId = child.text_string().parse().unwrap_or(0);

                // Check if this is an output variable
                let is_output = child
                    .get_attribute("Output")
                    .map(|a| a.value_string().eq_ignore_ascii_case("true"))
                    .unwrap_or(false);

                if is_output {
                    outputs.push(var_id);
                } else {
                    inputs.push(var_id);
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Get a required string attribute.
fn get_attr_string(node: &pipeline::xmb::Node, name: &str) -> LoadResult<String> {
    node.get_attribute(name)
        .map(|a| a.value_string())
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
        "RefCountType" => VarType::RefCountType,
        "UnitFlag" => VarType::UnitFlag,
        "LOSType" => VarType::LOSType,
        "EntityFilterSet" => VarType::EntityFilterSet,
        "PopBucket" => VarType::PopBucket,
        "ListPosition" => VarType::ListPosition,
        "RelationType" => VarType::RelationType,
        "ExposedAction" => VarType::ExposedAction,
        "SquadMode" => VarType::SquadMode,
        "ExposedScript" => VarType::ExposedScript,
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
        "Integer" => VarType::Integer,
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
        "Vector" => VarType::Vector,
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
        "TechDataCommandType" => VarType::TechDataCommandType,
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
        // Location is stored as UILocation in vanilla
        "Location" => VarType::UILocation,
        // Numeric types - stored as Float or Integer
        "Count" => VarType::Integer,
        "Distance" => VarType::Float,
        "Percent" => VarType::Float,
        "Hitpoints" => VarType::Float,
        _ => return None,
    })
}

/// Parse a value from text based on variable type.
fn parse_var_value(text: &str, var_type: VarType) -> TriggerValue {
    use super::value::{Color, Cost, Vec3};
    use crate::EntityId;

    match var_type {
        VarType::Bool => TriggerValue::Bool(text.eq_ignore_ascii_case("true")),
        VarType::Integer => TriggerValue::Int(text.parse().unwrap_or(0)),
        VarType::Float => TriggerValue::Float(text.parse().unwrap_or(0.0)),
        VarType::Time => TriggerValue::Time(text.parse().unwrap_or(0)),
        VarType::String | VarType::LocStringID => TriggerValue::String(text.to_string()),
        VarType::UILocation => {
            // Format: "x,y,z"
            let parts: Vec<&str> = text.split(',').collect();
            if parts.len() >= 3 {
                let x = parts[0].trim().parse().unwrap_or(0.0);
                let y = parts[1].trim().parse().unwrap_or(0.0);
                let z = parts[2].trim().parse().unwrap_or(0.0);
                TriggerValue::Location(Vec3::new(x, y, z))
            } else {
                TriggerValue::default()
            }
        }
        VarType::Vector => {
            let parts: Vec<&str> = text.split(',').collect();
            if parts.len() >= 3 {
                let x = parts[0].trim().parse().unwrap_or(0.0);
                let y = parts[1].trim().parse().unwrap_or(0.0);
                let z = parts[2].trim().parse().unwrap_or(0.0);
                TriggerValue::Vector(Vec3::new(x, y, z))
            } else {
                TriggerValue::default()
            }
        }
        VarType::Color => {
            // Format: "r,g,b,a" or "r,g,b"
            let parts: Vec<&str> = text.split(',').collect();
            if parts.len() >= 3 {
                let r: f32 = parts[0].trim().parse().unwrap_or(0.0);
                let g: f32 = parts[1].trim().parse().unwrap_or(0.0);
                let b: f32 = parts[2].trim().parse().unwrap_or(0.0);
                let a: f32 = parts
                    .get(3)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1.0);
                // Convert from 0-1 float to 0-255 u8
                TriggerValue::Color(Color::new(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                    (a * 255.0) as u8,
                ))
            } else {
                TriggerValue::default()
            }
        }
        VarType::Cost => {
            // Format: "supplies,power,population"
            let parts: Vec<&str> = text.split(',').collect();
            if parts.len() >= 3 {
                TriggerValue::Cost(Cost {
                    supplies: parts[0].trim().parse().unwrap_or(0.0),
                    power: parts[1].trim().parse().unwrap_or(0.0),
                    population: parts[2].trim().parse().unwrap_or(0.0),
                })
            } else {
                TriggerValue::default()
            }
        }
        // Entity references
        VarType::Unit => TriggerValue::Unit(EntityId::from_u32(text.parse().unwrap_or(0))),
        VarType::Squad => TriggerValue::Squad(EntityId::from_u32(text.parse().unwrap_or(0))),
        VarType::Object => TriggerValue::Object(EntityId::from_u32(text.parse().unwrap_or(0))),
        VarType::Entity => TriggerValue::Entity(EntityId::from_u32(text.parse().unwrap_or(0))),
        // Proto types
        VarType::ProtoObject => TriggerValue::ProtoObject(text.parse().unwrap_or(0)),
        VarType::ProtoSquad => TriggerValue::ProtoSquad(text.parse().unwrap_or(0)),
        VarType::ObjectType => TriggerValue::ObjectType(text.parse().unwrap_or(0)),
        VarType::Tech => TriggerValue::Tech(text.parse().unwrap_or(0)),
        // Player/team references
        VarType::Player => TriggerValue::Player(text.parse().unwrap_or(0)),
        VarType::Team => TriggerValue::Team(text.parse().unwrap_or(0)),
        // Trigger references
        VarType::Trigger => TriggerValue::Trigger(text.parse().unwrap_or(0)),
        VarType::Objective => TriggerValue::Objective(text.parse().unwrap_or(0)),
        // Default to storing as string for unhandled types
        _ => TriggerValue::String(text.to_string()),
    }
}
