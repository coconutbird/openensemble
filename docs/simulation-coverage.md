# Simulation coverage

This document compares the systems of the retail engine (`xgameFinal.exe`) with the `sim` crate. It
does not cover trigger effects and conditions; see `triggers.md` for those.

Status values:

- **Implemented**: the sim runs the mechanic from the layered game data.
- **Partial**: the sim runs a subset. The notes give the missing part.
- **Missing**: the sim has no code for the mechanic.

Engine evidence is an IDA function name or a string address. The engine has no RTTI for game
classes. The action class names come from the save-game type strings near `0x141193338`. All paths
are below `crates/sim/src/` unless the path starts with `crates/` or `docs/`.

## Commands

The packet dispatcher (`dispatcher.rs`) decodes the five command packets: Work, Power, PowerInput,
Building and Game. `executor.rs` applies them to the `World`. The wire formats are complete. The
gaps are in the execution.

Recipient gates follow `BCommandManager_processCommands` @ `0x140195790`. A command with unit
recipients (all Building commands) reaches a unit of the command player, of Gaia, or, in a co-op
game, of a human player on the same team as the human command player (`BCommand_processUnits` @
`0x140194560`, `World::command_reaches_owner`). A command with squad or army recipients (all Work
commands) reaches a squad of the command player or of Gaia, and no co-op teammate squad
(`BCommand_processSquads` @ `0x1401946D0`, `BCommand_processArmies` @ `0x1401947B0`,
`World::command_reaches_squad_owner`). The executor filters Building recipients, and the Work
order entry points (move, attack, gather, capture, repair other, cloak, detonate, change mode,
join, garrison, ungarrison, hitch, mines, jump) use the squad gate. A co-op `BuildOther` belongs
to the co-op player of the command player, and the command player pays
(`BBuildingCommand_processUnit` @ `0x1401987D0` case 13).

