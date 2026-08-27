//! Trigger effects - actions that modify game state.

use super::VarId;

/// Vanilla effect type IDs.
/// Values match `BTriggerEffect::cTE*` for file format compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum EffectType {
    // Control flow
    TriggerActivate = 31,
    TriggerDeactivate = 32,

    // Audio
    PlaySound = 33,
    PlayRelationSound = 34,

    // Entity creation/destruction
    CreateObject = 35,
    CreateSquad = 36,
    Kill = 37,
    Destroy = 38,

    // Resources
    PayCost = 50,
    RefundCost = 51,
    CountIncrement = 52,
    CountDecrement = 53,

    // Location
    RandomLocation = 55,
    LocationTieToGround = 58,
    LocationAdjust = 59,

    // System
    Shutdown = 56,
    LaunchProjectile = 57,

    // Tech
    TechActivate = 60,
    TechDeactivate = 61,

    // Unit commands
    Unload = 65,
    Move = 66,
    TransportSquads = 69,
    CarpetBomb = 71,

    // Attachments
    AttachmentAddType = 74,
    AttachmentRemoveAll = 75,
    AttachmentRemoveType = 83,

    // Powers
    UsePower = 77,
    GetClosestPowerSquad = 78,
    SetUIPowerRadius = 79,

    // UI Input
    InputUILocation = 84,
    InputUIUnit = 161,
    InputUISquad = 162,

    // Copy operations
    CopyTech = 85,
    CopyTechStatus = 86,
    CopyOperator = 87,
    CopyProtoObject = 88,
    CopyObjectType = 89,
    CopyProtoSquad = 90,
    CopySound = 91,
    CopyEntity = 92,
    CopyEntityList = 93,
    CopyCost = 94,
    CopyDistance = 95,
    CopyTime = 96,
    CopyPlayer = 97,
    CopyCount = 98,
    CopyLocation = 99,
    CopyPercent = 100,
    CopyHitpoints = 101,
    CopyBool = 102,
    CopyFloat = 103,
    CopyUnit = 142,
    CopyUnitList = 143,
    CopySquad = 144,
    CopySquadList = 145,
    CopyColor = 168,
    CopyString = 169,

    // Distance calculations
    GetDistanceUnitUnit = 110,
    GetDistanceUnitLocation = 111,

    // Query operations
    GetUnits = 112,
    GetSquads = 113,

    // Work command
    Work = 117,

    // Iterators
    IteratorPlayerList = 120,
    IteratorTeamList = 121,
    IteratorUnitList = 152,
    IteratorSquadList = 153,
    IteratorObjectList = 281,
    IteratorLocationList = 288,
    IteratorKBBaseList = 443,
    IteratorProtoObjectList = 490,
    IteratorProtoSquadList = 491,
    IteratorObjectTypeList = 492,
    IteratorTechList = 493,

    // Team/Player operations
    GetTeams = 122,
    GetTeamPlayers = 123,
    PlayerListAdd = 124,
    PlayerListRemove = 125,
    TeamListAdd = 126,
    TeamListRemove = 127,
    SetPlayerState = 130,
    GetPlayers = 173,
    GetPlayers2 = 431,
    PlayersToTeams = 432,
    TeamsToPlayers = 433,
    GetPlayerTeam = 393,

    // Minimap
    FlareMinimapSpoof = 132,
    FlareMinimapNormal = 135,

    // Objectives
    ObjectiveComplete = 133,
    ObjectiveUserMessage = 163,
    ObjectiveDisplay = 183,

    // Ownership
    ChangeOwner = 137,

    // List operations
    UnitListGetSize = 146,
    SquadListGetSize = 147,
    UnitListAdd = 148,
    SquadListAdd = 149,
    UnitListRemove = 150,
    SquadListRemove = 151,
    LocationListAdd = 289,
    LocationListRemove = 290,
    LocationListGetSize = 291,

    // Unit creation
    CreateUnit = 154,

    // Animation
    PlayAnimationUnit = 159,
    PlayAnimationSquad = 160,

    // Messages
    UserMessage = 164,
    TimeUserMessage = 167,

    // Transform
    Transform = 172,

    // Health
    GetHealth = 174,

    // Math/Random
    RandomCount = 178,
    MathCount = 179,
    MathHitpoints = 180,
    MathPercent = 190,
    MathTime = 263,
    MathDistance = 286,
    MathFloat = 353,
    MathLocation = 295,
    MathResources = 279,

    // String conversion
    AsString = 181,
    AsFloat = 358,
    AsCount = 498,

    // Percentage calculations
    CalculatePercentCount = 184,
    CalculatePercentHitpoints = 185,
    CalculatePercentTime = 186,

    // Lerp operations
    LerpCount = 187,
    LerpColor = 188,
    LerpPercent = 262,
    LerpLocation = 294,

    // Get operations
    GetLocation = 189,
    GetOwner = 193,
    GetGameTime = 265,
    GetGameTimeRemaining = 267,
    GetIdleDuration = 243,
    GetAmmo = 389,
    GetDirection = 499,
    GetDirectionFromLocations = 500,
    GetPlayerCiv = 239,
    GetPlayerLeader = 475,

    // Resources
    SetResources = 277,
    GetResources = 278,
    SetResourcesTotals = 337,
    GetResourcesTotals = 338,

    // Hit zones
    GetHitZoneHealth = 257,
    SetHitZoneHealth = 258,
    SetHitZoneActive = 259,

    // Enable/Disable
    EnableAttackNotifications = 255,
    EnableFogOfWar = 385,
    EnableShield = 463,

    // Misc
    Forbid = 283,
    InvertBool = 284,
    Revealer = 285,
    GroupDeactivate = 287,
    Repair = 318,
    Damage = 325,
    CombatDamage = 336,
    Teleport = 354,
    Settle = 359,
    Cloak = 417,
    Blocker = 418,
    SensorLock = 419,
    ReinforceSquad = 430,
    MovePath = 439,
    SetDirection = 489,
    SetAmmo = 390,
    SetIgnoreUserInput = 382,

    // Partition/Shuffle
    SquadListPartition = 296,
    SquadListShuffle = 297,
    LocationListShuffle = 298,
    EntityListShuffle = 299,
    PlayerListShuffle = 300,
    TeamListShuffle = 301,
    UnitListShuffle = 302,
    ProtoObjectListShuffle = 303,
    ObjectTypeListShuffle = 304,
    ProtoSquadListShuffle = 305,
    TechListShuffle = 306,
    UnitListPartition = 308,
    LocationListPartition = 309,

    // RefCount
    RefCountUnitAdd = 311,
    RefCountUnitRemove = 312,
    RefCountSquadAdd = 316,
    RefCountSquadRemove = 317,

    // Unit flags
    UnitFlagSet = 319,
    GetChildUnits = 324,

    // UI
    UIUnlock = 330,

    // List diff
    SquadListDiff = 334,
    UnitListDiff = 335,

    // Entity filters
    EntityFilterClear = 341,
    EntityFilterAddIsAlive = 342,
    EntityFilterAddInList = 343,
    EntityFilterAddPlayers = 344,
    EntityFilterAddTeams = 345,
    EntityFilterAddProtoObjects = 346,
    EntityFilterAddProtoSquads = 347,
    EntityFilterAddObjectTypes = 348,
    UnitListFilter = 349,
    SquadListFilter = 350,
    EntityFilterAddRefCount = 351,
    EntityFilterAddIsIdle = 355,
    EntityFilterAddDiplomacy = 379,

    // Attachments
    AttachmentRemoveObject = 361,
    AttachmentAddObject = 363,
    AttachmentAddUnit = 364,
    AttachmentRemoveUnit = 365,

    // Bid system
    BidCreateBlank = 369,
    BidCreateBuilding = 370,
    BidCreateTech = 371,
    BidCreateSquad = 372,
    BidDelete = 373,
    BidSetBuilding = 374,
    BidSetTech = 375,
    BidSetSquad = 376,
    BidClear = 377,
    BidSetPriority = 378,
    BidPurchase = 391,

    // Scripts/Cinematics
    LaunchScript = 392,
    LaunchCinematic = 480,

    // Squad mode
    ChangeSquadMode = 388,

    // Copy lists
    CopyProtoObjectList = 481,
    CopyProtoSquadList = 482,
    CopyObjectTypeList = 483,
    CopyTechList = 484,
    CopyDirection = 501,
    CopyObjective = 360,
    CopyMessageIndex = 261,
    CopyKBBase = 455,

    // KB operations
    KBBaseGetDistance = 446,
    KBBaseGetMass = 447,
    KBBQReset = 448,
    KBBQExecute = 449,
    KBBQPointRadius = 450,
    KBBQPlayerRelation = 451,
    KBBQMinStaleness = 471,
    KBBQMaxStaleness = 472,

    // Powers
    PowerGrant = 456,
    PowerRevoke = 457,

    // Proto queries
    GetSquadTrainerType = 458,
    GetTechResearcherType = 459,

    // Design
    DesignLineGetPoints = 460,
    DesignFindSphere = 425,
    ModifyDataScalar = 413,
    ModifyProtoData = 237,

    /// Custom effect for scripting extensions (not in vanilla).
    Custom = 0xFFFF,
}

