//! Trigger conditions - evaluate game state to produce boolean results.

use super::VarId;

/// Vanilla condition type IDs.
/// Values match `BTriggerCondition::cTC*` for file format compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ConditionType {
    // Query conditions
    CanGetUnits = 2,
    CanGetSquads = 4,
    TechStatus = 5,
    TriggerActiveTime = 7,
    UnitUnitDistance = 8,
    UnitLocationDistance = 9,
    CompareCount = 14,
    CanPayCost = 15,
    UILocationOK = 16,
    UILocationCancel = 17,
    PlayerSelectingUnit = 20,
    PlayerLookingAtUnit = 21,
    PlayerLookingAtLocation = 22,
    CanUsePower = 27,
    IsUnitPower = 28,
    IsOwnedBy = 29,
    IsProtoObject = 30,
    IsAlive = 80,
    IsDead = 81,
    CheckPlacement = 82,
    CompareBool = 114,
    NextPlayer = 118,
    NextTeam = 119,
    UnitListLocationDistance = 128,
    PlayerInState = 131,
    ContainsGarrisoned = 136,
    NextUnit = 140,
    NextSquad = 141,
    UIUnitOK = 155,
    UIUnitCancel = 156,
    UISquadOK = 157,
    UISquadCancel = 158,
    CompareTime = 170,
    CompareString = 171,
    ComparePercent = 175,
    CompareHitpoints = 182,
    IsMultiplayerActive = 191,
    CanRetrieveExternals = 192,
    SquadLocationDistance = 238,
    CompareCiv = 240,
    CompareAmmoPercent = 256,
    IsHitZoneActive = 260,
    IsIdle = 264,
    GameTime = 266,
    GameTimeReached = 268,
    TriggerActiveTimeReached = 269,
    PlayerSelectingSquad = 280,
    NextObject = 282,
    NextLocation = 292,
    RandomListLocation = 293,
    RefCountUnit = 314,
    RefCountSquad = 315,
    UnitFlag = 320,
    UILocationUILockError = 327,
    UIUnitUILockError = 328,
    UISquadUILockError = 329,
    UILocationWaiting = 331,
    UIUnitWaiting = 332,
    UISquadWaiting = 333,
    CompareCost = 339,
    CheckResourceTotals = 340,
    ComparePopulation = 352,
    CanGetOneUnit = 356,
    CanGetOneSquad = 357,
    CheckDiplomacy = 362,
    CheckModeChange = 386,
    IsBuilt = 414,
    IsMoving = 416,
    PlayerIsHuman = 426,
    PlayerIsGaia = 427,
    ComparePlayers = 428,
    CompareTeams = 429,
    CanGetOnePlayer = 434,
    CanGetOneTeam = 435,
    ComparePlayerUnitCount = 436,
    IsSquadAtMaxSize = 438,
    CanRetrieveExternalLocation = 440,
    CanRetrieveExternalLocationList = 441,
    NextKBBase = 444,
    CompareFloat = 453,
    IsUnderAttack = 461,
    CompareLeader = 476,
    PlayerUsingLeader = 477,
    CanGetOneProtoObject = 485,
    CanGetOneProtoSquad = 486,
    CanGetOneObjectType = 487,
    CanGetOneTech = 488,
    NextProtoObject = 494,
    NextProtoSquad = 495,
    NextObjectType = 496,
    NextTech = 497,
    HasAttached = 508,
    IsMobile = 509,
    CompareProtoSquad = 513,
    HasCinematicTagFired = 518,
    CanGetBuilder = 524,
    UIButtonPressed = 527,
    UIButtonWaiting = 528,
    CompareAIMissionType = 546,
    CompareAIMissionState = 547,
    CompareAIMissionTargetType = 548,
    CanGetOneInteger = 552,
    CanGetUnitsAlongRay = 558,
    CanRemoveOneInteger = 563,
    CanGetOneLocation = 565,
    CanRemoveOneLocation = 566,
    CompareProtoObject = 575,
    CompareTech = 576,
    ProtoObjectListContains = 577,
    ProtoSquadListContains = 578,
    TechListContains = 579,
    CanRemoveOneProtoObject = 583,
    CanRemoveOneProtoSquad = 584,
    CanRemoveOneTech = 585,
    BidState = 586,
    BuildingCommandDone = 596,
    AITopicIsActive = 600,
    HasGarrisoned = 602,
    IsGarrisoned = 603,
    IsAttached = 604,
    ComparePlayerSquadCount = 606,
    IsPassable = 611,
    CanGetCentroid = 621,
    PlayerIsPrimaryUser = 624,
    IsCoop = 627,
    IsConfigDefined = 629,
    CustomCommandCheck = 635,
    UISquadListOK = 640,
    UISquadListCancel = 641,
    UISquadListUILockError = 642,
    UISquadListWaiting = 643,
    IsObjectType = 649,
    IsTimerDone = 661,
    NextKBSquad = 662,
    IsAttacking = 709,
    CanGetUnitLaunchLocation = 714,
    IsGathering = 722,
    IsCapturing = 723,
    PlayerIsComputerAI = 740,
    CanGetGreatestThreat = 743,
    CanGetTargetedSquad = 744,
    IsObjectiveComplete = 746,
    CompareVector = 752,
    GetTableRow = 762,
    CheckPop = 763,
    IsUserModeNormal = 789,
    IsHitched = 800,
    HasHitched = 801,
    SquadSquadDistance = 807,
    EventTriggered = 813,
    IsInQueue = 814,
    CanGetObjects = 819,
    CompareLocStringID = 825,
    CanGetOneObject = 835,
    CanGetCorpseUnits = 844,
    CanGetOneTime = 860,
    CanGetOneDesignLine = 865,
    CompareDesignLine = 866,
    CanRetrieveExternalFlag = 877,
    AICanGetTopicFocus = 883,
    CanGetOneFloat = 885,
    UILocationMinigameWaiting = 893,
    UILocationMinigameOK = 894,
    UILocationMinigameCancel = 895,
    UILocationMinigameUILockError = 896,
    IsSelectable = 899,
    ChatCompleted = 905,
    CinematicCompleted = 906,
    SquadFlag = 911,
    CanGetSocketUnits = 908,
    CanGetOneSocketUnit = 909,
    FadeCompleted = 913,
    IsAutoAttackable = 916,
    IsBeingGatheredFrom = 918,
    AICanGetDifficultySetting = 920,
    ConceptGetParent = 929,
    ConceptGetCommand = 930,
    ConceptGetStateChange = 931,
    ConceptCompareState = 932,
    IsEmptySocketUnit = 942,
    IsForbidden = 944,
    CanGetSocketParentBuilding = 954,
    CanGetRandomLocation = 956,
    CheckDifficulty = 972,
    CanGetDesignSpheres = 974,
    CanRemoveOneFloat = 975,
    CanRetrieveExternalFloat = 977,
    MarkerSquadsInArea = 980,
    CanGetSocketPlugUnit = 986,
    CanGetHoverPoint = 990,
    CanGetCoopPlayer = 991,
    CompareUnit = 1028,
    ASYNCUnitsOnScreenSelected = 1053,
    CheckAndSetFalse = 1056,
    AITopicGetTickets = 1059,

    /// Custom condition for scripting extensions (not in vanilla).
    Custom = 0xFFFF,
}