| Command | Engine evidence | Status | Notes |
| --- | --- | --- | --- |
| Work: Move, Attack, Gather, Capture, RepairOther, Cloak, Detonate, ChangeMode, Join, Garrison, Ungarrison, Hitch, Unhitch, Mines, Jump family | `BEventDefinitions_getGeneralEventEnum` @ `0x140448700` (`CommandMove`, `CommandAttack`, `CommandDoWork`) | Implemented | `executor.rs`, `executor/` |
| Work: retail right-click (`id` -1, recipient type Army) | `BUser_doWorkAtCurrentLocation` @ `0x14058C2E0`, `BCommand_processArmies` @ `0x1401947B0`, `BWorkCommand_processArmy` @ `0x140196300`, `BSquad_getDefaultOrderType` @ `0x140399E30`, `BSquad_getTacticAction` @ `0x14039C730` | Partial | `WorkCommand::user_work` builds the command. `executor/user_work.rs` applies the retail recipient filter (command player or Gaia; squads that already capture the target and squads with a Repair work action (`mFlagIsRepairing`) are dropped) and the simulation-bounds check. Then each squad gets the action of `default_work_action` (`work_action.rs`: tactic target rules, else the `<Default>` action): Attack, Gather, Capture, Garrison, Join, Hitch or RepairOther, through the same handlers as the typed Work commands. With no action, the squad moves to the waypoint; flag bit 0 queues the move. Missing: armies and platoons (each squad decides alone), queued targeted actions (they start at once). |
| Work: Repair, Transport, Unpack | `BSquad_executeOrder` @ `0x140399780` (cases 4, 23, 20), `BSquad_doTransport` @ `0x1403A2C70`, `BSquad_doUnpack` @ `0x1403A2B90` | Partial | `executor/work_orders.rs`. Repair runs `BSquadActionRepair` (`world/repair/squad.rs`). Transport flies the squad with its garrisoned squads to the order position and unloads them (`world/transports/controllable.rs`); no exit-location search. Unpack moves to the position, then creates the first ability object for the co-op player or owner and kills the squad (`world/work_actions.rs`); no placement check (P4). A repairing squad is not NonMobile for typed Move commands. |
| Work: PointBlankAttack, PlayBlockingAnimation | `BSquad_doPointBlankAttack` @ `0x1403A2990`, `BSquad_doPlayBlockingAnimation` @ `0x14039B880` | Partial | The order records `SquadWorkAction` on the squad. The unit `PointBlankAttack` action (W2) and the blocking-animation action (W3) do not run yet. |
| Work: RequestRepair, Build, Honk, Guard, RallyPoint, Wander, EnergyShield, InfantryEnergyShield | `BSquad_executeOrder` @ `0x140399780` jump table: these types go to the default case | Implemented | Retail gives these orders no squad action; the executor ignores them. Wander and energy shields are persistent tactic actions, not order actions. Retail also removes the executing orders of the squad when the empty order starts; the sim has no squad order queue for that (P5). |
| Building: Research, TrainUnit, TrainSquad, Build, BuildOther, ChangeMode, CustomCommand, ClearRallyPoint | `BBuildingCommand_processUnit` @ `0x1401987d0` | Implemented | `executor/building.rs`, `world/research.rs`, `world/training.rs`, `world/construction.rs` |
| Building: UnloadUnits | `BBuildingCommand_processUnit` case 4, `BSquad_queueUnload` @ `0x14039AD80` | Implemented | `World::unload_building_squads` (`world/building_commands.rs`): the command player's squads leave the parent squad of the recipient. An unloaded trained squad also leaves the birth queue of its trainer. |
| Building: Kill, DestroyBase, CancelKill, CancelDestroyBase | `BBuildingCommand_processUnit` cases 8, 18, 9, 19, `BUnit_doSelfDestruct` @ `0x1404D0640`, `BUnitActionBuilding_doSelfDestruct` @ `0x1404F9780` | Partial | `world/building_commands.rs`: events 73 and 74, the Kill/DestroyBase command check, the Fading state, and the recycle refund on the next production update (`getCost` with count -1, `RecyleRefundRate` when built, hitpoint part when damaged). Not modelled: the construction-paused branch, the creating player of a finished building (the owner gets the refund), and the team score of a recycled building (`0x140387430`). |
| Building: TrainLock, TrainUnlock | `BBuildingCommand_processUnit` cases 14, 15, `BUnit_setTrainLock` @ `0x1404D12E0`, `BUnit_getTrainLock` @ `0x1404D1520` | Partial | `world/building_commands.rs`: the `TrainLock` reference (on `BuildingProduction`), the parking-lot redirect, the Lockdown or Normal mode change of the building squad, and trained squads that stay inside a locked building. The `unit+193` bit that blocks TrainUnlock is not modelled. |
| Building: Reinforce, Ability, Tribute, Power, RallyPoint, ReverseHotDrop | `BBuildingCommand_processUnit` jump table: these types go to the default case | Implemented | Retail does nothing for these building commands; the executor ignores them. |
| Game: Set/Clear global and building rally point, CreateSquad, CreateObject | — | Implemented | `executor.rs` |
| Game: Resign, Disconnect | `BGameCommand_execute` `0x140199690` (cases 7, 8), `BPlayer_resign` `0x1402F0320`, `BPlayer_disconnect` `0x1402F0570` | Implemented | `World::resign_player`, `World::disconnect_player` |
| Game: FogOfWar, RevealMap | `BGameCommand_execute` `0x140199690` (cases 4 and 15 jump to the default case) | Implemented | Retail no-ops: the executor ignores them. |
| Game: QuickBuild, SwitchPlayer, AddResources, AddPopulation, DestroySquad, DestroyUnit, Flare, SelectPower, Tribute, Repair, AttackMove, UnitPower, LookAtPosBroadcast, GameSpeed, ToggleAI, ReverseHotDrop, ActivateSkull, SubUpdating | `Flare` `0x1411ab6c0` | Missing | `CommandExecutor::execute_game` ignores them. A Work command with the `ATTACK_MOVE` flag gives attack-move (`executor/mode_orders.rs`). |
| Power: GrantPower, InvokePower2 | `BPowerManager` `0x1411b6270` | Implemented | `executor.rs`, `world/powers.rs` |
| Power: InvokePower, InvokeAbility | — | Missing | Ignored in `CommandExecutor::execute_power`. |
| PowerInput: Confirm, Position, Direction, Shutdown | `BPowerUser` `0x1411b6260` | Implemented | `World::submit_native_power_input` |

