//! Vanilla trigger effect type IDs.

use zerocopy::TryFromBytes;

/// Vanilla effect type IDs.
/// Values match `BTriggerEffect::cTE*` for file format compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromBytes)]
#[repr(u16)]
pub enum EffectType {
    // Control flow
    /// Activates a trigger in the same script.
    TriggerActivate = 31,
    /// Deactivates a trigger in the same script.
    TriggerDeactivate = 32,

    // Audio
    /// Plays a sound.
    PlaySound = 33,
    /// Plays a sound that depends on the relation of each player to a source player.
    PlayRelationSound = 34,

    // Entity creation/destruction
    /// Creates an object from a prototype object.
    CreateObject = 35,
    /// Creates a squad from a prototype squad.
    CreateSquad = 36,
    /// Kills entities with a normal death.
    Kill = 37,
    /// Removes entities immediately, without a normal death.
    Destroy = 38,

    // Resources
    /// Takes a cost from the resources of a player.
    PayCost = 50,
    /// Gives a cost back to the resources of a player.
    RefundCost = 51,
    /// Adds one to a count variable.
    CountIncrement = 52,
    /// Subtracts one from a count variable.
    CountDecrement = 53,

    // Location
    /// Writes a random location to a variable.
    RandomLocation = 55,
    /// Moves a location onto the terrain surface.
    LocationTieToGround = 58,
    /// Adds an offset to a location.
    LocationAdjust = 59,

    // System
    /// Stops the trigger script.
    Shutdown = 56,
    /// Launches a projectile.
    LaunchProjectile = 57,

    // Tech
    /// Activates a technology for a player.
    TechActivate = 60,
    /// Deactivates a technology for a player.
    TechDeactivate = 61,

    // Unit commands
    /// Orders squads to unload their passengers.
    Unload = 65,
    /// Orders squads to move.
    Move = 66,
    /// Moves squads with transport units.
    TransportSquads = 69,
    /// Starts a carpet bomb attack along a line.
    CarpetBomb = 71,

    // Attachments
    /// Adds an attachment type to entities.
    AttachmentAddType = 74,
    /// Removes all attachments from entities.
    AttachmentRemoveAll = 75,
    /// Removes one attachment type from entities.
    AttachmentRemoveType = 83,

    // Powers
    /// Uses a power for a player.
    UsePower = 77,
    /// Gets the squad nearest to a power location.
    GetClosestPowerSquad = 78,
    /// Sets the radius that the UI shows for a power.
    SetUIPowerRadius = 79,

    // UI Input
    /// Asks the user to select a location in the UI.
    InputUILocation = 84,
    /// Asks the user to select a unit in the UI.
    InputUIUnit = 161,
    /// Asks the user to select a squad in the UI.
    InputUISquad = 162,

    // Copy operations
    /// Copies a technology value.
    CopyTech = 85,
    /// Copies a technology status value.
    CopyTechStatus = 86,
    /// Copies an operator value.
    CopyOperator = 87,
    /// Copies a prototype object value.
    CopyProtoObject = 88,
    /// Copies an object type value.
    CopyObjectType = 89,
    /// Copies a prototype squad value.
    CopyProtoSquad = 90,
    /// Copies a sound value.
    CopySound = 91,
    /// Copies an entity value.
    CopyEntity = 92,
    /// Copies an entity list value.
    CopyEntityList = 93,
    /// Copies a cost value.
    CopyCost = 94,
    /// Copies a distance value.
    CopyDistance = 95,
    /// Copies a time value.
    CopyTime = 96,
    /// Copies a player value.
    CopyPlayer = 97,
    /// Copies a count value.
    CopyCount = 98,
    /// Copies a location value.
    CopyLocation = 99,
    /// Copies a percent value.
    CopyPercent = 100,
    /// Copies a hitpoints value.
    CopyHitpoints = 101,
    /// Copies a bool value.
    CopyBool = 102,
    /// Copies a float value.
    CopyFloat = 103,
    /// Copies a unit value.
    CopyUnit = 142,
    /// Copies a unit list value.
    CopyUnitList = 143,
    /// Copies a squad value.
    CopySquad = 144,
    /// Copies a squad list value.
    CopySquadList = 145,
    /// Copies a color value.
    CopyColor = 168,
    /// Copies a string value.
    CopyString = 169,

