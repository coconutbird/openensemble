//! Trigger conditions - evaluate game state to produce boolean results.

use super::{VarBinding, VarId};

/// Vanilla condition type IDs.
/// Values match `BTriggerCondition::cTC*` for file format compatibility.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    zerocopy::TryFromBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
#[repr(u16)]
pub enum ConditionType {
    // Query conditions
    /// Checks if a query can get units, and writes the units to the output.
    CanGetUnits = 2,
    /// Checks if a query can get squads, and writes the squads to the output.
    CanGetSquads = 4,
    /// Checks the research status of a technology for a player.
    TechStatus = 5,
    /// Compares the time that the trigger is active with a value.
    TriggerActiveTime = 7,
    /// Checks the distance between two units.
    UnitUnitDistance = 8,
    /// Checks the distance between a unit and a location.
    UnitLocationDistance = 9,
    /// Compares two integer counts with an operator.
    CompareCount = 14,
    /// Checks if the player can pay the cost.
    CanPayCost = 15,
    /// Checks if the player confirms the UI location input.
    UILocationOK = 16,
    /// Checks if the player cancels the UI location input.
    UILocationCancel = 17,
    /// Checks if the player selects the unit.
    PlayerSelectingUnit = 20,
    /// Checks if the player looks at the unit.
    PlayerLookingAtUnit = 21,
    /// Checks if the player looks at the location.
    PlayerLookingAtLocation = 22,
    /// Checks if the player can use the power.
    CanUsePower = 27,
    /// Checks if the unit is a power unit.
    IsUnitPower = 28,
    /// Checks if the entity is owned by the player.
    IsOwnedBy = 29,
    /// Checks if the unit is of a prototype object.
    IsProtoObject = 30,
    /// Checks if the entity is alive.
    IsAlive = 80,
    /// Checks if the entity is dead.
    IsDead = 81,
    /// Checks if an object can be placed at a location.
    CheckPlacement = 82,
    /// Compares two boolean values.
    CompareBool = 114,
    /// Gets the next player from an iterator.
    NextPlayer = 118,
    /// Gets the next team from an iterator.
    NextTeam = 119,
    /// Checks the distance between a unit list and a location.
    UnitListLocationDistance = 128,
    /// Checks if the player is in a player state.
    PlayerInState = 131,
    /// Checks if the container holds garrisoned units.
    ContainsGarrisoned = 136,
    /// Gets the next unit from an iterator.
    NextUnit = 140,
    /// Gets the next squad from an iterator.
    NextSquad = 141,
    /// Checks if the player confirms the UI unit input.
    UIUnitOK = 155,
    /// Checks if the player cancels the UI unit input.
    UIUnitCancel = 156,
    /// Checks if the player confirms the UI squad input.
    UISquadOK = 157,
    /// Checks if the player cancels the UI squad input.
    UISquadCancel = 158,
    /// Compares two time values with an operator.
    CompareTime = 170,
    /// Compares two strings.
    CompareString = 171,
    /// Compares two percent values with an operator.
    ComparePercent = 175,
    /// Compares two hitpoint values with an operator.
    CompareHitpoints = 182,
    /// Checks if multiplayer is active.
    IsMultiplayerActive = 191,
    /// Checks if the script can get the external values.
    CanRetrieveExternals = 192,
    /// Checks the distance between a squad and a location.
    SquadLocationDistance = 238,
    /// Compares the civilization of a player with a value.
    CompareCiv = 240,
    /// Compares the ammunition percent of an entity with a value.
    CompareAmmoPercent = 256,
    /// Checks if a hit zone is active.
    IsHitZoneActive = 260,
    /// Checks if the entity is idle.
    IsIdle = 264,
    /// Compares the game time with a value.
    GameTime = 266,
    /// Checks if the game time reaches a value.
    GameTimeReached = 268,
    /// Checks if the trigger active time reaches a value.
    TriggerActiveTimeReached = 269,
    /// Checks if the player selects the squad.
    PlayerSelectingSquad = 280,
    /// Gets the next object from an iterator.
    NextObject = 282,
    /// Gets the next location from an iterator.
    NextLocation = 292,
    /// Gets a random location from a location list.
    RandomListLocation = 293,
    /// Checks the reference count of a unit.
    RefCountUnit = 314,
    /// Checks the reference count of a squad.
    RefCountSquad = 315,
    /// Checks a flag of the unit.
    UnitFlag = 320,
    /// Checks if the UI location input fails because the UI is locked.
    UILocationUILockError = 327,
    /// Checks if the UI unit input fails because the UI is locked.
    UIUnitUILockError = 328,
    /// Checks if the UI squad input fails because the UI is locked.
    UISquadUILockError = 329,
    /// Checks if the UI location input waits for the player.
    UILocationWaiting = 331,
    /// Checks if the UI unit input waits for the player.
    UIUnitWaiting = 332,
    /// Checks if the UI squad input waits for the player.
    UISquadWaiting = 333,
    /// Compares two resource costs.
    CompareCost = 339,
    /// Checks the resource totals of the player.
    CheckResourceTotals = 340,
    /// Compares the population of a player with a value.
    ComparePopulation = 352,
    /// Checks if the script can get one unit from a list.
    CanGetOneUnit = 356,
    /// Checks if the script can get one squad from a list.
    CanGetOneSquad = 357,
    /// Checks the diplomatic relation between two players or teams.
    CheckDiplomacy = 362,
    /// Checks if the squad mode changes.
    CheckModeChange = 386,
    /// Checks if the building is fully built.
    IsBuilt = 414,
    /// Checks if the entity is moving.
    IsMoving = 416,
    /// Checks if the player is a human.
    PlayerIsHuman = 426,
    /// Checks if the player is Gaia (the neutral world player).
    PlayerIsGaia = 427,
    /// Compares two player IDs with an operator.
    ComparePlayers = 428,
    /// Compares two team IDs with an operator.
    CompareTeams = 429,
    /// Checks if the script can get one player from a list.
    CanGetOnePlayer = 434,
    /// Checks if the script can get one team from a list.
    CanGetOneTeam = 435,
    /// Compares the unit count of a player with a value.
    ComparePlayerUnitCount = 436,
    /// Checks if the squad is at its maximum size.
    IsSquadAtMaxSize = 438,
    /// Checks if the script can get the external location value.
    CanRetrieveExternalLocation = 440,
    /// Checks if the script can get the external location list value.
    CanRetrieveExternalLocationList = 441,
    /// Gets the next knowledge-base base from an iterator.
    NextKBBase = 444,
    /// Compares two float values with an operator.
    CompareFloat = 453,
    /// Checks if the entity is under attack.
    IsUnderAttack = 461,
    /// Compares the leader of a player with a value.
    CompareLeader = 476,
    /// Checks if the player uses a leader.
    PlayerUsingLeader = 477,
    /// Checks if the script can get one prototype object from a list.
    CanGetOneProtoObject = 485,
    /// Checks if the script can get one prototype squad from a list.
    CanGetOneProtoSquad = 486,
    /// Checks if the script can get one object type from a list.
    CanGetOneObjectType = 487,
    /// Checks if the script can get one technology from a list.
    CanGetOneTech = 488,
    /// Gets the next prototype object from an iterator.
    NextProtoObject = 494,
    /// Gets the next prototype squad from an iterator.
    NextProtoSquad = 495,
    /// Gets the next object type from an iterator.
    NextObjectType = 496,
    /// Gets the next technology from an iterator.
    NextTech = 497,
    /// Checks if the entity has attached objects.
    HasAttached = 508,
    /// Checks if the entity is mobile.
    IsMobile = 509,
    /// Compares two prototype squad values.
    CompareProtoSquad = 513,
    /// Checks if a cinematic tag fired.
    HasCinematicTagFired = 518,
    /// Checks if the script can get the builder.
    CanGetBuilder = 524,
    /// Checks if the UI button is pressed.
    UIButtonPressed = 527,
    /// Checks if the UI button waits for input.
    UIButtonWaiting = 528,
    /// Compares two AI mission type values.
    CompareAIMissionType = 546,
    /// Compares two AI mission state values.
    CompareAIMissionState = 547,
    /// Compares two AI mission target type values.
    CompareAIMissionTargetType = 548,
    /// Checks if the script can get one integer from a list.
    CanGetOneInteger = 552,
    /// Checks if the script can get the units along a ray.
    CanGetUnitsAlongRay = 558,
    /// Checks if the script can remove one integer from a list.
    CanRemoveOneInteger = 563,
    /// Checks if the script can get one location from a list.
    CanGetOneLocation = 565,
    /// Checks if the script can remove one location from a list.
    CanRemoveOneLocation = 566,
    /// Compares two prototype object values.
    CompareProtoObject = 575,
    /// Compares two technology values.
    CompareTech = 576,
    /// Checks if a prototype object list contains a value.
    ProtoObjectListContains = 577,
    /// Checks if a prototype squad list contains a value.
    ProtoSquadListContains = 578,
    /// Checks if a technology list contains a value.
    TechListContains = 579,
    /// Checks if the script can remove one prototype object from a list.
    CanRemoveOneProtoObject = 583,
    /// Checks if the script can remove one prototype squad from a list.
    CanRemoveOneProtoSquad = 584,
    /// Checks if the script can remove one technology from a list.
    CanRemoveOneTech = 585,
    /// Checks the state of an AI bid.
    BidState = 586,
    /// Checks if a building command is done.
    BuildingCommandDone = 596,
    /// Checks if an AI topic is active.
    AITopicIsActive = 600,
    /// Checks if the entity has garrisoned units.
    HasGarrisoned = 602,
    /// Checks if the entity is garrisoned in a container.
    IsGarrisoned = 603,
    /// Checks if the entity is attached.
    IsAttached = 604,
    /// Compares the squad count of a player with a value.
    ComparePlayerSquadCount = 606,
    /// Checks if a location is passable.
    IsPassable = 611,
    /// Checks if the script can get the centroid.
    CanGetCentroid = 621,
    /// Checks if the player is the primary user.
    PlayerIsPrimaryUser = 624,
    /// Checks if the game is a cooperative game.
    IsCoop = 627,
    /// Checks if a configuration value is defined.
    IsConfigDefined = 629,
    /// Checks the state of a custom command.
    CustomCommandCheck = 635,
    /// Checks if the player confirms the UI squad list input.
    UISquadListOK = 640,
    /// Checks if the player cancels the UI squad list input.
    UISquadListCancel = 641,
    /// Checks if the UI squad list input fails because the UI is locked.
    UISquadListUILockError = 642,
    /// Checks if the UI squad list input waits for the player.
    UISquadListWaiting = 643,
    /// Checks if the unit is of an object type.
    IsObjectType = 649,
    /// Checks if the timer is done.
    IsTimerDone = 661,
    /// Gets the next knowledge-base squad from an iterator.
    NextKBSquad = 662,
    /// Checks if the entity is attacking.
    IsAttacking = 709,
    /// Checks if the script can get the unit launch location.
    CanGetUnitLaunchLocation = 714,
    /// Checks if the entity is gathering.
    IsGathering = 722,
    /// Checks if the entity is capturing.
    IsCapturing = 723,
    /// Checks if the player is a computer AI.
    PlayerIsComputerAI = 740,
    /// Checks if the script can get the greatest threat.
    CanGetGreatestThreat = 743,
    /// Checks if the script can get the targeted squad.
    CanGetTargetedSquad = 744,
    /// Checks if the objective is complete.
    IsObjectiveComplete = 746,
    /// Compares two vectors.
    CompareVector = 752,
    /// Gets a row from a table.
    GetTableRow = 762,
    /// Checks the population of a player.
    CheckPop = 763,
    /// Checks if the user mode is normal.
    IsUserModeNormal = 789,
    /// Checks if the unit is hitched.
    IsHitched = 800,
    /// Checks if the unit has a hitched unit.
    HasHitched = 801,
    /// Checks the distance between two squads.
    SquadSquadDistance = 807,
    /// Checks if a game event fired.
    EventTriggered = 813,
    /// Checks if an item is in a production queue.
    IsInQueue = 814,
    /// Checks if a query can get objects.
    CanGetObjects = 819,
    /// Compares two localized string IDs.
    CompareLocStringID = 825,
    /// Checks if the script can get one object from a list.
    CanGetOneObject = 835,
    /// Checks if the script can get the corpse units.
    CanGetCorpseUnits = 844,
    /// Checks if the script can get one time from a list.
    CanGetOneTime = 860,
    /// Checks if the script can get one design line from a list.
    CanGetOneDesignLine = 865,
    /// Compares two design line values.
    CompareDesignLine = 866,
    /// Checks if the script can get the external flag value.
    CanRetrieveExternalFlag = 877,
    /// Checks if the AI can get the focus of a topic.
    AICanGetTopicFocus = 883,
    /// Checks if the script can get one float from a list.
    CanGetOneFloat = 885,
    /// Checks if the UI location minigame input waits for the player.
    UILocationMinigameWaiting = 893,
    /// Checks if the player confirms the UI location minigame input.
    UILocationMinigameOK = 894,
    /// Checks if the player cancels the UI location minigame input.
    UILocationMinigameCancel = 895,
    /// Checks if the UI location minigame input fails because the UI is locked.
    UILocationMinigameUILockError = 896,
    /// Checks if the entity is selectable.
    IsSelectable = 899,
    /// Checks if a chat message is complete.
    ChatCompleted = 905,
    /// Checks if the current cinematic is complete.
    CinematicCompleted = 906,
    /// Checks a flag of the squad.
    SquadFlag = 911,
    /// Checks if the script can get the socket units.
    CanGetSocketUnits = 908,
    /// Checks if the script can get one socket unit from a list.
    CanGetOneSocketUnit = 909,
    /// Checks if the current screen fade is complete.
    FadeCompleted = 913,
    /// Checks if the entity is auto-attackable.
    IsAutoAttackable = 916,
    /// Checks if the resource is being gathered from.
    IsBeingGatheredFrom = 918,
    /// Checks if the AI can get the difficulty setting.
    AICanGetDifficultySetting = 920,
    /// Gets the parent of a hint concept.
    ConceptGetParent = 929,
    /// Gets the command of a hint concept.
    ConceptGetCommand = 930,
    /// Gets the state change of a hint concept.
    ConceptGetStateChange = 931,
    /// Compares the state of a hint concept with a value.
    ConceptCompareState = 932,
    /// Checks if the socket unit is empty.
    IsEmptySocketUnit = 942,
    /// Checks if an item is forbidden for the player.
    IsForbidden = 944,
    /// Checks if the script can get the socket parent building.
    CanGetSocketParentBuilding = 954,
    /// Checks if the script can get a random location.
    CanGetRandomLocation = 956,
    /// Checks the game difficulty.
    CheckDifficulty = 972,
    /// Checks if the script can get the design spheres.
    CanGetDesignSpheres = 974,
    /// Checks if the script can remove one float from a list.
    CanRemoveOneFloat = 975,
    /// Checks if the script can get the external float value.
    CanRetrieveExternalFloat = 977,
    /// Checks for marked squads in an area.
    MarkerSquadsInArea = 980,
    /// Checks if the script can get the socket plug unit.
    CanGetSocketPlugUnit = 986,
    /// Checks if the script can get the hover point.
    CanGetHoverPoint = 990,
    /// Checks if the script can get the cooperative player.
    CanGetCoopPlayer = 991,
    /// Compares two units.
    CompareUnit = 1028,
    /// Checks if units on the screen are selected (async).
    ASYNCUnitsOnScreenSelected = 1053,
    /// Checks a boolean variable and sets it to false.
    CheckAndSetFalse = 1056,
    /// Gets the ticket count of an AI topic.
    AITopicGetTickets = 1059,