## Power types

`BDatabase_getPowerTypeFromName` @ `0x1401f8400` maps the ten `PowerType` names.

| Power type | Status | Code |
| --- | --- | --- |
| Cleansing | Implemented | `world/powers/cleansing.rs` |
| Orbital | Implemented | `world/powers/orbital.rs` |
| CarpetBombing | Implemented | `world/powers/carpet_bombing.rs` |
| Cryo | Implemented | `world/powers/cryo.rs` |
| Wave | Implemented | `world/powers/wave.rs` |
| Rage | Implemented | `world/powers/rage.rs` |
| Disruption | Implemented | `world/powers/disruption.rs` |
| Repair | Implemented | `world/powers/repair.rs` |
| Transport | Implemented | `world/powers/transport.rs` |
| ODST | Implemented | `world/powers/odst.rs` |
| Trigger-script powers (`TriggerScript`, `CommandTriggerScript` in `BProtoPower_load` @ `0x14034cb00`) | Missing | `powers.xml` has 11 such powers, for example `HookMegaTurret` and `UnscCpgn12Missile`. The sim has no path that runs a power trigger script. |

## Ability types

`BDatabase_getAbilityTypeFromName` @ `0x1401f6560` maps six ability types.

| Ability type | Status | Notes |
| --- | --- | --- |
| Work | Implemented | `gameplay/abilities.rs`, `world/ability.rs` |
| ChangeMode | Implemented | `world/change_mode.rs` |
| Unload | Missing | The Building `UnloadUnits` command is not executed. The trigger `Unload` effect uses `World::trigger_unload_squad`. |
| Unpack | Missing | No Unpack order. |
| CommandMenu | Missing | UI command menu; no sim effect is known. |
| Power | Missing | The Game `UnitPower` and Building `Power` commands are not executed. |

## Unit actions

`BDatabase_getProtoActionTypeFromName` @ `0x1401f8be0` maps the authored tactic `ActionType`.
`BActionManager_createAction` @ `0x14010dbb0` creates 94 action types.