    // Distance calculations
    /// Gets the distance between two units.
    GetDistanceUnitUnit = 110,
    /// Gets the distance between a unit and a location.
    GetDistanceUnitLocation = 111,

    // Query operations
    /// Gets the units of a squad or entity list.
    GetUnits = 112,
    /// Gets the squads of a unit or entity list.
    GetSquads = 113,

    // Work command
    /// Gives squads a work command.
    Work = 117,

    // Iterators
    /// Attaches an iterator to a player list.
    IteratorPlayerList = 120,
    /// Attaches an iterator to a team list.
    IteratorTeamList = 121,
    /// Attaches an iterator to a unit list.
    IteratorUnitList = 152,
    /// Attaches an iterator to a squad list.
    IteratorSquadList = 153,
    /// Attaches an iterator to an object list.
    IteratorObjectList = 281,
    /// Attaches an iterator to a location list.
    IteratorLocationList = 288,
    /// Attaches an iterator to a knowledge-base base list.
    IteratorKBBaseList = 443,
    /// Attaches an iterator to a prototype object list.
    IteratorProtoObjectList = 490,
    /// Attaches an iterator to a prototype squad list.
    IteratorProtoSquadList = 491,
    /// Attaches an iterator to an object type list.
    IteratorObjectTypeList = 492,
    /// Attaches an iterator to a technology list.
    IteratorTechList = 493,

    // Team/Player operations
    /// Gets the list of teams.
    GetTeams = 122,
    /// Gets the players of a team.
    GetTeamPlayers = 123,
    /// Adds a player to a player list.
    PlayerListAdd = 124,
    /// Removes a player from a player list.
    PlayerListRemove = 125,
    /// Adds a team to a team list.
    TeamListAdd = 126,
    /// Removes a team from a team list.
    TeamListRemove = 127,
    /// Sets the state of a player, for example won or lost.
    SetPlayerState = 130,
    /// Gets a list of players.
    GetPlayers = 173,
    /// Gets the players that have a relation to a test player (version 2 only).
    GetPlayers2 = 431,
    /// Gets the number of players in a player list.
    PlayerListGetSize = 970,
    /// Gets the NPC players with a name.
    GetNPCPlayersByName = 943,
    /// Converts a player list to the list of their teams.
    PlayersToTeams = 432,
    /// Converts a team list to the list of their players.
    TeamsToPlayers = 433,
    /// Gets the team of a player.
    GetPlayerTeam = 393,

    // Minimap
    /// Shows a minimap flare that does not come from a real player.
    FlareMinimapSpoof = 132,
    /// Shows a normal minimap flare.
    FlareMinimapNormal = 135,
    /// Creates an icon object on the map.
    CreateIconObject = 520,

    // Objectives
    /// Marks an objective as complete or not complete.
    ObjectiveComplete = 133,
    /// Shows the user message of an objective.
    ObjectiveUserMessage = 163,
    /// Shows or hides an objective.
    ObjectiveDisplay = 183,

    // Ownership
    /// Gives squads to a different player.
    ChangeOwner = 137,

    // List operations
    /// Gets the number of units in a unit list.
    UnitListGetSize = 146,
    /// Gets the number of squads in a squad list.
    SquadListGetSize = 147,
    /// Adds units to a unit list.
    UnitListAdd = 148,
    /// Adds squads to a squad list.
    SquadListAdd = 149,
    /// Removes units from a unit list.
    UnitListRemove = 150,
    /// Removes squads from a squad list.
    SquadListRemove = 151,
    /// Adds a location to a location list.
    LocationListAdd = 289,
    /// Removes a location from a location list.
    LocationListRemove = 290,
    /// Gets the number of locations in a location list.
    LocationListGetSize = 291,