    /// Custom condition for scripting extensions (not in vanilla).
    Custom = 0xFFFF,
}

impl ConditionType {
    /// Try to convert from a raw u16 value.
    #[must_use]
    pub fn from_u16(value: u16) -> Option<Self> {
        use zerocopy::TryFromBytes as _;

        if is_known_condition_value_low(value) || is_known_condition_value_high(value) {
            Self::try_read_from_bytes(&value.to_ne_bytes()).ok()
        } else {
            None
        }
    }
}

fn is_known_condition_value_low(value: u16) -> bool {
    matches!(
        value,
        2 | 4
            | 5
            | 7..=9
            | 14..=17
            | 20..=22
            | 27..=30
            | 80..=82
            | 114
            | 118
            | 119
            | 128
            | 131
            | 136
            | 140
            | 141
            | 155..=158
            | 170
            | 171
            | 175
            | 182
            | 191
            | 192
            | 238
            | 240
            | 256
            | 260
            | 264
            | 266
            | 268
            | 269
            | 280
            | 282
            | 292
            | 293
            | 314
            | 315
            | 320
            | 327..=329
            | 331..=333
            | 339
            | 340
            | 352
            | 356
            | 357
            | 362
            | 386
            | 414
            | 416
            | 426..=429
            | 434..=436
            | 438
            | 440
            | 441
            | 444
            | 453
            | 461
            | 476
            | 477
            | 485..=488
            | 494..=497
            | 508
            | 509
            | 513
            | 518
            | 524
            | 527
            | 528
    )
}

