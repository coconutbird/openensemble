//! Trigger variable types - matches vanilla enum for compatibility.

/// Variable types supported by the trigger system.
/// Values match vanilla `BTriggerVar::cVarType*` for file format compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum VarType {
    #[default]
    Invalid = 0,
    Tech = 1,
    TechStatus = 2,
    Operator = 3,
    ProtoObject = 4,
    ObjectType = 5,
    ProtoSquad = 6,
    Sound = 7,
    Entity = 8,
    EntityList = 9,
    Trigger = 10,
    Time = 11,
    Player = 12,
    UILocation = 13,
    UIEntity = 14,
    Cost = 15,
    AnimType = 16,
    ActionStatus = 17,
    Power = 18,
    Bool = 19,
    Float = 20,
    Iterator = 21,
    Team = 22,
    PlayerList = 23,
    TeamList = 24,
    PlayerState = 25,
    Objective = 26,
    Unit = 27,
    UnitList = 28,
    Squad = 29,
    SquadList = 30,
    UIUnit = 31,
    UISquad = 32,
    UISquadList = 33,
    String = 34,
    MessageIndex = 35,
    MessageJustify = 36,
    MessagePoint = 37,
    Color = 38,
    ProtoObjectList = 39,
    ObjectTypeList = 40,
    ProtoSquadList = 41,
    TechList = 42,
    MathOperator = 43,
    ObjectDataType = 44,
    ObjectDataRelative = 45,
    Civ = 46,
    ProtoObjectCollection = 47,
    Object = 48,
    ObjectList = 49,
    Group = 50,
    RefCountType = 51,
    UnitFlag = 52,
    LOSType = 53,
    EntityFilterSet = 54,
    PopBucket = 55,
    ListPosition = 56,
    RelationType = 57,
    ExposedAction = 58,
    SquadMode = 59,
    ExposedScript = 60,
    KBBase = 61,
    KBBaseList = 62,
    DataScalar = 63,
    KBBaseQuery = 64,
    DesignLine = 65,
    LocStringID = 66,
    Leader = 67,
    Cinematic = 68,
    FlareType = 69,
    CinematicTag = 70,
    IconType = 71,
    Difficulty = 72,
    Integer = 73,
    HUDItem = 74,
    ControlType = 75,
    UIButton = 76,
    MissionType = 77,
    MissionState = 78,
    MissionTargetType = 79,
    IntegerList = 80,
    BidType = 81,
    BidState = 82,
    BuildingCommandState = 83,
    Vector = 84,
    VectorList = 85,
    PlacementRule = 86,
    KBSquad = 87,
    KBSquadList = 88,
    KBSquadQuery = 89,
    AISquadAnalysis = 90,
    AISquadAnalysisComponent = 91,
    KBSquadFilterSet = 92,
    ChatSpeaker = 93,
    RumbleType = 94,
    RumbleMotor = 95,
    TechDataCommandType = 96,
    SquadDataType = 97,
    EventType = 98,
    TimeList = 99,
    DesignLineList = 100,
    GameStatePredicate = 101,
    FloatList = 102,
    UILocationMinigame = 103,
    SquadFlag = 104,
    FlashableUIItem = 105,
    TalkingHead = 106,
    Concept = 107,
    ConceptList = 108,
    UserClassType = 109,
}

impl VarType {
    /// Try to convert from a raw u8 value.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        if value <= 109 {
            // SAFETY: All values 0-109 are valid enum variants
            Some(unsafe { std::mem::transmute::<u8, VarType>(value) })
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