    // Unit creation
    /// Creates a unit from a prototype object.
    CreateUnit = 154,

    // Animation
    /// Plays an animation on units.
    PlayAnimationUnit = 159,
    /// Plays an animation on squads.
    PlayAnimationSquad = 160,
    /// Plays an animation on objects.
    PlayAnimationObject = 588,

    // Messages
    /// Shows a message to users.
    UserMessage = 164,
    /// Shows a message to users for a time.
    TimeUserMessage = 167,

    // Transform
    /// Changes entities into a different prototype.
    Transform = 172,

    // Health
    /// Gets the health of an entity.
    GetHealth = 174,

    // Math/Random
    /// Writes a random count in a range to a variable.
    RandomCount = 178,
    /// Applies a math operator to two counts.
    MathCount = 179,
    /// Applies a math operator to two hitpoints values.
    MathHitpoints = 180,
    /// Applies a math operator to two percent values.
    MathPercent = 190,
    /// Applies a math operator to two time values.
    MathTime = 263,
    /// Applies a math operator to two distances.
    MathDistance = 286,
    /// Applies a math operator to two floats.
    MathFloat = 353,
    /// Applies a math operator to two locations.
    MathLocation = 295,
    /// Applies a math operator to two resource values.
    MathResources = 279,
    /// Writes a random time in a range to a variable.
    RandomTime = 516,

    // String conversion
    /// Converts a value to a string.
    AsString = 181,
    /// Converts a value to a float.
    AsFloat = 358,
    /// Converts a value to a count.
    AsCount = 498,

    // Percentage calculations
    /// Calculates the percent of one count in a second count.
    CalculatePercentCount = 184,
    /// Calculates the percent of one hitpoints value in a second value.
    CalculatePercentHitpoints = 185,
    /// Calculates the percent of one time in a second time.
    CalculatePercentTime = 186,

    // Lerp operations
    /// Interpolates linearly between two counts.
    LerpCount = 187,
    /// Interpolates linearly between two colors.
    LerpColor = 188,
    /// Interpolates linearly between two percent values.
    LerpPercent = 262,
    /// Interpolates linearly between two locations.
    LerpLocation = 294,
    /// Interpolates linearly between two times.
    LerpTime = 748,

    // Get operations
    /// Gets the location of an entity.
    GetLocation = 189,
    /// Gets the owner of an entity.
    GetOwner = 193,
    /// Gets the current game time.
    GetGameTime = 265,
    /// Gets the game time that remains.
    GetGameTimeRemaining = 267,
    /// Gets the time for which an entity was idle.
    GetIdleDuration = 243,
    /// Gets the ammunition of a unit or squad.
    GetAmmo = 389,
    /// Gets the forward direction of an entity.
    GetDirection = 499,
    /// Gets the direction from one location to a second location.
    GetDirectionFromLocations = 500,
    /// Gets the civilization of a player.
    GetPlayerCiv = 239,
    /// Gets the leader of a player.
    GetPlayerLeader = 475,
    /// Gets the parent squad of a unit.
    GetParentSquad = 519,
    /// Gets the population values of a player.
    GetPlayerPop = 646,
    /// Gets the mean location of entities or locations.
    GetMeanLocation = 670,

    // Resources
    /// Sets the current resources of a player.
    SetResources = 277,
    /// Gets the current resources of a player.
    GetResources = 278,
    /// Sets the resource totals of a player.
    SetResourcesTotals = 337,
    /// Gets the resource totals of a player.
    GetResourcesTotals = 338,

    // Hit zones
    /// Gets the health of a hit zone on a unit.
    GetHitZoneHealth = 257,
    /// Sets the health of a hit zone on a unit.
    SetHitZoneHealth = 258,
    /// Activates or deactivates a hit zone on a unit.
    SetHitZoneActive = 259,

    // Enable/Disable
    /// Enables or disables attack notifications.
    EnableAttackNotifications = 255,
    /// Enables or disables the fog of war.
    EnableFogOfWar = 385,
    /// Enables or disables the shield of entities.
    EnableShield = 463,