fn is_known_condition_value_high(value: u16) -> bool {
    matches!(
        value,
        546..=548
            | 552
            | 558
            | 563
            | 565
            | 566
            | 575..=579
            | 583..=586
            | 596
            | 600
            | 602..=604
            | 606
            | 611
            | 621
            | 624
            | 627
            | 629
            | 635
            | 640..=643
            | 649
            | 661
            | 662
            | 709
            | 714
            | 722
            | 723
            | 740
            | 743
            | 744
            | 746
            | 752
            | 762
            | 763
            | 789
            | 800
            | 801
            | 807
            | 813
            | 814
            | 819
            | 825
            | 835
            | 844
            | 860
            | 865
            | 866
            | 877
            | 883
            | 885
            | 893..=896
            | 899
            | 905
            | 906
            | 908
            | 909
            | 911
            | 913
            | 916
            | 918
            | 920
            | 929..=932
            | 942
            | 944
            | 954
            | 956
            | 972
            | 974
            | 975
            | 977
            | 980
            | 986
            | 990
            | 991
            | 1028
            | 1053
            | 1056
            | 1059
    )
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

    /// Original condition DBID, including IDs not implemented by this build.
    pub raw_type: u16,

    /// Input signature slots and their sparse script-variable IDs.
    pub inputs: Vec<VarBinding>,

    /// Output signature slots and their sparse script-variable IDs.
    pub outputs: Vec<VarBinding>,

    /// Whether this is an async condition (UI input, etc).
    pub is_async: bool,

    /// Whether to invert the result.
    pub invert: bool,

    /// Version for compatibility.
    pub version: u8,
}