impl EffectType {
    /// Try to convert from a raw u16 value.
    /// Returns None for obsolete or unknown values.
    #[must_use]
    pub fn from_u16(value: u16) -> Option<Self> {
        // This is a simplified check - in practice we'd have a complete match
        match value {
            31..=38
            | 50..=53
            | 55..=61
            | 65
            | 66
            | 69
            | 71
            | 74
            | 75
            | 77..=79
            | 83..=103
            | 110..=113
            | 117
            | 120..=127
            | 130
            | 132
            | 133
            | 135
            | 137
            | 142..=154
            | 159..=164
            | 167..=175
            | 178..=181
            | 183..=190
            | 193
            | 237
            | 239
            | 243
            | 255
            | 257..=263
            | 265
            | 267
            | 277..=279
            | 281
            | 283..=298
            | 300..=306
            | 308
            | 309
            | 311
            | 312
            | 316..=320
            | 324
            | 325
            | 330
            | 334..=351
            | 353..=365
            | 369..=379
            | 382
            | 385
            | 388..=393
            | 413
            | 417..=419
            | 425
            | 430..=433
            | 439
            | 443
            | 446..=451
            | 455..=460
            | 463
            | 471
            | 472
            | 475
            | 480..=484
            | 489..=493
            | 498
            | 499..=501 => Some(unsafe { std::mem::transmute::<u16, EffectType>(value) }),
            _ => None,
        }
    }
}

/// An effect that modifies game state when executed.
#[derive(Debug, Clone)]
pub struct Effect {
    /// Unique ID within the parent trigger.
    pub id: i32,

    /// The type of effect to execute.
    pub effect_type: EffectType,

    /// Input variable references (indices into `TriggerScript`'s variable list).
    pub inputs: Vec<VarId>,

    /// Output variable references (for effects that produce values).
    pub outputs: Vec<VarId>,

    /// Version for compatibility.
    pub version: u8,
}

impl Effect {
    /// Create a new effect.
    #[must_use]
    pub fn new(id: i32, effect_type: EffectType) -> Self {
        Self {
            id,
            effect_type,
            inputs: Vec::new(),
            outputs: Vec::new(),
            version: 0,
        }
    }

    /// Add an input variable reference.
    #[must_use]
    pub fn with_input(mut self, var_id: VarId) -> Self {
        self.inputs.push(var_id);
        self
    }

    /// Add an output variable reference.
    #[must_use]
    pub fn with_output(mut self, var_id: VarId) -> Self {
        self.outputs.push(var_id);
        self
    }
}