    // Misc
    /// Sets the `Forbid` flag of prototypes for a player.
    Forbid = 283,
    /// Inverts a bool variable.
    InvertBool = 284,
    /// Creates a revealer that shows an area to a player.
    Revealer = 285,
    /// Deactivates a group of triggers.
    GroupDeactivate = 287,
    /// Adds hitpoints to entities.
    Repair = 318,
    /// Removes hitpoints from entities.
    Damage = 325,
    /// Applies combat damage to entities.
    CombatDamage = 336,
    /// Moves entities to a location immediately.
    Teleport = 354,
    /// Orders squads to settle into formation.
    Settle = 359,
    /// Enables or disables the cloak of squads.
    Cloak = 417,
    /// Creates or removes a blocker that stops movement.
    Blocker = 418,
    /// Locks the sensors of entities on a target.
    SensorLock = 419,
    /// Adds reinforcement units to a squad.
    ReinforceSquad = 430,
    /// Orders squads to move along a path.
    MovePath = 439,
    /// Sets the forward direction of entities.
    SetDirection = 489,
    /// Sets the ammunition of a unit or squad.
    SetAmmo = 390,
    /// Makes the game ignore or accept user input.
    SetIgnoreUserInput = 382,
    /// Sets whether entities can move.
    SetMobile = 510,

    // Partition/Shuffle
    /// Divides a squad list into two lists.
    SquadListPartition = 296,
    /// Puts a squad list in random order.
    SquadListShuffle = 297,
    /// Puts a location list in random order.
    LocationListShuffle = 298,
    /// Puts an entity list in random order.
    EntityListShuffle = 299,
    /// Puts a player list in random order.
    PlayerListShuffle = 300,
    /// Puts a team list in random order.
    TeamListShuffle = 301,
    /// Puts a unit list in random order.
    UnitListShuffle = 302,
    /// Puts a prototype object list in random order.
    ProtoObjectListShuffle = 303,
    /// Puts an object type list in random order.
    ObjectTypeListShuffle = 304,
    /// Puts a prototype squad list in random order.
    ProtoSquadListShuffle = 305,
    /// Puts a technology list in random order.
    TechListShuffle = 306,
    /// Divides a unit list into two lists.
    UnitListPartition = 308,
    /// Divides a location list into two lists.
    LocationListPartition = 309,

    // RefCount
    /// Adds a reference count to units.
    RefCountUnitAdd = 311,
    /// Removes a reference count from units.
    RefCountUnitRemove = 312,
    /// Adds a reference count to squads.
    RefCountSquadAdd = 316,
    /// Removes a reference count from squads.
    RefCountSquadRemove = 317,

    // Unit flags
    /// Sets a flag on units.
    UnitFlagSet = 319,
    /// Gets the child units of an entity.
    GetChildUnits = 324,

    // UI
    /// Unlocks a part of the UI.
    UIUnlock = 330,
    /// Shows or hides a HUD item.
    HudToggle = 526,
    /// Enables or disables the render of the terrain skirt.
    SetRenderTerrainSkirt = 532,

    // List diff
    /// Gets the squads of one squad list that are not in a second list.
    SquadListDiff = 334,
    /// Gets the units of one unit list that are not in a second list.
    UnitListDiff = 335,

    // Entity filters
    /// Removes all filters from an entity filter set.
    EntityFilterClear = 341,
    /// Adds an alive test to an entity filter set.
    EntityFilterAddIsAlive = 342,
    /// Adds an in-list test to an entity filter set.
    EntityFilterAddInList = 343,
    /// Adds a player test to an entity filter set.
    EntityFilterAddPlayers = 344,
    /// Adds a team test to an entity filter set.
    EntityFilterAddTeams = 345,
    /// Adds a prototype object test to an entity filter set.
    EntityFilterAddProtoObjects = 346,
    /// Adds a prototype squad test to an entity filter set.
    EntityFilterAddProtoSquads = 347,
    /// Adds an object type test to an entity filter set.
    EntityFilterAddObjectTypes = 348,
    /// Filters a unit list with an entity filter set.
    UnitListFilter = 349,
    /// Filters a squad list with an entity filter set.
    SquadListFilter = 350,
    /// Adds a reference count test to an entity filter set.
    EntityFilterAddRefCount = 351,
    /// Adds an idle test to an entity filter set.
    EntityFilterAddIsIdle = 355,
    /// Adds a diplomacy test to an entity filter set.
    EntityFilterAddDiplomacy = 379,