impl Condition {
    /// Create a new condition.
    #[must_use]
    pub fn new(id: i32, condition_type: ConditionType) -> Self {
        Self {
            id,
            condition_type,
            raw_type: condition_type as u16,
            inputs: Vec::new(),
            outputs: Vec::new(),
            is_async: false,
            invert: false,
            version: 0,
        }
    }

    /// Add an input variable reference.
    #[must_use]
    pub fn with_input(mut self, var_id: VarId) -> Self {
        let signature_id = u16::try_from(self.inputs.len() + 1).unwrap_or(u16::MAX);
        self.inputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Add an input variable at an explicit retail signature slot.
    #[must_use]
    pub fn with_input_at(mut self, signature_id: u16, var_id: VarId) -> Self {
        self.inputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Add an output variable reference.
    #[must_use]
    pub fn with_output(mut self, var_id: VarId) -> Self {
        let signature_id = u16::try_from(self.outputs.len() + 1).unwrap_or(u16::MAX);
        self.outputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Add an output variable at an explicit retail signature slot.
    #[must_use]
    pub fn with_output_at(mut self, signature_id: u16, var_id: VarId) -> Self {
        self.outputs.push(VarBinding::new(signature_id, var_id));
        self
    }

    /// Resolve a variable ID by its one-based retail signature slot.
    #[must_use]
    pub fn variable_id(&self, signature_id: u16) -> Option<VarId> {
        self.inputs
            .iter()
            .chain(&self.outputs)
            .find(|binding| binding.signature_id == signature_id)
            .map(|binding| binding.variable_id)
    }

    /// Set as async condition.
    #[must_use]
    pub fn async_condition(mut self) -> Self {
        self.is_async = true;
        self
    }

    /// Set as inverted.
    #[must_use]
    pub fn inverted(mut self) -> Self {
        self.invert = true;
        self
    }
}
