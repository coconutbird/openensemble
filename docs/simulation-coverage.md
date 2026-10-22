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

| Command | Engine evidence | Status | Notes |
| --- | --- | --- | --- |
| Work: Move, Attack, Gather, Capture, RepairOther, Cloak, Detonate, ChangeMode, Join, Garrison, Ungarrison, Hitch, Unhitch, Mines, Jump family | `BEventDefinitions_getGeneralEventEnum` @ `0x140448700` (`CommandMove`, `CommandAttack`, `CommandDoWork`) | Implemented | `executor.rs`, `executor/` |
| Work: Repair, RequestRepair, Build, Honk, Guard, PlayBlockingAnimation, Unpack, Transport, Wander, PointBlankAttack, EnergyShield, InfantryEnergyShield | `BUnitActionHonk` `0x141193a38`, `BUnitActionPointBlankAttack` `0x141194ab8`, `CommandUnpack` | Missing | `OrderType` in `order.rs` has the values. `CommandExecutor::execute_work` ignores them. |
| Building: Research, TrainUnit, TrainSquad, Build, BuildOther, ChangeMode, CustomCommand, ClearRallyPoint | `BBuildingCommand_processUnit` @ `0x1401987d0` | Implemented | `executor.rs`, `world/research.rs`, `world/training.rs`, `world/construction.rs` |
| Building: UnloadUnits, Reinforce, Ability, Kill, CancelKill, Tribute, Power, TrainLock, TrainUnlock, RallyPoint, DestroyBase, CancelDestroyBase, ReverseHotDrop | `BSquadActionReinforce` `0x1411955e8`, `CommandRecycle` | Missing | `CommandExecutor::execute_building` ignores them. |
| Game: Set/Clear global and building rally point, CreateSquad, CreateObject | — | Implemented | `executor.rs` |
| Game: QuickBuild, SwitchPlayer, AddResources, AddPopulation, FogOfWar, DestroySquad, DestroyUnit, Resign, Disconnect, Flare, SelectPower, RevealMap, Tribute, Repair, AttackMove, UnitPower, LookAtPosBroadcast, GameSpeed, ToggleAI, ReverseHotDrop, ActivateSkull, SubUpdating | `PlayerResignEvent` `0x1411cafe0`, `Flare` `0x1411ab6c0` | Missing | `CommandExecutor::execute_game` ignores them. A Work command with the `ATTACK_MOVE` flag gives attack-move (`executor/mode_orders.rs`). |
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
| `BUnitActionMoveAir` | Implemented | `world/flight/`, `entities/units/flight.rs` | No flight-height mesh. See `aircraft-avoidance.md`. |
| `BUnitActionMoveWarthog` | Implemented | `entities/units/warthog.rs` | |
| `BUnitActionMoveGhost` | Partial | `entities/units/vehicle.rs` | Forward response only. |
| `BUnitActionPhysics` | Partial | `physics.rs` | Deterministic boxes, no Havok shapes or springs. |
| `BUnitActionRangedAttack` (`RangedAttack`, `HandAttack`) | Implemented | `world/combat/` | |
| `BUnitActionChargedRangedAttack` | Implemented | `world/charge.rs` | |
| `BUnitActionSecondaryTurretAttack`, `BUnitActionSlaveTurretAttack` | Implemented | `world/combat/turrets.rs` | |
| `BUnitActionCollisionAttack` | Implemented | `world/collision_attacks.rs` | |
| `BUnitActionPointBlankAttack` | Missing | — | Used by the shipped Hawk and Hornet tactics. |
| `BUnitActionAreaAttack`, `BUnitActionUnderAttack` | Missing | — | |
| `BUnitActionDOT` | Missing | — | The weapon `DOTrate`, `DOTduration` and `DOTEffect` fields are not read. |
| `BUnitActionStasis`, `BSquadActionDaze` | Missing | — | The weapon `Stasis`, `StasisDrain`, `StasisBomb` and `Daze` fields are not read. |
| `BUnitActionThrown` | Partial | `entities/units/thrown.rs` | Wave debris only. Weapon `ThrowUnits`, `PhysicsLaunch*` and `ApplyKnockback` are not read. |
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
| `BUnitActionDeath` | Partial | `world/lifecycle.rs`, `world/death_spawns.rs`, `world/death_replacements.rs` | No fatalities. |
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
| `BSquadActionTransport` | Partial | `world/transports.rs`, `world/powers/transport.rs` | Trigger and power transports. The Work `Transport` order is not executed. |
| `BSquadActionRepairOther` | Implemented | `world/repair/other.rs` | |
| `BSquadActionReinforce` | Missing | — | |
| `BSquadActionRepair`, `BSquadActionRepairUnit` | Partial | `world/repair.rs` | `Heal`, `RepairOther` and the Repair and Rage powers use the repair code. The Repair command is not executed. |
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
| `BSquadActionPlayBlockingAnimation` | Missing | — | |
| `BPlatoonActionMove` | Missing | — | The sim has no platoons. |

## World and player systems

| System | Engine evidence | Status | Notes |
| --- | --- | --- | --- |
| Pathing and obstruction manager | `BUnitActionMove_requestPath` @ `0x14053f060`, `BUnitActionMove_refreshPath` @ `0x14053f280` | Missing | Squads steer in a straight line (`Squad::update_movement_with_speed_limit`). Obstructions are circles and boxes in `physics.rs`. |
| Formations | `BFormation2_writeSavegame` @ `0x14003d040` | Partial | Fixed squad-local offsets. No formation reassignment or priorities. |
| Fog of war and visibility | `BWorldVisibility_loadConfig` @ `0x1406d4d10`, `fogOfWar.xml` `0x1411daa68` | Partial | `world/visibility.rs` tests unit sight circles and revealers on each query. No visibility grid, explored map or terrain occlusion. |
| Resources, population, technology | `BTechTree_save` @ `0x140417190` | Implemented | `world/resources.rs`, `world/technology.rs`, `player/` |
| Tribute | `MenuShowTribute` event | Missing | Commands are not executed. |
| Player end states | `PlayerResignEvent`, `BPlayer_setPlayerState` `0x1402F0610` | Partial | `World::set_player_state` and trigger effect 130 set the state, the end time and the unit cleanup flag, and queue a `PlayerStateChange`. Nothing consumes the queue yet: no game over, no unit cleanup (`BWorld_updatePlayerUnitCleanup` `0x1405D43C0`), no AI shutdown. |
| Computer AI | `BAIManager` `0x1411b6068`, `BAIMission` `0x141197658`, `BAITopic` `0x1411975e0`, `BBidManager` `0x1411b6058`, `aidifficultysettings.xml` `0x141196b38` | Missing | `player/squad_ai.rs` keeps only the squad-AI roster that triggers read. |
| Knowledge base | `BKBBase` `0x1411976c8`, `BKBSquad` `0x141197668` | Missing | |
| Platoons, armies, dopples | `BPlatoon` `0x1411b6280`, `BArmy` `0x1411b6028`, `Dopples` `0x1411c88c8` | Missing | `entity_id.rs` reserves the entity classes. |
| Skulls | `skulls.xml` `0x1411b8508` | Missing | The `ActivateSkull` command is not executed. |
| Fatalities | `fatalityData.xml` `0x1411a6f50` | Missing | |
| Corpse handling | `deathmanager.xml` `0x1411a55d0` | Missing | |
| Game modes | `gamemodes.xml` `0x1411a53b8` | Missing | The sim does not read game modes. |
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
- the weapon effect families: damage over time, stasis, daze, throw and knockback;
- skulls, fatalities, corpse handling, game modes and save games.