    // Attachments
    /// Removes an attached object from entities.
    AttachmentRemoveObject = 361,
    /// Attaches an object to entities.
    AttachmentAddObject = 363,
    /// Attaches a unit to entities.
    AttachmentAddUnit = 364,
    /// Removes an attached unit from entities.
    AttachmentRemoveUnit = 365,

    // Bid system
    /// Creates an empty AI bid.
    BidCreateBlank = 369,
    /// Creates an AI bid for a building.
    BidCreateBuilding = 370,
    /// Creates an AI bid for a technology.
    BidCreateTech = 371,
    /// Creates an AI bid for a squad.
    BidCreateSquad = 372,
    /// Deletes an AI bid.
    BidDelete = 373,
    /// Sets the building of an AI bid.
    BidSetBuilding = 374,
    /// Sets the technology of an AI bid.
    BidSetTech = 375,
    /// Sets the squad of an AI bid.
    BidSetSquad = 376,
    /// Clears an AI bid.
    BidClear = 377,
    /// Sets the priority of an AI bid.
    BidSetPriority = 378,
    /// Buys the item of an AI bid.
    BidPurchase = 391,

    // Scripts/Cinematics
    /// Starts a different trigger script.
    LaunchScript = 392,
    /// Starts a cinematic.
    LaunchCinematic = 480,
    /// Plays a chat message.
    PlayChat = 729,

    // General events
    /// Subscribes a variable to a general event.
    EventSubscribe = 811,
    /// Sets the filter of a general event subscription.
    EventSetFilter = 812,
    /// Resets a general event subscription.
    EventReset = 837,
    /// Adds a camera filter to a general event subscription.
    EventFilterCamera = 838,
    /// Adds an entity filter to a general event subscription.
    EventFilterEntity = 839,
    /// Adds an entity list filter to a general event subscription.
    EventFilterEntityList = 840,
    /// Deletes a general event subscription.
    EventDelete = 988,
    /// Removes all filters from a general event subscription.
    EventClearFilters = 989,
    /// Subscribes a variable to a general event and counts the event fires.
    EventSubscribeUseCount = 1018,

    // Squad mode
    /// Changes the mode of squads.
    ChangeSquadMode = 388,

    // Copy lists
    /// Copies a prototype object list value.
    CopyProtoObjectList = 481,
    /// Copies a prototype squad list value.
    CopyProtoSquadList = 482,
    /// Copies an object type list value.
    CopyObjectTypeList = 483,
    /// Copies a technology list value.
    CopyTechList = 484,
    /// Copies an integer list value.
    CopyIntegerList = 562,
    /// Copies a location list value.
    CopyLocationList = 609,
    /// Copies a direction value.
    CopyDirection = 501,
    /// Copies an objective value.
    CopyObjective = 360,
    /// Copies a message index value.
    CopyMessageIndex = 261,
    /// Copies a knowledge-base base value.
    CopyKBBase = 455,

    // Typed value-list operations
    /// Adds a prototype object to a prototype object list.
    ProtoObjectListAdd = 569,
    /// Removes a prototype object from a prototype object list.
    ProtoObjectListRemove = 570,
    /// Adds a prototype squad to a prototype squad list.
    ProtoSquadListAdd = 571,
    /// Removes a prototype squad from a prototype squad list.
    ProtoSquadListRemove = 572,
    /// Adds a technology to a technology list.
    TechListAdd = 573,
    /// Removes a technology from a technology list.
    TechListRemove = 574,
    /// Adds an integer to an integer list.
    IntegerListAdd = 580,
    /// Removes an integer from an integer list.
    IntegerListRemove = 581,
    /// Gets the number of integers in an integer list.
    IntegerListGetSize = 582,