impl ConditionType {
    /// Try to convert from a raw u16 value.
    pub fn from_u16(value: u16) -> Option<Self> {
        // Only validate known values
        match value {
            2 | 4 | 5 | 7..=9 | 14..=17 | 20..=22 | 27..=30 | 80..=82 | 114 | 118 | 119 | 128
            | 131 | 136 | 140 | 141 | 155..=158 | 170 | 171 | 175 | 182 | 191 | 192 | 238 | 240
            | 256 | 260 | 264 | 266 | 268 | 269 | 280 | 282 | 292 | 293 | 314 | 315 | 320
            | 327..=329 | 331..=333 | 339 | 340 | 352 | 356 | 357 | 362 | 386 | 414 | 416
            | 426..=429 | 434..=436 | 438 | 440 | 441 | 444 | 453 | 461 | 476 | 477 | 485..=488
            | 494..=497 | 508 | 509 | 513 | 518 | 524 | 527 | 528 | 546..=548 | 552 | 558 | 563
            | 565 | 566 | 575..=579 | 583..=586 | 596 | 600 | 602..=604 | 606 | 611 | 621 | 624
            | 627 | 629 | 635 | 640..=643 | 649 | 661 | 662 | 709 | 714 | 722 | 723 | 740
            | 743 | 744 | 746 | 752 | 762 | 763 | 789 | 800 | 801 | 807 | 813 | 814 | 819 | 825
            | 835 | 844 | 860 | 865 | 866 | 877 | 883 | 885 | 893..=896 | 899 | 905 | 906 | 908
            | 909 | 911 | 913 | 916 | 918 | 920 | 929..=932 | 942 | 944 | 954 | 956 | 972 | 974
            | 975 | 977 | 980 | 986 | 990 | 991 | 1028 | 1053 | 1056 | 1059 => {
                Some(unsafe { std::mem::transmute(value) })
            }
            _ => None,
        }
    }
}

/// A condition that can be evaluated against game state.
///
/// Conditions can be:
/// - Simple vanilla conditions (loaded from .triggerscript)
/// - Composite conditions (And, Or, Not)
/// - Custom script conditions (future)
#[derive(Debug, Clone)]
pub struct Condition {
    /// Unique ID within the parent trigger.
    pub id: i32,

    /// The type of condition to evaluate.
    pub condition_type: ConditionType,

    /// Input variable references (indices into TriggerScript's variable list).
    pub inputs: Vec<VarId>,

    /// Output variable references (for conditions that produce values).
    pub outputs: Vec<VarId>,

    /// Whether this is an async condition (UI input, etc).
    pub is_async: bool,

    /// Whether to invert the result.
    pub invert: bool,

    /// Version for compatibility.
    pub version: u8,
}

impl Condition {
    /// Create a new condition.
    pub fn new(id: i32, condition_type: ConditionType) -> Self {
        Self {
            id,
            condition_type,
            inputs: Vec::new(),
            outputs: Vec::new(),
            is_async: false,
            invert: false,
            version: 0,
        }
    }

    /// Add an input variable reference.
    pub fn with_input(mut self, var_id: VarId) -> Self {
        self.inputs.push(var_id);
        self
    }

    /// Add an output variable reference.
    pub fn with_output(mut self, var_id: VarId) -> Self {
        self.outputs.push(var_id);
        self
    }

    /// Set as async condition.
    pub fn async_condition(mut self) -> Self {
        self.is_async = true;
        self
    }

    /// Set as inverted.
    pub fn inverted(mut self) -> Self {
        self.invert = true;
        self
    }
}

