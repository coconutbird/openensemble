//! Trigger variable types - matches vanilla enum for compatibility.

/// Variable types supported by the trigger system.
/// Values match vanilla `BTriggerVar::cVarType*` for file format compatibility.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    zerocopy::TryFromBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(u8)]
pub enum VarType {
    #[default]
    /// No type. Marks an unset or unknown variable.
    Invalid = 0,
    /// A technology ID.
    Tech = 1,
    /// A technology research status.
    TechStatus = 2,
    /// A comparison operator.
    Operator = 3,
    /// A prototype object ID.
    ProtoObject = 4,
    /// An object type ID.
    ObjectType = 5,
    /// A prototype squad ID.
    ProtoSquad = 6,
    /// A sound cue name.
    Sound = 7,
    /// One game entity.
    Entity = 8,
    /// A list of game entities.
    EntityList = 9,
    /// A trigger ID.
    Trigger = 10,
    /// A time value.
    Time = 11,
    /// A player ID.
    Player = 12,
    /// A map location that a player picks in the UI.
    UILocation = 13,
    /// An entity that a player picks in the UI.
    UIEntity = 14,
    /// A resource cost.
    Cost = 15,
    /// An animation type.
    AnimType = 16,
    /// An action status.
    ActionStatus = 17,
    /// A power ID.
    Power = 18,
    /// A boolean value.
    Bool = 19,
    /// A floating-point value.
    Float = 20,
    /// An iterator over a list variable.
    Iterator = 21,
    /// A team ID.
    Team = 22,
    /// A list of player IDs.
    PlayerList = 23,
    /// A list of team IDs.
    TeamList = 24,
    /// A player state.
    PlayerState = 25,
    /// An objective ID.
    Objective = 26,
    /// One unit.
    Unit = 27,
    /// A list of units.
    UnitList = 28,
    /// One squad.
    Squad = 29,
    /// A list of squads.
    SquadList = 30,
    /// A unit that a player picks in the UI.
    UIUnit = 31,
    /// A squad that a player picks in the UI.
    UISquad = 32,
    /// A list of squads that a player picks in the UI.
    UISquadList = 33,
    /// A text string.
    String = 34,
    /// A message index.
    MessageIndex = 35,
    /// A message text alignment.
    MessageJustify = 36,
    /// A message screen position.
    MessagePoint = 37,
    /// A color value.
    Color = 38,
    /// A list of prototype object IDs.
    ProtoObjectList = 39,
    /// A list of object type IDs.
    ObjectTypeList = 40,
    /// A list of prototype squad IDs.
    ProtoSquadList = 41,
    /// A list of technology IDs.
    TechList = 42,
    /// An arithmetic operator.
    MathOperator = 43,
    /// An object data field selector.
    ObjectDataType = 44,
    /// A relative or absolute mode for object data changes.
    ObjectDataRelative = 45,
    /// A civilization ID.
    Civ = 46,
    /// A collection of prototype objects.
    ProtoObjectCollection = 47,
    /// One object.
    Object = 48,
    /// A list of objects.
    ObjectList = 49,
    /// A trigger group ID.
    Group = 50,
    /// A reference count type.
    RefCountType = 51,
    /// A unit flag.
    UnitFlag = 52,
    /// A line-of-sight type.
    LOSType = 53,
    /// A set of entity filters.
    EntityFilterSet = 54,
    /// A population bucket.
    PopBucket = 55,
    /// A position in a list.
    ListPosition = 56,
    /// A diplomatic relation type.
    RelationType = 57,
    /// An exposed action.
    ExposedAction = 58,
    /// A squad mode.
    SquadMode = 59,
    /// An exposed script.
    ExposedScript = 60,
    /// One knowledge-base base.
    KBBase = 61,
    /// A list of knowledge-base bases.
    KBBaseList = 62,
    /// A data scalar.
    DataScalar = 63,
    /// A knowledge-base base query.
    KBBaseQuery = 64,
    /// A design line.
    DesignLine = 65,
    /// A localized string ID.
    LocStringID = 66,
    /// A leader ID.
    Leader = 67,
    /// A cinematic ID.
    Cinematic = 68,
    /// A flare type.
    FlareType = 69,
    /// A cinematic tag.
    CinematicTag = 70,
    /// An icon type.
    IconType = 71,
    /// A difficulty level.
    Difficulty = 72,
    /// An integer value.
    Integer = 73,
    /// A HUD item.
    HUDItem = 74,
    /// A controller input type.
    ControlType = 75,
    /// A UI button.
    UIButton = 76,
    /// An AI mission type.
    MissionType = 77,
    /// An AI mission state.
    MissionState = 78,
    /// An AI mission target type.
    MissionTargetType = 79,
    /// A list of integer values.
    IntegerList = 80,
    /// An AI bid type.
    BidType = 81,
    /// An AI bid state.
    BidState = 82,
    /// A building command state.
    BuildingCommandState = 83,
    /// A 3D vector or location.
    Vector = 84,
    /// A list of 3D vectors or locations.
    VectorList = 85,
    /// A placement rule.
    PlacementRule = 86,
    /// One knowledge-base squad.
    KBSquad = 87,
    /// A list of knowledge-base squads.
    KBSquadList = 88,
    /// A knowledge-base squad query.
    KBSquadQuery = 89,
    /// An AI squad analysis.
    AISquadAnalysis = 90,
    /// An AI squad analysis component.
    AISquadAnalysisComponent = 91,
    /// A set of knowledge-base squad filters.
    KBSquadFilterSet = 92,
    /// A chat speaker.
    ChatSpeaker = 93,
    /// A controller rumble type.
    RumbleType = 94,
    /// A controller rumble motor.
    RumbleMotor = 95,
    /// A technology data command type.
    TechDataCommandType = 96,
    /// A squad data field selector.
    SquadDataType = 97,
    /// A game event type.
    EventType = 98,
    /// A list of time values.
    TimeList = 99,
    /// A list of design lines.
    DesignLineList = 100,
    /// A game state predicate.
    GameStatePredicate = 101,
    /// A list of floating-point values.
    FloatList = 102,
    /// A minigame location that a player picks in the UI.
    UILocationMinigame = 103,
    /// A squad flag.
    SquadFlag = 104,
    /// A UI item that can flash.
    FlashableUIItem = 105,
    /// A talking head.
    TalkingHead = 106,
    /// A hint concept ID.
    Concept = 107,
    /// A list of hint concept IDs.
    ConceptList = 108,
    /// A user class type.
    UserClassType = 109,
}

impl VarType {
    /// Try to convert from a raw u8 value.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        use zerocopy::TryFromBytes as _;

        if value <= 109 {
            Self::try_read_from_bytes(&value.to_ne_bytes()).ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_retail_discriminants_preserve_declared_order() {
        assert_eq!(VarType::SquadFlag as u8, 104);
        assert_eq!(VarType::FlashableUIItem as u8, 105);
        assert_eq!(VarType::TalkingHead as u8, 106);
        assert_eq!(VarType::Concept as u8, 107);
        assert_eq!(VarType::ConceptList as u8, 108);
        assert_eq!(VarType::UserClassType as u8, 109);
        assert_eq!(VarType::from_u8(104), Some(VarType::SquadFlag));
        assert_eq!(VarType::from_u8(109), Some(VarType::UserClassType));
    }
}