    // Command state
    /// Gives a command to a building, for example to train or research.
    BuildingCommand = 559,
    /// Adds a custom command to a building.
    CustomCommandAdd = 633,
    /// Removes a custom command from a building.
    CustomCommandRemove = 634,
    /// Clears the state of a building command.
    ClearBuildingCommandState = 940,

    // Player resource income
    /// Sets whether users can select entities.
    SetSelectable = 900,
    /// Sets the resource trickle rate of a player.
    SetTrickleRate = 901,
    /// Gets the resource trickle rate of a player.
    GetTrickleRate = 902,
    /// Sets whether automatic targeting can attack entities.
    SetAutoAttackable = 915,

    // KB operations
    /// Gets the distance to a knowledge-base base.
    KBBaseGetDistance = 446,
    /// Gets the mass of a knowledge-base base.
    KBBaseGetMass = 447,
    /// Resets a knowledge-base base query.
    KBBQReset = 448,
    /// Runs a knowledge-base base query.
    KBBQExecute = 449,
    /// Sets the point and radius of a knowledge-base base query.
    KBBQPointRadius = 450,
    /// Sets the player relation of a knowledge-base base query.
    KBBQPlayerRelation = 451,
    /// Sets the minimum staleness of a knowledge-base base query.
    KBBQMinStaleness = 471,
    /// Sets the maximum staleness of a knowledge-base base query.
    KBBQMaxStaleness = 472,

    // Powers
    /// Gives a power to a player.
    PowerGrant = 456,
    /// Removes a power from a player.
    PowerRevoke = 457,

    // Proto queries
    /// Gets the building type that trains a prototype squad.
    GetSquadTrainerType = 458,
    /// Gets the building type that researches a technology.
    GetTechResearcherType = 459,
    /// Gets the prototype squad of a squad.
    GetProtoSquad = 630,

    // Late copy/list operations
    /// Copies a localized string ID value.
    CopyLocStringID = 818,
    /// Creates an obstruction unit.
    CreateObstructionUnit = 833,
    /// Removes objects from an object list.
    ObjectListRemove = 836,

    // Squad veterancy
    /// Gives experience to squads.
    AddXP = 852,

    // Whole-map exploration
    /// Clears the black map, so that all of the map is explored.
    ClearBlackMap = 863,

    // Renderer/UI presentation controls
    /// Shakes the camera.
    CameraShake = 632,
    /// Shows or hides the pointer to an objective.
    ShowObjectivePointer = 687,
    /// Starts a controller rumble.
    RumbleStart = 773,
    /// Stops a controller rumble.
    RumbleStop = 774,
    /// Creates a hint callout.
    HintCalloutCreate = 809,
    /// Removes a hint callout.
    HintCalloutDestroy = 810,
    /// Enables or disables chats.
    EnableChats = 841,
    /// Sets the camera of players.
    SetCamera = 884,
    /// Fades the screen to a color.
    FadeToColor = 912,
    /// Plays a fade transition on the screen.
    FadeTransition = 922,
    /// Shows a text message to players.
    ShowMessage = 904,

    // Batch squad creation
    /// Creates many squads from a list of prototype squads.
    CreateSquads = 875,

    // Design
    /// Gets the points of a scenario design line.
    DesignLineGetPoints = 460,
    /// Copies a design line list value.
    CopyDesignLineList = 867,
    /// Adds a design line to a design line list.
    DesignLineListAdd = 868,
    /// Removes a design line from a design line list.
    DesignLineListRemove = 869,
    /// Gets the number of design lines in a design line list.
    DesignLineListGetSize = 870,
    /// Copies a design line value.
    CopyDesignLine = 872,
    /// Finds a scenario design sphere.
    DesignFindSphere = 425,
    /// Changes a data scalar of live units.
    ModifyDataScalar = 413,
    /// Changes the prototype data of a player.
    ModifyProtoData = 237,

