# Trigger simulation

Scenario trigger scripts are authoritative simulation state. The `sim` crate loads them, runs them
in the fixed tick and includes them in the world checksum. The renderer does not interpret
triggers. It reads only the results that the sim puts into `World`.

The sim loads the `TriggerSystem` nodes that a scenario SCN file embeds. It also parses the
`.triggerscript` files that scripts can launch into a script library before the first tick. It
does not yet start the scripts of game modes, powers, abilities, weapons or the skirmish AI. See
[Script files and launch](#script-files-and-launch) and [Gaps](#gaps).

## Code layout

| Path (`crates/sim/src/trigger/`) | Content |
| --- | --- |
| `script.rs` | `TriggerScript`, `TriggerVar` and `ScriptType`. A script keeps its variables in a `BTreeMap` by sparse `VarId`. |
| `definition.rs` | `Trigger` and `ConditionMode` (`All` for `<And>`, `Any` for `<Or>`). |
| `condition.rs`, `effect.rs`, `effect/ids.rs` | `Condition`, `Effect` and the `ConditionType` and `EffectType` DBID enums. |
| `binding.rs` | `VarBinding` (one-based `SigID` slot plus `VarId`) and the internal `Bindings` view. |
| `value.rs`, `var_type.rs` | `TriggerValue` and `VarType`. |
| `loader.rs`, `loader/value_types.rs` | `VanillaLoader` and `TriggerLoadContext`. |
| `engine.rs` | `TriggerEngine` scheduling, script launch and the `TriggerUpdate` report. |
| `externals.rs` | `TriggerExternals`, the inputs that the launcher of a script gives (`BTriggerScriptExternals`). |
| `library.rs` | `TriggerScriptLibrary`, the parsed script files that the engine can launch. |
| `conditions.rs`, `conditions/**` | Condition handlers. `conditions/dispatch.rs` routes by `ConditionType`. |
| `effects.rs`, `effects/**` | Effect handlers. `effects/dispatch.rs` routes by `EffectType`. |
| `checksum.rs` | Hash of scripts, triggers, variables and filter sets for the world checksum. |
| `ai_analysis.rs` | `AISquadAnalysis` values that AI trigger values use. |

`crates/sim/src/scenario/triggers.rs` reads the scenario XMB and adds each script to the
`TriggerEngine` of `World`. It also fills the script library and has the setup launch API.

## Data model

- A `Trigger` keeps its conditions and its two effect lists as `Arc<[Condition]>` and
  `Arc<[Effect]>`. A clone of a trigger does not copy the lists.
- A `Condition` or an `Effect` keeps the authored DBID in `raw_type`, the `version`, and its
  `inputs` and `outputs` as `VarBinding` lists. A DBID without an enum variant loads as
  `ConditionType::Custom` or `EffectType::Custom`. `raw_type` keeps the original DBID.
- Handlers find a parameter by its `SigID` slot, not by XML child order. `Bindings` searches the
  inputs first, then the outputs. An optional parameter therefore does not move the other
  parameters.
- A variable can be explicitly null. A handler treats a null variable as an unused parameter.
  Retail sets the null flag only when it loads the script (`BTriggerScript_loadTriggerVars`
  `0x14048EAA0`). A write does not clear it, so `TriggerVar::is_null` is read-only after load.
  A null variable gets the default value of its type, not the authored text.
- Each script keeps its active triggers in activation order, its `TriggerExternals` and the
  destroyed unit and squad IDs that it must remove from its list variables.

## Loading

`VanillaLoader::from_node_with_context` parses one `TriggerSystem` node. `TriggerLoadContext`
gives it the scenario ID maps and the scenario-layered `Database`.

- Sparse variable IDs stay sparse. The loader does not renumber them.
- Runtime trigger IDs follow the authored order. Trigger-valued variables change from editor IDs
  to runtime IDs.
- Scenario Unit, Squad, Object and Entity references change to the generational `EntityId` of
  the placed entity. Unknown references become `EntityId::INVALID`.
- Prototype object, prototype squad, technology, civilization, leader, player state and
  population bucket names resolve through the layered database to retail numeric IDs.
  Relation type, tech status, squad mode, difficulty, operator, general event and command
  type names resolve through fixed retail tables. An unknown command name becomes `-1`.
- The loader accepts the retail aliases `CommandType`, `Diplomacy`, `Direction`, `LocationList`
  and `Count`. They map to `TechDataCommandType`, `RelationType`, `Vector`, `VectorList` and
  `Integer`.
- Object-type variables keep their authored names. Retail uses ordinals into `objects.xml`
  followed by `objecttypes.xml`. A name gives the same membership test without database
  ordinals.
- Placed Unit, Building and Squad prototype objects follow retail `createEntity`. Each one
  becomes a one-member squad, also when `IsSquad=false`, unless it is a physics replacement.
  Trigger variables therefore resolve to squads, as in Blood Gulch.
- An empty variable that is not null gets the default value of its authored type.
- Conditions keep `Invert` and `Async`.

The loader reads the same attributes as the retail loaders `BTrigger_load` (`0x140421890`),
`BTriggerCondition_load` (`0x140434180`) and `BTriggerEffect_load` (`0x140449E20`). Neither
reads `CommentOut`. Shipped data has 1,930 nodes with `CommentOut="true"`.

## Script files and launch

Retail `BTriggerManager_createScriptFromFile` (`0x140487520`) takes a script from the preloaded
XML files or reads `data\triggerscripts\<name>`. The sim must not read files in the tick, so
the scenario loader parses the files first:

1. `load_scenario_from_game_dir` adds the `TriggerScripts` entries of `preloadxmlfiles.xml`.
2. It adds each file name that a string variable of a resident script holds (for example
   `CrateMaker3.triggerscript`), and then the names in those files, until no new name occurs.
3. `TriggerScriptLibrary` keeps each parsed script by its lowercase file name. A file with a
   `TriggerSystem` root that does not parse stays in the library as invalid.

`TriggerEngine::launch_script(name, externals, time)` follows `createScriptFromFile`, the
`setExternal*` calls and `BTriggerManager_activateScript` (`0x140487800`):

- A file that is not in the library gives `None` and uses no script ID.
- An invalid file uses one script ID, like a retail `BTriggerScript_load` failure.
- Otherwise the copy gets the next script ID, the file name and the externals. Its start-active
  triggers become active in trigger order.

Setup code uses `LoadedScenario::launch_trigger_script`. It parses the file and the files that
it names through the scenario asset source, then launches it. `LaunchScript` (392) v4 returns a
launch request to the engine. Script IDs start at 0 and follow creation order, like the retail
`nextTriggerScriptID`.

## Scheduling

The update order follows retail `BTriggerManager_update` (`0x1404898C0`) and
`BTriggerScript_update` (`0x140490E40`):

1. Commands, production and entity updates run for the fixed tick.
2. `TriggerEngine` visits the scripts in runtime-ID order, which is creation order. The list of
   scripts is fixed at the start, so a script that an effect launches first runs in the next
   update.
3. Each script removes the destroyed units and squads from its list variables. It resets the
   evaluation count of each trigger. It copies its active triggers into a FIFO queue.
4. `BTriggerScript_activateTrigger` (`0x14048A8B0`) adds a trigger to the end of the active list
   and to the queue, once. The order of the active list is therefore the activation order, and
   the next update evaluates the triggers in that order. An activation also sets the activation
   time when the trigger is already active.
5. An evaluation sets the next evaluation time and increments the evaluation count. `All` and
   `Any` stop at the first result that decides the tree, in authored order. An empty tree is
   true.
6. A true trigger, or a conditional trigger with a false result, deactivates before its effects
   run. `BTriggerScript_deactivateTrigger` (`0x14048AA10`) removes it from the active list and
   the queue. An effect can activate the same trigger again in the same tick.
7. `Shutdown` (56) clears the active list and the queue. The other effects of the list still
   run.
8. The engine removes a script that has no active triggers, or that ran `Shutdown`, after its
   update.

When the world removes a unit or squad, each active script records the ID
(`BTriggerScript_invalidateEntityID` `0x14048AB10`). The script removes the first copy of the ID
from each non-null `UnitList` or `SquadList` variable at the start of its next update. If an
effect of the running script removed the entity, that script removes the ID at once. A script
that starts later does not get the ID.

`EvalLimit` limits how many times a trigger evaluates in one tick. The engine also stops the
update after 50,000 evaluations and sets `TriggerUpdate::infinite_loop_guard_reached`.

## Results and reporting

`TriggerEngine::update` returns a `TriggerUpdate`:

- `effects_applied` counts effects that changed sim state.
- `presentation_effects` counts effects that only add data for the UI or audio.
- `effects_skipped` and `skipped_effect_types` count effects with an invalid binding or target.
- `unsupported_condition_types` and `unsupported_effect_types` list each DBID that has no handler,
  or that uses a version or path that the sim does not model. These lists are sorted and unique.
- `launched_scripts` lists the scripts that `LaunchScript` started. `missing_script_files` lists
  the file names that the library does not have.

An unsupported condition does not pass. An unsupported effect does not change state. An async
condition always returns `ConditionResult::Waiting`.

The world checksum includes trigger definitions, variables, the ordered active lists, externals,
pending entity removals, evaluation times, player end states, queued player state changes,
co-op and configuration settings, difficulty scalars, teleporter links, general-event
subscribers, pending presentation requests, and unit and squad idle state.

## Sim and UI boundary

- Chat and cinematic effects (DBIDs 729 and 480) add typed requests to `World`. The UI reads
  `World::presentation_requests()`. When playback ends, it sends `acknowledge_presentation` as
  synchronized input. The sim then fires the completion event.
- Headless runs do not complete these requests.
- `PlaySound` and the `DebugVar` effects are presentation effects.
- `ShowMessage` (904) stores the last message of each target player. The UI reads
  `World::trigger_message`.
- `SetPlayerState` (130) queues a `PlayerStateChange` with the retail world event (53 resigned,
  54 defeated, 56 won). The game layer reads `World::take_player_state_changes`. Each event
  checks for game over at once (`BWorld_checkForGameOver`); the client reads
  `World::game_result`. After the game ended, the trigger manager does not update again.

## Retail behavior that the sim keeps

This section lists retail details that are easy to get wrong. The handler modules have the full
logic.

### Conditions

- `PlayerInState` (131) v2 compares the live player state with the resolved `PlayerState`.
- `CompareCiv` (240) and `CompareLeader` (476) use all six operators of
  `BTriggerCondition_compareIntValues` (`0x140431AD0`). `CompareLeader` compares the low bytes of
  the leader IDs as signed bytes.
- `CanRetrieveExternals` (192) v2 and v3 copy each requested external (player, power, squad, squad
  list, unit, unit list, cost) into its non-null output. A requested external that the launcher
  did not set gives false, but the other outputs still get their values.
  `CanRetrieveExternalLocation` (440) and `CanRetrieveExternalFloat` (977) need their flag.
- `CanRemoveOneLocation` (566) and `CanRemoveOneFloat` (975) read the list and the position
  without a null check. `First` and `Last` keep the order of the other values. `Random` takes
  an index from the trigger random stream and moves the last value into the free slot.
- `CheckDiplomacy` (362) collects the teams of all used entity inputs. A used input with only
  stale handles gives an empty set, and an empty set matches. An invalid used player does not
  fall back to the team input.
- `IsSquadAtMaxSize` (438) compares the live child count with the proto-squad capacity.
- `CheckDifficulty` (972) v1 tests equality. v2 compares the difficulty category that the
  layered thresholds give.
- `CompareCost` (339) uses OR across resource slots if slot 3 is true. Otherwise it uses AND.
- `CheckResourceTotals` (340) reads the lifetime resource total, not the balance.
- `TechStatus` (5) v1 and v2 read the research state. An invalid tech ID compares as
  `Unobtainable`. v2 returns false for per-unit `UniqueProtoUnitInstance` technologies, because
  the sim does not model unique technology ownership.
- `CompareAmmoPercent` (256) v2 writes the ratio output. A stale unit reads zero.
- `IsUnderAttack` (461) v1 and v2 pass when the last damage time is in the authored interval.
- `IsIdle` (264) tests the explicit idle action. A used valid unit takes precedence over the
  squad.
- `ComparePlayerUnitCount` (436) v2 never adds units in training. Retail
  `BTriggerCondition_tcComparePlayerUnitCountV2` (`0x140429880`) reads slot 0 instead of slot 5
  for `IncludeTraining`. The sim treats slot 0 as absent.
- `ComparePlayerSquadCount` (606) adds queued `TrainSquad` work when slot 5 is true.
- `CanGetUnits` (2) supports v5 and v6. `CanGetSquads` (4) supports v8 and v9. Other versions
  return false, as in retail `BTriggerCondition_evaluate` (`0x140432070`).
- `NextUnit` (140) and `NextSquad` (141) visit the current list order and mark each visited ID.

### Effects

- `PayCost` (50) pays only if the player can pay all slots. `RefundCost` (51) changes only the
  balance.
- `SetResources` and `GetResources` (277 and 278) use the balance. `SetResourcesTotals` and
  `GetResourcesTotals` (337 and 338) use the lifetime totals.
- `TechActivate` and `TechDeactivate` (60 and 61) use the normal technology path.
- `GetAmmo` and `SetAmmo` (389 and 390): v1 reads a unit, v2 adds a squad. A used unit takes
  precedence. An amount takes precedence over a percentage.
- `CreateObject` (35) v5 and v6, `CreateSquad` (36) v6 and v7, `CreateSquads` (875) and
  `CreateUnit` (154) v1 and v2 create database-backed entities. Authored fly-in creates the
  civilization transport. If the player has no transport, the squad appears on the ground.
- `Kill` and `Destroy` (37 and 38) v3 and v4: a kill leaves the dead state visible until the
  next entity update. Destroy removes the entity at once. A kill of a `_HeroDeath` unit leaves it
  at 1 HP with the `Down` flag. The sim keeps no per-kill score, so the `PreventScoring` input
  has no effect.
- `Repair` and `Damage` (318 and 325) set hit points directly. `CombatDamage` (336) uses the
  normal shield-first damage path.
- `ChangeOwner` (137) v3 moves the squad, its units, population and base anchor together.
- `SetDirection` (489) v2 with turret aiming, and `Teleport` (354) of a live squad without
  `IgnorePlot`, are reported as unsupported before any change. The sim has no hardpoint
  orientation and no `BSquadPlotter`.
- `Move` (66) v6 moves squads to a unit, a squad or a location, in that order of precedence. It
  supports attack-move and queued orders.
- `Work` (117) v3 and v4 select the work action through the tactic data of the squad: attack,
  garrison, join, gather, capture, repair, move, hitch, unhitch or mines. If no tactic action
  matches, the squad tries repair, capture and gather, then moves to the target. v4 `DoAbility`
  uses the command ability. The sim resolves all squads first. If one squad gets an action type
  that the sim does not map, or a queued order is not a move, the effect is unsupported and no
  squad changes.
- `Unload` (65) v3 and v4 send passengers through the normal ungarrison path.
- `CarpetBomb` (71) v3 and v4 start the `BSquadActionCarpetBomb` attack run. No shipped
  scenario uses DBID 71. Synthetic tests cover it.
- List effects (146 to 151, 296, 297, 302, 308, 334, 335 and 836) keep the retail order of
  mutation, also when an input is the same variable as the output. `SquadListShuffle` and
  `UnitListShuffle` (297 and 302) use `cSimRand` (the MSVC `214013/2531011` LCG). They swap each
  position with a random index in the full list. They do not use Fisher-Yates.
- Entity filter effects (341 to 350, 355, 379) append ordered predicates. Each predicate applies
  its inversion after the base test.
- `SetTeleporter` (967) links a source teleporter squad to its target. See
  [Garrison and teleporters](garrison-and-teleporters.md).
- General-event effects (811, 812, 837 to 840, 988, 989, 1018) use the retail subscriber ID
  layout `(index << 16) + eventType`.
- The copy effects (85 to 103 and the others in `is_copy_effect`) copy the value without a null
  check, as in retail case `0x55`.
- `SetPlayerState` (130) v2 follows `BPlayer_setPlayerState` (`0x1402F0610`): `Disconnected`
  while playing becomes `Resigned`, the first change away from `Playing` stores
  `(time + 500) / 1000` seconds, and `Resigned` or `Defeated` marks the units for cleanup.
- `GetPlayers` (173) v2 lists the players in ID order. Slot 2 includes Gaia, and a non-null slot 4
  filters the state. `GetNPCPlayersByName` (943) keeps NPC players whose name matches without
  case. `PlayerListGetSize` (970) writes the list size.
- `TimerSetPaused` (781) sets the paused flag of the timer with the given ID.
- `LaunchScript` (392) v4 sets each external from a non-null input: 2 unit, 3 unit list, 4 squad,
  5 squad list, 6 player, 7 power, 8 cost, 11 location, 12 location list, 13 float. Power
  variables load as strings, so only an integer power value becomes an external.
- `CanGetDesignSpheres` (974, `0x140430C50`) reads the scenario design spheres
  (`BDesignObjectManager_load` `0x140200530`, kept in `World::design_spheres`). It matches the
  type without case, or every sphere if the type is null, and always overwrites the center and
  radius lists. The `Creeps` and `CrateMaker` scripts that `SkirmishWorld` launches still need
  `CheckPlacement` (82), `CanGetRandomLocation` (956), `MarkerSquadsInArea` (980),
  `RandomLocation` (55) v4 with obstruction tests, `GetDistanceLocationLocation` (512) and
  `SetLevel` (820) (issue #24).

## Coverage

The coverage data comes from three sources:

- Engine: the effect switch in `BTriggerEffect_fire` (`0x14044A0A0`, jump table at
  `0x14044A0ED`) has slots for DBIDs 31 to 1086. 588 DBIDs have code. 468 DBIDs jump to the
  shared no-op block `0x14044DDD7`. `BTriggerCondition_evaluate` (`0x140432070`) has 188 DBIDs
  in its switches and direct tests. It also tests 1059, 1081 and 1087 after the switches. Async
  conditions read their async result there.
- Our code: `EffectType` has 344 variants and `ConditionType` has 195. Handlers exist for 248
  effect DBIDs (including the copy family in `is_copy_effect`) and 89 condition DBIDs.
- Shipped data: the 33 installed scenarios (15 campaign missions, 2 tutorials, 16 skirmish maps)
  and the 90 unique files in `data/triggerscripts` of `root.era`, `root_update.era`, `dlc01.era`
  and `dlc02.era`. The counts include nodes with `CommentOut="true"`.

| Kind | Distinct DBIDs used | Implemented | Retail no-op | Missing | Scenario uses covered | External uses covered |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Effects | 471 (315 in scenarios) | 223 (202) | 26 | 227 (102) | 66,422 of 72,757 (91%) | 23,647 of 26,345 (90%) |
| Conditions | 148 (79 in scenarios) | 81 (63) | 0 | 67 (16) | 16,638 of 17,177 (97%) | 6,459 of 7,257 (89%) |

"Implemented" includes 5 `DebugVar` effects that retail compiles as no-ops. The other retail
no-ops are the other `DebugVar` effects, `KillUnits` (271), `AIRegisterHook` (855),
`PlaySoundFile` (1084), `ConnectHitpointWidget` (638), `Breakpoint` (614) and `AIGetMemory`
(957). They are not gaps.

The "covered" columns count uses of implemented DBIDs. A handler can still report a version or a
path as unsupported. See [Partial handlers](#partial-handlers).

### Missing effects used by scenarios

The table ranks the effects by the number of uses in scenarios. The number in parentheses is the
number of scenarios. "External uses" counts uses in `data/triggerscripts`. An "inline case" is
code in `BTriggerEffect_fire` without a separate function.

| DBID | Authored name | Scenario uses (scenarios) | External uses | Engine evidence |
| ---: | --- | ---: | ---: | --- |
| 556 | `AIMissionRemoveSquads` | 887 (13) | 52 | `BTriggerEffect_teAIMissionRemoveSquads` @ `0x140481A20` |
| 557 | `AIMissionGetValidSquads` | 601 (13) | 54 | inline case @ `0x14044C508` |
| 550 | `AIMissionCancel` | 577 (13) | 46 | `BTriggerEffect_teAIMissionCancel` @ `0x140480A30` |
| 555 | `AIMissionAddSquads` | 339 (13) | 36 | `BTriggerEffect_teAIMissionAddSquads` @ `0x140481980` |
| 133 | `ObjectiveComplete` | 238 (17) | 0 | inline case @ `0x14044A943` |
| 183 | `ObjectiveDisplay` | 224 (17) | 2 | inline case @ `0x14044ABDA` |
| 135 | `FlareMinimapNormal` | 193 (15) | 18 | inline case @ `0x14044A99C` |
| 1008 | `AICreateTargetWrapper` | 189 (13) | 28 | `BTriggerEffect_teAICreateTargetWrapper` @ `0x140486B50` |
| 1011 | `AIWrapperModifyRadius` | 189 (13) | 28 | `BTriggerEffect_teAIWrapperModifyRadius` @ `0x140486C60` |
| 1012 | `AIWrapperModifyFlags` | 189 (13) | 28 | `BTriggerEffect_teAIWrapperModifyFlags` @ `0x140486D70` |
| 1013 | `AIWrapperModifyParms` | 189 (13) | 28 | `BTriggerEffect_teAIWrapperModifyParms` @ `0x140486E40` |
| 1005 | `AIMissionCreate` | 189 (13) | 20 | `BTriggerEffect_teAIMissionCreate` @ `0x140486A00` |
| 982 | `AIMissionSetMoveAttack` | 189 (13) | 17 | `BTriggerEffect_teAIMissionSetMoveAttack` @ `0x140486460` |
| 805 | `HintMessageShow` | 188 (15) | 35 | `BTriggerEffect_teHintMessageShow` @ `0x140472EA0` |
| 1004 | `AICreateAreaTarget` | 184 (13) | 28 | `BTriggerEffect_teAICreateAreaTarget` @ `0x140486900` |
| 605 | `AIGetMissions` | 155 (11) | 17 | `BTriggerEffect_teAIGetMissions` @ `0x140481C00` |
| 815 | `HintMessageDestroy` | 151 (15) | 55 | `BTriggerEffect_teHintMessageDestroy` @ `0x140473960` |
| 388 | `ChangeSquadMode` | 82 (6) | 14 | inline case @ `0x14044BA68` |
| 1002 | `MissionResult` | 68 (17) | 0 | `BTriggerEffect_teMissionResult` @ `0x14047D510` |
| 888 | `FloatListRemove` | 63 (5) | 0 | `BTriggerEffect_teFloatListRemove` @ `0x140478560` |
| 59 | `LocationAdjust` | 60 (7) | 50 | inline case @ `0x14044A42F` |
| 923 | `FlashUIElement` | 60 (3) | 26 | `BTriggerEffect_teFlashUIElement` @ `0x14047AFF0` |
| 671 | `CopyObjectList` | 56 (3) | 0 | inline case @ `0x14044CAC2` |
| 619 | `AIMissionGetTarget` | 53 (7) | 10 | `BTriggerEffect_teAIMissionGetTarget` @ `0x140482460` |
| 757 | `AIMissionTargetGetLocation` | 48 (7) | 28 | `BTriggerEffect_teAIMissionTargetGetLocation` @ `0x140482BD0` |
| 939 | `GetClosestUnit` | 42 (6) | 8 | `BTriggerEffect_teGetClosestUnit` @ `0x14047BD00` |
| 886 | `CopyFloatList` | 42 (5) | 0 | `BTriggerEffect_teCopyFloatList` @ `0x140487470` |
| 924 | `ConceptPermission` | 40 (4) | 6 | `BTriggerEffect_teConceptPermission` @ `0x14047B5E0` |
| 1006 | `EnableMusicManager` | 36 (9) | 0 | `BTriggerEffect_teEnableMusicManager` @ `0x14047C470` |
| 132 | `FlareMinimapSpoof` | 36 (3) | 3 | inline case @ `0x14044A917` |
| 631 | `GetProtoObject` | 35 (8) | 11 | inline case @ `0x14044C82E` |
| 515 | `PlayWorldSoundOnEntity` | 34 (2) | 0 | inline case @ `0x14044C1F8` |
| 1060 | `TeleportUnitsOffObstruction` | 34 (4) | 0 | `BTriggerEffect_teTeleportUnitsOffObstruction` @ `0x14047E9F0` |
| 751 | `TimeToFloat` | 33 (5) | 4 | `BTriggerEffect_teTimeToFloat` @ `0x140470880` |
| 512 | `GetDistanceLocationLocation` | 30 (6) | 64 | inline case @ `0x14044C1AA` |
| 1030 | `GrantAchievement` | 30 (15) | 6 | `BTriggerEffect_teGrantAchievement` @ `0x14047FF40` |
| 537 | `SetPosition` | 29 (4) | 18 | inline case @ `0x14044C3F8` |
| 284 | `InvertBool` | 29 (4) | 2 | inline case @ `0x14044B143` |
| 316 | `RefCountSquadAdd` | 28 (2) | 88 | `BTriggerEffect_teRefCountSquadAdd` @ `0x14045F520` |
| 845 | `ClearCorpseUnits` | 28 (3) | 0 | `BTriggerEffect_teClearCorpseUnits` @ `0x140474CF0` |
| 1070 | `FilterAddJacking` | 28 (3) | 0 | inline case @ `0x14044B5F1` |
| 968 | `SetGarrisonedCount` | 26 (1) | 18 | `BTriggerEffect_teSetGarrisonedCount` @ `0x14047C4A0` |
| 1068 | `PatherObstructionUpdates` | 24 (1) | 0 | `BTriggerEffect_tePatherObstructionUpdates` @ `0x140461550` |
| 874 | `GetObstructionRadius` | 23 (8) | 4 | `BTriggerEffect_teGetObstructionRadius` @ `0x1404756C0` |
| 887 | `FloatListAdd` | 21 (5) | 0 | `BTriggerEffect_teFloatListAdd` @ `0x140478370` |
| 820 | `SetLevel` | 14 (4) | 22 | `BTriggerEffect_teSetLevel` @ `0x140473A00` |
| 83 | `AttachmentRemoveType` | 14 (3) | 16 | inline case @ `0x14044A66C` |
| 160 | `PlayAnimationSquad` | 12 (3) | 0 | inline case @ `0x14044AA56` |
| 780 | `TimerGet` | 12 (1) | 0 | `BTriggerEffect_teTimerGet` @ `0x140472950` |
| 1069 | `PatherObstructionRebuild` | 12 (1) | 0 | `BTriggerEffect_tePatherObstructionRebuild` @ `0x140461570` |
| 739 | `PowerInvoke` | 11 (4) | 0 | inline case @ `0x14044D323` |
| 382 | `SetIgnoreUserInput` | 10 (2) | 8 | inline case @ `0x14044B794` |
| 159 | `PlayAnimationUnit` | 10 (2) | 5 | inline case @ `0x14044AA3F` |
| 910 | `SquadFlagSet` | 10 (3) | 4 | `BTriggerEffect_teSquadFlagSet` @ `0x14047ABA0` |
| 1032 | `ResetAbilityTimer` | 10 (3) | 0 | `BTriggerEffect_teResetAbilityTimer` @ `0x140480010` |
| 554 | `LocationAdjustDir` | 9 (2) | 23 | `BTriggerEffect_teLocationAdjustDir` @ `0x14046A9E0` |
| 934 | `ConceptSetParameters` | 9 (2) | 8 | `BTriggerEffect_teConceptSetParameters` @ `0x14047B8E0` |
| 498 | `AsInt` | 8 (2) | 43 | inline case @ `0x14044C09B` |
| 767 | `GetGarrisonedSquads` | 8 (1) | 14 | `BTriggerEffect_teGetGarrisonedSquads` @ `0x140470990` |
| 319 | `UnitFlagSet` | 8 (2) | 2 | `BTriggerEffect_teUnitFlagSet` @ `0x14045FC50` |
| 738 | `MegaTurretAttack` | 8 (3) | 2 | `BTriggerEffect_teMegaTurretAttack` @ `0x14046D130` |
| 180 | `MathHitpoints` | 8 (1) | 0 | inline case @ `0x14044B189` |
| 779 | `TimerSet` | 8 (1) | 0 | `BTriggerEffect_teTimerSet` @ `0x1404728A0` |
| 1031 | `AIAddTeleporterZone` | 8 (1) | 0 | `BTriggerEffect_teAIAddTeleporterZone` @ `0x140486EF0` |
| 505 | `GetLegalSquads` | 7 (6) | 12 | `BTriggerEffect_teGetLegalSquads` @ `0x140468F30` |
| 506 | `GetLegalTechs` | 7 (6) | 4 | `BTriggerEffect_teGetLegalTechs` @ `0x140468FE0` |
| 459 | `GetTechResearcherType` | 7 (6) | 1 | inline case @ `0x14044BE4D` |
| 279 | `MathResources` | 7 (6) | 0 | inline case @ `0x14044B0F8` |
| 458 | `GetSquadTrainerType` | 7 (6) | 0 | inline case @ `0x14044BDFB` |
| 726 | `UITogglePowerOverlay` | 6 (2) | 38 | inline case @ `0x14044D1EF` |
| 765 | `EventFilterGameState` | 6 (2) | 30 | `BTriggerEffect_teEventFilterGameState` @ `0x140474EA0` |
| 595 | `CopyObject` | 6 (1) | 0 | inline case @ `0x14044CEC2` |
| 979 | `LocationListGetByIndex` | 6 (1) | 0 | `BTriggerEffect_teLocationListGetByIndex` @ `0x14047C6E0` |
| 983 | `Infect` | 6 (1) | 0 | `BTriggerEffect_teInfect` @ `0x14047CC50` |
| 164 | `UserMessage` | 5 (2) | 122 | inline case @ `0x14044A985` |
| 544 | `AIScoreMissionTargets` | 5 (3) | 22 | `BTriggerEffect_teAIScoreMissionTargets` @ `0x140480900` |
| 564 | `AISetScoringParms` | 5 (3) | 18 | `BTriggerEffect_teAISetScoringParms` @ `0x140481570` |
| 551 | `AIGetMissionTargets` | 5 (3) | 17 | inline case @ `0x14044C465` |
| 545 | `AISortMissionTargets` | 5 (3) | 16 | `BTriggerEffect_teAISortMissionTargets` @ `0x1404809B0` |
| 311 | `RefCountUnitAdd` | 5 (2) | 10 | `BTriggerEffect_teRefCountUnitAdd` @ `0x14045EFE0` |
| 312 | `RefCountUnitRemove` | 5 (2) | 2 | `BTriggerEffect_teRefCountUnitRemove` @ `0x14045F230` |
| 853 | `GetClosestSquad` | 4 (1) | 20 | `BTriggerEffect_teGetClosestSquad` @ `0x140475240` |
| 828 | `EventFilterNumeric` | 4 (1) | 6 | `BTriggerEffect_teEventFilterNumeric` @ `0x1404750A0` |
| 190 | `MathPercent` | 4 (2) | 0 | inline case @ `0x14044B6A0` |
| 533 | `LightsetAnimate` | 4 (1) | 0 | `BTriggerEffect_teLightsetAnimate` @ `0x14046A880` |
| 737 | `PowerClear` | 4 (2) | 0 | `BTriggerEffect_tePowerClear` @ `0x14046F890` |
| 808 | `HintGlowToggle` | 4 (2) | 0 | `BTriggerEffect_teHintGlowToggle` @ `0x140473160` |
| 822 | `GetLevel` | 4 (2) | 0 | `BTriggerEffect_teGetLevel` @ `0x140473AD0` |
| 981 | `LocationListGetClosest` | 4 (1) | 0 | `BTriggerEffect_teLocationListGetClosest` @ `0x14047C820` |
| 1049 | `ChatForceSubtitles` | 4 (2) | 0 | `BTriggerEffect_teChatForceSubtitles` @ `0x1404803F0` |
| 57 | `LaunchProjectile` | 2 (2) | 38 | inline case @ `0x14044A3AC` |
| 393 | `GetPlayerTeam` | 2 (1) | 14 | inline case @ `0x14044BB2F` |
| 359 | `Settle` | 2 (1) | 6 | `BTriggerEffect_teSettle` @ `0x1404627D0` |
| 775 | `SetOverrideTint` | 2 (1) | 6 | `BTriggerEffect_teSetOverrideTint` @ `0x140471E00` |
| 891 | `RepairByCombatValue` | 2 (2) | 5 | `BTriggerEffect_teRepairByCombatValue` @ `0x140478710` |
| 710 | `SetUnitAttackTarget` | 2 (1) | 0 | `BTriggerEffect_teSetUnitAttackTarget` @ `0x14046EBA0` |
| 941 | `TeamSetDiplomacy` | 2 (1) | 0 | `BTriggerEffect_teTeamSetDiplomacy` @ `0x14047B060` |
| 1022 | `PowerUserShutdown` | 2 (1) | 0 | `BTriggerEffect_tePowerUserShutdown` @ `0x14047E090` |
| 1027 | `ChangeControlledPlayer` | 2 (1) | 0 | `BTriggerEffect_teChangeControlledPlayer` @ `0x14047FDC0` |
| 294 | `LerpLocation` | 1 (1) | 44 | inline case @ `0x14044B1F8` |
| 186 | `CalculatePercentTime` | 1 (1) | 2 | `BTriggerEffect_teCalculatePercentTime` @ `0x14045CA80` |
| 521 | `SetFollowCam` | 1 (1) | 0 | `BTriggerEffect_teSetFollowCam` @ `0x14046A060` |

Retail sends `CopyObject` (595), `IntToCount` (568), 567 and 700 to the same copy block
(`0x14044CEC2`) as the copy effects 85 to 103. `is_copy_effect` in `effects.rs` does not include
them.

### Missing conditions used by scenarios

| DBID | Authored name | Scenario uses (scenarios) | External uses | Engine evidence |
| ---: | --- | ---: | ---: | --- |
| 575 | `CompareProtoObject` | 168 (8) | 101 | inline case @ `0x140432242` |
| 547 | `CompareAIMissionState` | 78 (10) | 28 | inline case @ `0x140432C5F` |
| 956 | `CanGetRandomLocation` | 59 (13) | 1 | `BTriggerCondition_tcCanGetRandomLocation` @ `0x140430600` |
| 835 | `CanGetOneObject` | 56 (3) | 0 | `BTriggerCondition_tcCanGetOneObject` @ `0x14042EB50` |
| 885 | `CanGetOneFloat` | 42 (5) | 0 | `BTriggerCondition_tcCanGetOneFloat` @ `0x14042EFA0` |
| 763 | `CheckPop` | 36 (7) | 11 | `BTriggerCondition_tcCheckPop` @ `0x14042DD40` |
| 844 | `CanGetCorpseUnits` | 28 (3) | 0 | `BTriggerCondition_tcCanGetCorpseUnits` @ `0x14042EC70` |
| 386 | `CheckModeChange` | 25 (3) | 4 | `BTriggerCondition_tcCheckModeChange` @ `0x1404294F0` |
| 576 | `CompareTech` | 21 (6) | 4 | inline case @ `0x140432242` |
| 604 | `IsAttached` | 10 (1) | 0 | inline case @ `0x140432F57` |
| 314 | `RefCountUnit` | 5 (2) | 36 | `BTriggerCondition_tcRefCountUnit` @ `0x1404281F0` |
| 558 | `CanGetUnitsAlongRay` | 4 (1) | 0 | inline case @ `0x140432CF6` |
| 714 | `CanGetUnitLaunchLocation` | 2 (1) | 0 | `BTriggerCondition_tcCanGetUnitLaunchLocation` @ `0x14042D0D0` |
| 743 | `CanGetGreatestThreat` | 2 (1) | 0 | `BTriggerCondition_tcCanGetGreatestThreat` @ `0x14042D600` |
| 1028 | `CompareUnit` | 2 (1) | 0 | inline case @ `0x1404336BD` |
| 20 | `ASYNCPlayerSelectingUnit` | 1 (1) | 0 | async result in `BTriggerCondition_evaluate` |

### Missing DBIDs used only by external scripts

These DBIDs occur only in `data/triggerscripts`. They become important when the sim loads those
scripts.

Effects (125 DBIDs):

- AI: 32 DBIDs, 776 uses. `AIFactoidSubmit` (993), `AIRemoveFromMissions` (612),
  `AITopicLotto` (625), `AIAnalyzeKBSquadList` (673), `AISetPlayerAssetModifier` (1035),
  `AISetFocus` (711), `AITopicModifyTickets` (591), `AITopicCreate` (589), and 24 more.
- AI bids: 14 DBIDs, 345 uses. `BidQuery` (778), `BidDelete` (373), `BidSetQueueLimits` (871),
  `BidCreatePower` (958), `BidCreateBuilding` (370), `BidCreateTech` (371), `BidPurchase` (391),
  `BidCreateSquad` (372), and 6 more.
- AI knowledge base: 11 DBIDs, 272 uses. `KBSQInit` (663), `KBSQExecute` (665),
  `KBSquadListFilter` (691), `KBBQExecute` (449), `KBAddSquadsToKB` (985), and 6 more.
- Hint concepts: 5 DBIDs, 90 uses. `ConceptSetState` (925), `ConceptSetPrecondition` (926),
  `ConceptGetParameters` (849), `ConceptStartSub` (927), `ConceptResetCooldown` (971).
- UI input: 6 DBIDs, 84 uses. `UIUnlock` (330), `InputUILocation` (84), `InputUIButton` (529),
  `InputUISquadList` (639), `InputUIPlaceSquads` (771), `InputUILocationMinigame` (892).
- DLC game modes: 7 DBIDs, 74 uses. `TugOfWarMarkPlayersAsTrailing` (1071), `setCTFCount`
  (1083), `GetCTFFlag` (1074), `GetCTFFlagCarrier` (1075), `TowSetVisible` (1079), `TowSetScore`
  (1080), `ClearCTFFlagOrCarrier` (1082).
- Other: 50 DBIDs, 751 uses. `ModifyProtoSquadData` (829), `PowerChargeUseOf` (77),
  `AttachmentAddObject` (363), `AsTime` (716), `GetDistanceUnitLocation` (111),
  `RefCountSquadRemove` (317), `MathLocation` (295), `TableLoad` (796), and 42 more.

Conditions (51 DBIDs):

- UI input: 13 DBIDs, 77 uses. The `UILocation*`, `UISquadList*` and `UILocationMinigame*`
  result conditions, and `UIButtonPressed` (527).
- AI: 5 DBIDs, 132 uses. `AICanGetDifficultySetting` (920), `AITopicIsActive` (600),
  `AITopicGetTickets` (1059), `AICanGetFocusTopic` (883), `BidState` (586).
- Hint concepts: `ConceptGetCommand` (930), `ConceptGetStateChange` (931).
- Other: 31 DBIDs, 636 uses. `CheckAndSetFalse` (1056), `CanRemoveOneInteger` (563),
  `RefCountSquad` (315), `CanUsePower` (27), `ProtoObjectListContains` (577),
  `CanGetHoverPoint` (990), and 25 more.

### Partial handlers

These handlers exist but do not cover all shipped uses:

- `SetCamera` (884) accepts only v4. Retail `BTriggerEffect_teSetCamera` (`0x140477280`) also
  handles v2 and v3. Shipped data has 11 v3 uses.
- `CreateObject` (35) v4, `ChangeOwner` (137) v2 and `MovePath` (439) v1 are reported as
  unsupported. Retail does nothing for these versions, so the state is the same.
- `SetDirection` (489) turret aiming and `Teleport` (354) squad plotting are unsupported. Object
  teleports do not search for a free position when the destination is obstructed.
- `Work` (117) does not queue orders other than a move. It does not map all tactic action types,
  for example `Mines` without the command ability.
- `TechStatus` (5) v2 returns false for `UniqueProtoUnitInstance` technologies.
- Async conditions always wait. The sim has no async consensus and no UI input delivery.

## Gaps

In order of shipped use:

1. AI mission effects and conditions (DBIDs 550 to 557, 605, 619, 757, 982, 1004 to 1013, 547).
   13 campaign missions use them. The sim has no trigger AI mission system.
2. Objectives and mission result: `ObjectiveComplete` (133), `ObjectiveDisplay` (183) and
   `MissionResult` (1002). All 17 campaign scenarios use them. A campaign cannot end without
   them. `SetPlayerState` (130) ends a skirmish (`World::game_result`), but the AI manager does
   not learn that a player left (`BAIManager_onPlayerLeft`).
3. External trigger script starts. `TriggerEngine::launch_script` exists, but nothing calls it
   for the game-mode world and player scripts (`BScenario_load`), the skirmish AI script
   (`BAI_startSkirmishAIScript` `0x14014E4D0`), `ExternalTriggerScript` in `BScenario_load`
   (`0x14037A616`), the power scripts of `BProtoPower_load` (`0x14034EB96`) and the weapon
   scripts of `BWeapon_loadWeapon` (`0x1403FFE52`).
4. Hints and UI: `HintMessageShow` (805), `HintMessageDestroy` (815), `FlareMinimapNormal`
   (135), `FlashUIElement` (923), `ConceptPermission` (924) and the UI input effects and
   conditions.
5. The other rows of the tables above.

Not gaps: the retail no-op DBIDs listed in [Coverage](#coverage). The DLC game modes (capture
the flag, tug of war) use only external scripts.

## Audit tool

`scenario-trigger-audit` prints the DBIDs that each scenario uses and the result of the first
trigger update:

```sh
cargo run -j 4 -p sim --bin scenario-trigger-audit -- "$OPENENSEMBLE_GAME_DIR" PHXscn01 blood_gulch
```

Set these environment variables for more output:

- `OPENENSEMBLE_AUDIT_DBIDS`: a comma-separated list of DBIDs. The tool prints their bindings
  before and after the update.
- `OPENENSEMBLE_AUDIT_SCENARIO_IDS`: scenario object IDs to print.
- `OPENENSEMBLE_AUDIT_OBJECTIVES`, `OPENENSEMBLE_AUDIT_RAW_VARIABLES`,
  `OPENENSEMBLE_AUDIT_ENTITIES`, `OPENENSEMBLE_AUDIT_PRESENTATION`: print objectives, raw
  variables, entities or presentation state.

The first update reaches only part of each script. Use the tables in [Coverage](#coverage) for
the full list of gaps.

The ignored test `crates/sim/tests/scenario-campaign-trigger-loading.rs` loads all 15 campaign
missions and both tutorials. It checks the variable types and that the first update ends.