| Action | Status | Code | Notes |
| --- | --- | --- | --- |
| `BUnitActionMove` | Partial | `entities/units/ground_movement.rs`, `world/ground_movement.rs` | No pather. See [Pathing](#world-and-player-systems). |
| `BUnitActionMoveAir` | Implemented | `world/flight/`, `entities/units/flight.rs` | No flight-height mesh. See `aircraft-avoidance.md`. A unit with a physics hover info flies with `BPhysicsHoverFlightAction` (`physics/vehicle_actions/hover.rs`). |
| `BUnitActionMoveWarthog` | Partial | `physics/readback.rs`, `physics/vehicle_actions/warthog.rs` | The Warthog squad is a physics vehicle squad: the squad moves, and `BPhysicsWarthogAction` drives the body. The timed terrain impulses of the move action and the jump lines are not modeled. |
| `BUnitActionMoveGhost` | Implemented | `physics/vehicle_actions/ghost.rs` | `BPhysicsGhostAction` drives the Ghost body toward its squad. |
| `BUnitActionPhysics` | Implemented | `physics/readback.rs`, `physics/death.rs` | Readback of the Havok bodies of units and class-0 objects, early terminate, `CompleteOnInactivePhysics`, `EndMoveOnInactivePhysics`, clamshell and rocket on death. See `movement-and-physics.md`. Jump lines are missing. |
| `BUnitActionRangedAttack` (`RangedAttack`, `HandAttack`) | Implemented | `world/combat/` | |
| `BUnitActionChargedRangedAttack` | Implemented | `world/charge.rs` | |
| `BUnitActionSecondaryTurretAttack`, `BUnitActionSlaveTurretAttack` | Implemented | `world/combat/turrets.rs` | |
| `BUnitActionCollisionAttack` | Implemented | `world/collision_attacks.rs` | |
| `BUnitActionPointBlankAttack` | Missing | — | Used by the shipped Hawk and Hornet tactics. |
| `BUnitActionAreaAttack`, `BUnitActionUnderAttack` | Missing | — | |
| `BUnitActionDOT` | Implemented | `world/weapon_effects/dot.rs` | `BUnit_applyDamageOverTimeEffect` `0x1404D7170`, `BUnitActionDOT_update` `0x1404EC550`. The effect object attaches at the unit root; the retail center offset is not modeled. |
| `BUnitActionStasis`, `BSquadActionDaze` | Partial | `world/weapon_effects/stasis.rs`, `world/weapon_effects/daze.rs` | Stasis counters, speed multiplier, attack block, full stop, release, `StasisDrain` self heal and `StasisBomb` area damage, and the daze with its projectile `TargetAlreadyDazed` rule are implemented. Smart targeting (`chooseSmartTarget`, counter increments), the stasis and stunned animations and the controller grabs are not modeled. The self heal is applied at the next step with a database. |
| `BUnitActionThrown` | Partial | `entities/units/thrown.rs` | The body takes the throw velocity and the throw ends on landing. Only Wave debris and bowled Ram targets are thrown. A weapon kill reads `ThrowUnits` and `PhysicsForce*`/`PhysicsLaunch*` to throw the physics replacement (`world/death_throw.rs`); a weapon does not throw a live unit. `ApplyKnockback` is in `world/combat/knockback.rs`. |
| `BUnitActionBuilding` | Implemented | `world/production/`, `world/research.rs`, `world/construction.rs` | See `production.md` and `research.md`. |
| `BUnitActionCapture` | Implemented | `world/capture.rs` | |
| `BUnitActionChangeOwner` | Missing | — | Used by the Covenant supply depots and `rsrc_power_capturepoint_01`. |
| `BUnitActionChangeMode` | Implemented | `world/change_mode.rs` | |
| `BUnitActionGarrison`, `BUnitActionUngarrison` | Implemented | `world/garrison.rs` | |
| `BUnitActionHitch`, `BUnitActionUnhitch` | Implemented | `world/hitch.rs` | |
| `BUnitActionHotDrop` | Partial | `world/garrison.rs` | Teleporter traversal only. No reverse hot drop. |
| `BUnitActionGather` | Implemented | `world/gather.rs` | |
| `BUnitActionJoin` | Implemented | `gameplay/join.rs`, `entities/squads/join.rs` | |
| `BUnitActionDetonate` | Implemented | `world/detonate.rs` | |
| `BUnitActionMines` | Implemented | `world/mines.rs` | |
| `BUnitActionBomb` | Implemented | `world/bomb.rs` | |
| `BUnitActionDeath` | Partial | `world/lifecycle.rs`, `world/death_spawns.rs`, `world/death_replacements.rs`, `world/death_throw.rs` | No fatalities. `world/death_throw.rs` has the weapon death throw, but no weapon damage gives it throw data yet. |
| `BUnitActionHeroDeath`, `BUnitActionRevive` | Implemented | `world/revival.rs` | |
| `BUnitActionInfect`, `BUnitActionInfectDeath` | Implemented | `world/infect.rs` | |
| `BUnitActionHeal` | Implemented | `world/heal.rs` | |
| `BUnitActionAoeHeal` | Missing | — | Used by `hook_buff_healing_01`. |
| `BUnitActionDodge`, `BUnitActionDeflect` | Implemented | `world/projectiles/defense.rs` | |
| `BUnitActionSpawnSquad` | Implemented | `world/persistent_spawns.rs` | |
| `BUnitActionAmbientLifeSpawner` | Implemented | `world/ambient_life/spawner.rs` | |
| `BUnitActionAirTrafficControl` | Implemented | `world/production/air_traffic_control.rs` | |
| `BUnitActionAvoidCollisionAir` | Implemented | `world/air_avoidance.rs` | |
| `BUnitActionBubbleShield`, `BUnitActionPlasmaShieldGen` | Implemented | `world/protection/` | |
| `BUnitActionEnergyShield`, `BUnitActionInfantryEnergyShield` | Implemented | `world/shields/presentation.rs` | No bone placement. |
| `BUnitActionShieldRegen`, `BSquadActionShieldRegen` | Implemented | `world/shields.rs` | |
| `BUnitActionAmmoRegen` | Implemented | `world/ammunition.rs` | |
| `BUnitActionJump` | Implemented | `world/jump.rs` | |
| `BUnitActionTowerWall` | Implemented | `world/tower_walls.rs` | |
| `BUnitActionRage` | Implemented | `world/powers/rage.rs` | |
| `BUnitActionRoar` | Missing | — | Used by `fld_inf_rhinodillo_01`. |
| `BUnitActionTentacleDormant` | Missing | — | Used by `fld_inf_tentacle_01`. |
| `BUnitActionCoreSlide` | Missing | — | Used by campaign `cpgn_scn14_reactor_01`. |
| `BUnitActionDome` | Missing | — | Used by campaign `cpgn_scn06_shield_01`. |
| `BUnitActionRevealToTeam`, `BUnitActionScaleLOS` | Missing | — | No shipped tactic uses `RevealToTeam`. |
| `BUnitActionHonk`, `BUnitActionBuff` | Missing | — | No shipped tactic uses them. |
| `BUnitActionPlayBlockingAnimation`, `BUnitActionPlayAttachmentAnims` | Partial | `trigger/effects/animations.rs` | Trigger animations only. No blocking order. |

## Squad and platoon actions

| Action | Status | Code | Notes |
| --- | --- | --- | --- |
| `BSquadActionMove`, `BSquadActionAttack`, `BSquadActionWork` | Partial | `entities/squads.rs`, `world/combat/orders.rs` | Straight-line moves to each waypoint. |
| `BSquadActionGarrison`, `BSquadActionUngarrison` | Implemented | `entities/squads/garrison.rs` | |
| `BSquadActionHitch`, `BSquadActionUnhitch` | Implemented | `world/hitch.rs` | |
| `BSquadActionTransport` | Partial | `world/transports.rs`, `world/transports/controllable.rs`, `world/powers/transport.rs` | Trigger, power and Work `Transport` (controllable) transports. The controllable transport uses the straight trigger flight. |
| `BSquadActionRepairOther` | Implemented | `world/repair/other.rs` | |
| `BSquadActionReinforce` | Missing | — | |
| `BSquadActionRepair`, `BSquadActionRepairUnit` | Partial | `world/repair.rs`, `world/repair/squad.rs` | Work order Repair runs `BSquadActionRepair` (heal over the leader `RepairTime`, then reinforce; fails on damage). The repair unit opportunity animation and tech changes to `RepairTime` are not modelled. The Repair game command (pay and order) is not executed. |
| `BSquadActionChangeMode` | Implemented | `world/change_mode.rs` | |
| `BSquadActionCloak` | Implemented | `world/cloak.rs` | |
| `BSquadActionCloakDetect` | Partial | `world/cloak.rs` | Detection event only. No detector action. |
| `BSquadActionCryo` | Implemented | `entities/squads/cryo.rs`, `world/cryo.rs` | |
| `BSquadActionDetonate` | Implemented | `entities/squads/detonate.rs` | |
| `BSquadActionJump` | Implemented | `world/jump.rs` | |
| `BSquadActionWander` | Implemented | `world/wander.rs` | |
| `BSquadActionAmbientLife` | Implemented | `world/ambient_life.rs` | |
| `BSquadActionSpiritBond` | Implemented | `world/spirit_bond.rs` | |
| `BSquadActionReflectDamage` | Implemented | `world/reflect_damage.rs` | |
| `BSquadActionCarpetBomb` | Implemented | `world/squad_carpet_bomb.rs` | |
| `BSquadActionAirStrike` | Missing | — | No shipped tactic uses `AirStrike`. |
| `BSquadActionPlayBlockingAnimation` | Missing | — | Work order 17 records the request (`SquadWorkAction::PlayBlockingAnimation`). |
| `BPlatoonActionMove` | Missing | — | The sim has no platoons. |

## World and player systems

| System | Engine evidence | Status | Notes |
| --- | --- | --- | --- |
| Scenario player setup and player colours | `BScenario_setupPlayers` @ `0x140381cf0`, `BDatabase_setupPlayerColors` @ `0x1401f9560`, `BPlayer_ctor` @ `0x1402ed960` | Implemented | `scenario/player_setup.rs` creates the players, sets the teams, relations, start positions, civs and leaders, and sets `Player::color`. `Player::color` is the index into the `playercolors.xml` colour set. A player with `UsePlayerSettings="false"` and a `Color` other than `-1` gets that `Color`. Each other player gets an entry from the `<civ>` row of its civ in `playercolors.xml`. The entry number is 1 plus the number of earlier world players (IDs from 1) with the same civ. A civ outside `0..=4` gives `-1`. `Player::color` keeps the low byte, so `-1` is `255`. Gaia and the game-mode NPC player keep the constructor value `-1`. The world checksum does not include the colour. |
| Pathing and obstruction manager | `BUnitActionMove_requestPath` @ `0x14053f060`, `BUnitActionMove_refreshPath` @ `0x14053f280` | Missing | Squads steer in a straight line (`Squad::update_movement_with_speed_limit`). The collision attacks test unit obstruction boxes (`world/collision_attacks/obstruction.rs`). |
| Havok physics world | `BPhysicsWorld_init` @ `0x1407BB390`, `BPhysicsWorld_update` @ `0x1407BBB40`, `BTerrainPhysics_createHeightFieldBody` @ `0x14041B500`, `BEntity_createPhysicsObject` @ `0x140204EF0`, `BEntity_setupVehiclePhysics` @ `0x1402055D0` | Implemented | `crates/physics` and `physics/`: gravity (0, −21.7, 0), substeps of at most 0.0125 s, the terrain heightfield body, bodies for units and class-0 objects with a `PhysicsInfo`, the retail filter, the vehicle hull and actions, physics replacements and the gravity ball action. See `movement-and-physics.md` for the gaps. |
| Physics replacements | `BObject_initFromProto` @ `0x14029C300`, `BUnit_createAndThrowPhysicsReplacement` @ `0x1404DEA30` | Partial | `World::spawn_physics_replacement` (`physics/bodies.rs`) for Detonate, jump and Wave. The weapon death throw is not wired to the damage path. No corpse manager. |
| Formations | `BFormation2_writeSavegame` @ `0x14003d040` | Partial | Fixed squad-local offsets. No formation reassignment or priorities. |
| Fog of war and visibility | `BVisibleMap_initMap` @ `0x1405BE260`, `BVisibleMap_exploreTile` @ `0x1405BEBD0`, `BVisibleMap_unexploreTile` @ `0x1405BED10`, `BTileSpanList_buildFromRadius` @ `0x1405BDC50`, `BObject_markLOSOn` @ `0x14029CD50`, `BObject_getSimLOS` @ `0x140299F50`, `BObject_getNewVisibility` @ `0x14029D0C0`, `BWorld_setFogOfWar` @ `0x1405D2A40`, `BWorld_clearBlackMap` @ `0x1405D2BE0`, `BTriggerEffect_teResetBlackMap` @ `0x1404630E0` | Partial | `world/visibility/`: one tile for each sim data tile (`World::visibility_grid`, read-only `VisibilityGrid`) with per-team reference counts, the explored (black map) and visible bits, the retail midpoint span circles, the sim LOS of units (proto LOS, LOS scalar, technology `LOS`) and revealers (whole-map revealer moves to the map center), dead and garrisoned units without LOS, effects 385, 713 and 863 and the sim bounding box sample of `is_entity_visible_to_team`. The circles are updated once per world update after the entity updates, and the grid and the circles are in the checksum. Retail has no terrain occlusion. Not done: LOS blockers (effect 418 and the blocked word; no shipped script uses them), units in cover, spy and sensor-lock (effect 419) visibility, dopples. `fogOfWar.xml` (`BWorldVisibility_loadConfig` @ `0x1406d4d10`) holds only the four render values of the blackmap shader. |
| Resources, population, technology | `BTechTree_save` @ `0x140417190` | Implemented | `world/resources.rs`, `world/technology.rs`, `player/` |
| Tribute | `MenuShowTribute` event | Missing | Commands are not executed. |
| Player end states and game over | `PlayerResignEvent`, `BPlayer_setPlayerState` `0x1402F0610`, `BWorld_checkForGameOver` `0x1405D1530`, `BWorld_updatePlayerUnitCleanup` `0x1405D43C0` | Partial | `World::set_player_state`, trigger effect 130 and game commands 7 and 8 (`BPlayer_resign`, `BPlayer_disconnect`) change the state and queue a `PlayerStateChange`. Each world event checks for game over at once with the skirmish or campaign rule (`GameType`); the winning team changes to `Won`, and `World::game_result` holds the result. The next tick kills the units of a resigned or defeated player unless the game ended. After the game ended, triggers stop and the world updates for `GameOverDelay` seconds (`BTimeManager_getUpdateTime`). Missing: AI shutdown (`BAIManager_onPlayerLeft`) and the local single-player resign (`BUser_resign` `0x1405AED50`). |
| Computer AI | `BAIManager` `0x1411b6068`, `BAIMission` `0x141197658`, `BAITopic` `0x1411975e0`, `BBidManager` `0x1411b6058`, `aidifficultysettings.xml` `0x141196b38`, `BAI_startSkirmishAIScript` @ `0x14014e4d0` | Partial | Each computer AI player runs `SkirmishAI_raw_.triggerscript` (`scenario/player_ais.rs`), but most of its AI effects and conditions are unsupported (issue #16). `player/squad_ai.rs` keeps only the squad-AI roster that triggers read. |
| Knowledge base | `BKBBase` `0x1411976c8`, `BKBSquad` `0x141197668` | Missing | |
| Platoons, armies, dopples | `BPlatoon` `0x1411b6280`, `BArmy` `0x1411b6028`, `Dopples` `0x1411c88c8` | Missing | `entity_id.rs` reserves the entity classes. A right-click Work command does not make an army; each recipient squad gets its order directly (`executor/user_work.rs`). |
| Skulls | `skulls.xml` `0x1411b8508` | Missing | The `ActivateSkull` command is not executed. |
| Fatalities | `fatalityData.xml` `0x1411a6f50` | Missing | |
| Corpse handling | `deathmanager.xml` `0x1411a55d0` | Missing | |
| Game modes | `gamemodes.xml` `0x1411a53b8`, `BGameMode_load` @ `0x140240e70`, `BScenario_load` @ `0x140377be0` (block `GG`) | Implemented | A skirmish adds the mode NPC player, activates the mode tech and launches the world script and one player script for each human and computer AI player (`scenario/game_mode.rs`). The DLC modes need their trigger effects (see `triggers.md`). |
| Save games | `BWorld_writeSavegame` @ `0x1405d47e0`, `BWorld_readSavegame` @ `0x1405d8530` | Missing | |

## Not started

These engine systems have no code in `sim`:

- the pather and the obstruction manager;
- computer AI: AI manager, missions, topics, bids and the knowledge base;
- platoons, armies and dopples;
- tribute, resign, disconnect and the other ignored Game commands;
- trigger-script powers;
- the Reinforce, Unload, Unpack and unit-power commands;
- the PointBlankAttack, ChangeOwner, AoeHeal, Roar, TentacleDormant, CoreSlide, Dome, AreaAttack,
  UnderAttack, RevealToTeam and AirStrike actions;
- smart targeting (`BSquad::chooseSmartTarget`) of stasis and daze weapons;
- skulls, fatalities, corpse handling and save games.