    // Debug/presentation
    /// Shows a time variable for debug.
    DebugVarTime = 207,
    /// Shows a count variable for debug.
    DebugVarCount = 209,
    /// Shows a float variable for debug.
    DebugVarFloat = 218,
    /// Shows a player list variable for debug.
    DebugVarPlayerList = 221,
    /// Shows a string variable for debug.
    DebugVarString = 230,

    // Economy queries
    /// Gets economy values of a player.
    GetPlayerEconomy = 647,
    /// Converts a cost to a float.
    CostToFloat = 678,
    /// Gets the cost of a prototype.
    GetCost = 679,
    /// Gets the population cost of a prototype.
    GetPop = 736,

    // Game timers
    /// Creates a game timer.
    CreateTimer = 658,
    /// Removes a game timer.
    DestroyTimer = 659,
    /// Pauses or resumes a game timer.
    TimerSetPaused = 781,

    // Scenario world bounds
    /// Sets the playable bounds of the map.
    SetPlayableBounds = 712,
    /// Resets the black map, so that all of the map is not explored.
    ResetBlackMap = 713,

    // Player population state
    /// Sets the population values of a player.
    SetPlayerPop = 741,

    // Rally points
    /// Sets the rally point of a player.
    RallyPointSet = 717,
    /// Clears the rally point of a player.
    RallyPointClear = 718,
    /// Gets the rally point of a player.
    RallyPointGet = 719,

    // AI force analysis
    /// Analyzes the forces in a squad list for the AI.
    AIAnalyzeSquadList = 668,
    /// Compares the offense of force A against force B.
    AIAnalyzeOffenseAToB = 669,
    /// Gets one component of an AI squad analysis.
    AISAGetComponent = 672,
    /// Analyzes the forces in a prototype squad list for the AI.
    AIAnalyzeProtoSquadList = 674,
    /// Calculates the offense ratio of force A against force B.
    AICalculateOffenseRatioAToB = 914,
    /// Copies an AI squad analysis value.
    CopyAISquadAnalysis = 921,

    // Scenario transport setup
    /// Sets the destination squad of a teleporter squad.
    SetTeleporterDestination = 967,

    // Scenario scoring
    /// Sets the score values of a scenario, for example par times and grades.
    SetScenarioScoreInfo = 984,

    // Entity presentation and fog-memory policy
    /// Makes entities flash in a color for a time.
    FlashEntity = 1000,
    /// Links tower wall endpoints.
    SetTowerWallDestination = 1001,
    /// Resets the fog-memory dopples of entities.
    ResetDopple = 1007,

    // Objective counters
    /// Adds an amount (default 1) to the counter of an objective.
    ObjectiveIncrementCounter = 935,
    /// Subtracts an amount (default 1) from the counter of an objective.
    ObjectiveDecrementCounter = 936,
    /// Gets the current counter of an objective.
    ObjectiveGetCurrentCounter = 937,
    /// Gets the final counter of an objective.
    ObjectiveGetFinalCounter = 938,

    // Late renderer/UI presentation controls
    /// Makes the game ignore or accept the D-pad of players.
    IgnoreDpad = 1034,
    /// Enables or disables the screen blur.
    EnableScreenBlur = 1037,
    /// Enables or disables the power menu of players.
    PowerMenuEnable = 1044,
    /// Sets the rotation of the minimap north pointer, in degrees.
    SetMinimapNorthPointerRotation = 1045,
    /// Hides the circle menu.
    HideCircleMenu = 1048,
    /// Enables or disables the mirror of the minimap skirt.
    SetMinimapSkirtMirroring = 1054,
    /// Locks or unlocks the user of a player.
    LockPlayerUser = 1061,

    /// Custom effect for scripting extensions (not in vanilla).
    Custom = 0xFFFF,
}
