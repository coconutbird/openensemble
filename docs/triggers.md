# Trigger simulation

Scenario trigger systems are authoritative simulation state. The SCN loader
reopens the selected scenario XMB from the same layered `AssetSource` used for
the database, loads only top-level `TriggerSystem` nodes, resolves their values
against the scenario-layered database, and attaches the resulting scripts to
`World`. The renderer does not interpret triggers or recreate their results.

## Retail scheduling reproduced

The update order follows the recovered Halo Wars implementation:

1. Commands, production, and entity/gameplay updates run for the fixed tick.
2. The world trigger manager visits scripts in runtime-ID order.
3. Each script resets its per-trigger evaluation counts and copies its active
   trigger list into a FIFO evaluation queue.
4. Activations during effects are unique-added to that same queue. These
   latecomers can therefore evaluate during the current tick.
5. Evaluating a condition tree advances its next-evaluation time and count.
   AND/OR trees short-circuit in authored order, and zero conditions are true.
6. A true trigger, or either branch of a conditional trigger, deactivates
   before firing effects. A self-activation effect can consequently enqueue it
   again during the same tick.
7. Scripts with no active triggers are removed after their update.

`EvalLimit` bounds same-tick reactivation exactly at the trigger level. The
engine also has a deterministic hard guard for malformed unbounded loops and
reports when that guard is reached.

## Authored data handling

- Sparse variable IDs remain sparse; no vector-index renumbering occurs.
- Runtime trigger IDs are assigned in authored order. Trigger-valued variables
  are remapped from editor IDs to those runtime IDs.
- `Input` and `Output` children retain their one-based `SigID` slots. Effects
  and conditions resolve by signature rather than child order.
- Scenario Unit, Squad, Object, and Entity references are remapped to current
  generational entity IDs. Prototype and technology names resolve through the
  active scenario-layered database.
- Database-backed simulation ticks pass that same layered database into trigger
  evaluation. Typed `TechStatus`, player-state, civilization, leader, relation,
  squad-mode, difficulty, general-event, loc-string, cinematic, and
  talking-head values are resolved to retail numeric identities during loading
  rather than interpreted by presentation code.
- The loader accepts retail's exported aliases `CommandType`, `Diplomacy`,
  `Direction`, and `LocationList`. They map to the canonical
  `TechDataCommandType`, `RelationType`, `Vector`, and `VectorList` runtime
  types. Command names resolve to the exact `BProtoObjectCommand` ordinal table;
  unknown names become `-1`.
- Object-type variables retain their authored names. Retail maps those names
  to an ordinal table made from `objects.xml` followed by `objecttypes.xml`;
  name identity gives the same membership semantics without exposing database
  ordinals to the renderer or making scenario overlays invalidate live values.
- The sim projects every layered proto-object's concrete name and abstract
  memberships into `World`. This immutable catalog is checksummed and supports
  trigger inputs that refer to a prototype rather than a live entity.
- A checksummed proto-squad catalog stores each layered definition's name and
  authored child capacity. It bridges the database IDs stored by
  trigger/live-squad state with the runtime table indices carried by production
  commands. Matching queued work by its authoritative prototype name avoids
  collisions between those two numeric domains.
- Placed Unit/Building/Squad-class proto-objects use retail `createEntity`
  semantics: even with `IsSquad=false`, they become a one-member squad unless
  they are physics replacements. This is why Blood Gulch's teleporter trigger
  variables correctly resolve to squads.
- Trigger definitions and live variables, activation state, evaluation times,
  co-op/configuration settings, per-player difficulty scalars, teleporter links,
  general-event subscribers, pending presentation requests, and unit/squad
  idle-action and unit revival state participate in the world checksum. Scenario loading
  initializes every player's difficulty from the layered
  `GameData/DifficultyDefault`; difficulty conditions use the layered Normal,
  Hard, and Legendary thresholds.
- Empty non-null trigger variables are initialized through their authored type.
  In particular, empty PlayerList, TeamList, spatial, color, and cost outputs no
  longer degrade to a boolean placeholder before their first write.

## Implemented operations

Authoritative condition implementations currently cover primitive count,
boolean, float, percent, hit-point, time, and string comparisons; game and
trigger-active time checks; reached-time outputs; affordability and population
checks; common unit/squad lifecycle queries; ownership and proto-object tests;
retail `IsObjectType` v1/v2 across squad, unit, object, and proto-object inputs;
basic player/team predicates; and retail `ContainsGarrisoned`,
`HasGarrisoned`, and `IsGarrisoned` containment queries. These include
optional contained-player/object-type filters, the version-2 count output, and
the squad-list input.

Player identity, diplomacy, and capacity predicates preserve their authored
signature behavior:

- DBID 131 `PlayerInState` supports retail version 2 and compares a valid
  player's live state with the scenario-database-resolved `PlayerState` value.
- DBIDs 240/476 `CompareCiv` and `CompareLeader` accept only equal/not-equal
  operators. DBID 477 `PlayerUsingLeader` compares a valid player's live
  leader selection.
- DBID 362 `CheckDiplomacy` gathers unique teams from its unit, unit-list,
  squad, and squad-list inputs and checks every team against the selected
  player or team. Stale entity handles are skipped. As in retail, a used input
  containing only stale handles produces an empty all-match set, while an
  invalid used player prevents fallback to the team input.
- DBID 438 `IsSquadAtMaxSize` compares the live squad child count for exact
  equality with the capacity loaded from its layered proto-squad definition.
- DBID 513 `CompareProtoSquad` applies the full retail ordered-operator table to
  the two resolved database IDs. DBID 627 `IsCoop` reads synchronized world
  setup, and DBID 629 `IsConfigDefined` tests a retail case-insensitive ASCII
  configuration symbol loaded into synchronized world state.
- DBID 972 `CheckDifficulty` reads the selected player's continuous difficulty
  scalar. Version 1 tests equality; version 2 applies its authored comparison
  operator to the standard Easy/Normal/Hard/Legendary category derived from
  the active layered database thresholds.

Resource, technology, and combat-readiness predicates use checksummed sim state:

- DBID 339 `CompareCost` compares each resource slot. Slot 3 selects retail OR
  semantics; absent or false uses AND semantics.
- DBID 340 `CheckResourceTotals` reads the player's lifetime resource
  accumulator, not the spendable balance. Layered leader resources seed both
  values, earning resources increases both, and payment/refunds affect only the
  balance.
- DBID 5 `TechStatus` supports versions 1 and 2 and derives global statuses from
  the same database-backed research system used by production. Invalid tech IDs
  compare as `Unobtainable`, matching retail. Version-2 unit scoping works for
  ordinary technologies; per-unit `UniqueProtoUnitInstance` state remains a
  deliberate false result until unique technology ownership is modeled.
- DBID 256 `CompareAmmoPercent` version 2 reads a unit's authoritative current-
  over-maximum ratio, writes the optional ratio output, and applies the authored
  comparison operator. A stale unit reads and writes zero before comparison.
- DBID 461 `IsUnderAttack` supports versions 1 and 2. Accepted member damage
  records the squad's game-time timestamp; a nonzero timestamp passes when its
  wrapping elapsed time is within the authored inclusive interval. Version 2
  unique-adds the scalar squad to its list and skips stale handles.
- DBID 264 `IsIdle` tests the explicit idle action rather than merely comparing
  an entity state. The first qualifying update creates an action at duration
  zero, later updates advance its wrapping millisecond duration once per whole
  sim update, and conflicting orders remove it. A used valid Unit takes
  precedence and writes its action duration, even when it is not idle. A stale
  Unit falls through to Squad; that path tests the squad's own idle action.

Player roster conditions consume authoritative `World` queries rather than
renderer-visible counts:

- DBID 436 `ComparePlayerUnitCount` counts every live class-1 unit owned by the
  player, including buildings. Its optional object type accepts either a
  concrete proto-object name or an abstract membership. Version 1 and version
  2 otherwise share the same live count.
- The shipped version-2 handler at `0x140429880` checks the `IncludeTraining`
  binding in slot 5 but accidentally uses the still-false local boolean as a
  variable index, reading slot 0. That is an unsafe out-of-bounds access in
  retail. The sim preserves the observable defect deterministically by treating
  slot 0 as absent, so this condition does not add future units. The separate
  `World::player_future_unit_count` query still models retail future-unit state
  for systems that use it and counts only `TrainUnit` work.
- DBID 606 `ComparePlayerSquadCount` counts live squads, optionally filters one
  proto-squad, and correctly adds matching queued `TrainSquad` work when slot 5
  is true. The scenario-layered proto-squad catalog resolves the filter across
  live database IDs and queued runtime indices. Queued `TrainUnit` work never
  contributes to a squad count.

The installed executable handlers used for this contract were named
`BTriggerCondition_tcComparePlayerUnitCountV1` (`0x140429800`),
`BTriggerCondition_tcComparePlayerUnitCountV2` (`0x140429880`), and
`BTriggerCondition_tcComparePlayerSquadCount` (`0x14042C110`) in the saved IDB.
The resource/technology/readiness handlers were likewise verified and saved as
`BTriggerCondition_tcTechStatusV1` (`0x1404256E0`),
`BTriggerCondition_tcTechStatusV2` (`0x140425770`),
`BTriggerCondition_tcIsIdle` (`0x140427D70`),
`BTriggerCondition_tcCompareCost` (`0x140428EB0`),
`BTriggerCondition_tcCheckResourceTotals` (`0x140428FA0`),
`BTriggerCondition_tcIsUnderAttack` (`0x140429CF0`), and
`BTriggerCondition_tcIsUnderAttackV2` (`0x140429D90`).

Authoritative effects cover trigger activation/deactivation, integer
increment/decrement, the retail copy-effect family, player resources and
technology, database-backed entity creation and lifecycle, health, ownership,
spatial queries and direct transforms, deterministic entity-list construction,
supported scripted orders, and teleporter-destination setup.

Resource and technology effects operate on the same player state used by
production and conditions:

- DBID 50 `PayCost` subtracts the three trigger-cost slots only when the player
  can afford all of them. An unaffordable payment is a supported no-op. DBID 51
  `RefundCost` changes only the spendable balance.
- DBIDs 277/278 `SetResources`/`GetResources` write and read the spendable
  balance. DBIDs 337/338 `SetResourcesTotals`/`GetResourcesTotals` independently
  write and read lifetime totals.
- DBIDs 60/61 `TechActivate`/`TechDeactivate` resolve the runtime technology
  index in the active layered database and use the normal world technology
  path. Existing entities and future spawns therefore observe the same
  transforms and modifiers.
- `ModifyProtoData` and ordinary technology activation support retail proto-
  data ordinals 3 `AmmoMax` and 58 `AmmoRegenRate`. Existing enabled units
  reconcile immediately, future units use the effective values at spawn, and
  proto-squad maximums retain authored member counts.

Ammunition effects share that same synchronized unit and squad state:

- DBID 389 `GetAmmo` version 1 reads Unit and version 2 adds Squad. A used Unit
  takes precedence; amount and percentage outputs are written only for a valid
  selected target. Squad current ammunition sums live ammo-enabled children,
  while its denominator is the effective proto-squad maximum.
- DBID 390 `SetAmmo` has the same versioned target slots and Unit precedence.
  Amount takes precedence over percentage. Unit assignment is the retail raw
  setter; a squad amount is converted through its proto maximum, and a squad
  percentage assigns each current child its own maximum times that ratio.

Entity effects preserve the representable retail contracts:

- DBID 35 `CreateObject` versions 5/6 creates a standalone database-backed
  mobile unit or building, supports authored facing and version-6 `NoPhysics`,
  writes the invalid-entity sentinel on failure, and applies retail clear/add
  behavior to its optional object-list output.
- DBID 36 `CreateSquad` versions 6/7 creates the full proto-squad roster with
  version-7 facing, optional rally or attack-move movement, and scalar/list
  outputs. Authored fly-in/fly-off inputs create the civilization's trigger
  transport, contain the squad for its incoming flight, release it at the
  drop-off, apply facing and rally state, then remove the outgoing carrier.
- DBID 875 `CreateSquads` creates one full database-backed squad for every
  ordered ProtoSquadList entry, including duplicates. Its first output replaces
  a SquadList; its second optionally clears and then appends, matching retail's
  distinct output contracts. Authored fly-ins partition whole squads across a
  capacity-aware, `TransportMax`-bounded carrier group and share the same
  authoritative containment, flight, drop-off, facing, rally/attack-move, and
  cleanup lifecycle. If the player has no usable civilization transport, both
  creation effects retain retail's ground fallback.
- DBID 154 `CreateUnit` versions 1/2 promotes an allowed proto-object into the
  retail one-member wrapper squad, returns its leader unit and squad, preserves
  `StartBuilt` for buildings, and supports version-2 facing. On failure, retail
  invalidates scalar outputs but leaves list outputs unchanged; the sim keeps
  that distinction.
- DBIDs 37/38 `Kill`/`Destroy` versions 3/4 accept their Unit, UnitList, Squad,
  SquadList, Object, and ObjectList slots in retail order. A kill leaves dead
  state observable until the next entity update. Destroy removes the entity and
  invalidates its generational ID immediately. Squad lifecycle cascades to all
  member units. A regular kill of a scenario-layered `_HeroDeath` unit or squad
  instead leaves the unit alive at one HP with its independent `Down` flag;
  immediate destruction still removes it. Retail's prevent-scoring flag is
  currently vacuous because no scoring subsystem exists yet.

Health and ownership effects mutate the same checksummed state used by combat,
population, conditions, and rendering:

- DBID 174 `GetHealth` versions 2/3 reproduces retail target collection and
  writes optional aggregate HP, HP ratio, shields, and shield ratio outputs.
  Version 3 adds Unit and UnitList inputs. Maximums are accumulated per used
  input while current values follow retail's unique target list, including its
  observable overlap behavior.
- DBIDs 318/325 `Repair`/`Damage` accept squad/unit scalar and list inputs,
  prefer absolute values over percentages for each channel, and divide only
  absolute values when `Spread` is true. Repair clamps at the player-modified
  maximum; direct damage clamps at zero. Like retail's raw setters, direct
  damage does not emit a combat event or clear the unit's explicit alive flag.
- DBID 336 `CombatDamage` versions 1/2 uses the normal shield-first damage path,
  updates the squad damage timestamp, and applies the target's layered mortal,
  tactic-`Revive`, or `_HeroDeath` zero-HP behavior. Version-2
  `OverrideRevive=true` reproduces retail's pre-damage check: it only marks a
  tactic-`Revive` unit that was already at zero HP, which dies on the following
  entity update. The same lethal event still hibernates the unit, and hero-down
  units are unaffected. Prevent-scoring behavior remains vacuous without a
  scoring subsystem.
- DBID 137 `ChangeOwner` version 3 transfers each selected squad and its member
  units. Live population, built population-cap contributions, and an included
  base anchor move to the new player atomically. DBID 193 `GetOwner` reads that
  authoritative unit/squad ownership directly.

Spatial effects likewise operate only on world transforms; the renderer reads
their result rather than interpreting trigger logic:

- DBID 189 `GetLocation` versions 1/2 preserves retail's unit/squad/object
  precedence and leaves the output unchanged for a stale selected handle.
- DBID 499 `GetDirection` versions 1/2 returns normalized forward and optional
  right/up vectors. A missing or stale entity writes retail's
  `cInvalidVector=(-1,-1,-1)`. DBID 500 `GetDirectionFromLocations` normalizes
  the vector from its first location to its second.
- DBID 489 `SetDirection` versions 1/2 follows the distinct retail collection
  rules: version 1 updates squads and their members, while version 2 updates
  backing unit/object transforms only. Turret-hardpoint aiming is reported
  unsupported before mutation because hardpoint orientation state is not yet
  simulated.
- DBID 354 `Teleport` version 3 supports exact squad placement when
  `IgnorePlot=true`, resets authoritative orders/containment, and resynchronizes
  every member from its formation offset. Object-only version-2/3 teleports are
  also authoritative. Squad placement that requests retail's `BSquadPlotter`
  is reported unsupported before any squad or object moves. Object obstruction
  fallback/search remains a fidelity gap; without a valid plotted alternative,
  the sim uses the authored destination.

Entity-list effects build targets from authoritative world state:

- Conditions 2/4 implement the shipped v5/v6 `CanGetUnits` and v8/v9
  `CanGetSquads` search paths. The older versions preserve pool-order stable
  filtering; the newer global paths preserve player/roster order and retail's
  unordered rejection. Their distinct empty-PlayerList behavior, spherical
  precedence over oriented boxes, area liveness rules, object/proto/filter
  predicates, output order, and version-9 `IgnoreAir` leader test are retained.

- DBIDs 122/123 write the unique current team roster and the selected team's
  players in world order. Version-2 DBIDs 124-127 mutate PlayerList and TeamList
  variables with retail clear, unique-add, ordered removal, and self-alias
  behavior; IDs are data values and are not silently validated against the
  current roster.
- DBID 265 `GetGameTime` writes wrapping 32-bit game milliseconds plus its
  optional delta. DBID 267 `GetGameTimeRemaining` writes the saturating time to
  its target. Both query authoritative sim time rather than renderer time.

- DBID 112 `GetUnits` version 3 selects explicitly live class-1 units by an
  optional player and concrete/abstract object type. When both location and
  distance are used, the search is an inclusive circular X/Z obstruction
  query; otherwise it follows retail's all-player roster order. A UnitList
  filter can contain unit IDs directly or squad IDs whose children should be
  selected, matching the retail handler's deliberately loose entity-ID use.
  Global dead-entry removal preserves retail's unordered swap behavior.
- DBID 113 `GetSquads` version 4 supports player, proto-squad, circular-area,
  exact list-membership, and object-type filters. Every current child must
  match the object type. Empty squads pass that all-children test in a global
  query but cannot enter an area result because retail spatial lookup tests a
  squad through a leader unit. Global dead, filter-list, and object-type
  rejection occurs in retail's same sequential unordered-removal order.
- DBIDs 146/147 write UnitList/SquadList sizes. DBIDs 148/149 optionally clear
  a destination, validate only the scalar entity, and unique-add two authored
  lists without silently purging stale generational handles. DBIDs 150/151
  preserve list order while removing the scalar and then each listed value.
  Aliasing an input with the destination follows retail's mutation order,
  including its observable every-other-element result for a self-aliased
  remove list. DBID 836 applies that same mutation contract to ObjectList.
- DBID 630 writes a live squad's proto-squad ID or retail's invalid `-1`
  sentinel. DBID 818 copies a typed loc-string ID through the ordinary retail
  copy path.
- DBIDs 296/308 partition SquadList/UnitList values by a preferred count or a
  clamped percentage rounded to nearest-even. Count takes precedence. Retail's
  unusual mutation order is preserved: unit partition clears its source before
  its two outputs, while squad partition leaves the source untouched. DBIDs
  297/302 shuffle in place with shipping `cSimRand`: every position swaps with
  an inclusive full-list index produced by the MSVC `214013/2531011` LCG,
  rather than using Fisher-Yates.
- DBIDs 334/335 compute stable only-left, only-right, and intersection lists.
  Both sources are copied before outputs are cleared, so source/output aliasing
  follows retail exactly.

DBIDs 152/153 attach a checksummed iterator to the current UnitList or
SquadList variable and reset its per-entity visited sets. Conditions 140/141
`NextUnit`/`NextSquad` scan the list's current order, mark the first unvisited
ID, and write it to the authored output. Self-reactivation through the normal
FIFO trigger scheduler can therefore consume a whole list during one fixed
tick; reattachment resets traversal, while edits to the attached list are seen
by the next condition evaluation.

Entity filters are authoritative, ordered, checksummed trigger values:

- DBIDs 341–348 clear a set or append `IsAlive`, copied UnitList/SquadList
  membership, player, team, proto-object, proto-squad, and object-type
  predicates. Predicates are ANDed in insertion order and each applies its
  authored inversion after its base test.
- DBID 355 appends `IsIdle`. Units test their explicit idle action; squads use
  retail's virtual squad predicate, which requires every valid current child
  unit to be idle and therefore considers an empty squad idle.
- DBID 379 appends a directed diplomacy-relation predicate. A used Player input
  takes precedence over Team; an invalid used player does not fall back, and no
  predicate is appended when no reference team resolves. Missing entity teams
  compare as Neutral, and inversion is applied after the raw relation match.
- DBIDs 349/350 filter copied UnitList/SquadList sources into stable passed and
  failed outputs. Stale handles are omitted from both. Outputs are written
  passed then failed, including the retail behavior where a shared output is
  left with the failed list. An empty set passes every valid handle.
- Proto-object and object-type predicates require every current valid child of
  a squad to match at least one supplied type; missing children are skipped and
  empty squads pass. A proto-squad predicate checks a squad directly and a unit
  through its current parent squad.
- `IsAlive` reproduces `BEntityFilterIsAlive`, `tcIsAliveV3`, and
  `tcIsDeadV3`: units require their explicit alive flag and reject independent
  `Down`/`IsHibernating` flags; squads additionally reject either flag on any
  current child. Inversion is applied only after this complete base test.

Scripted orders mutate the same squad movement and containment state consumed
by physics, checksums, and rendering:

- DBID 65 `Unload` versions 3/4 treats its Squad/SquadList inputs as container
  squads. Version 4 optionally restricts the contained passenger squads; an
  empty filter means all passengers. Passengers are dispatched under their
  current owners and enter the normal authoritative ungarrison lifecycle.
- DBID 71 `CarpetBomb` supports the two source-active signatures. Version 3 accepts a Flying unit,
  resolves its parent squad, and uses the squad's current position as the attack-run origin.
  Version 4 accepts a proto-squad carrying the `Flying` flag, refuses a squad with any returning
  `MoveAir` child, and obtains the base only from child zero; a non-`MoveAir` lead therefore keeps
  the retail invalid-base result even when a later child has a base. Optional distance/count
  inputs default to 20/1. The resulting checksummed squad action owns its ground-target list,
  per-member attack progress, persistent `IgnoreLeash` flag, `MoveAir` launch/return requests, and
  lead-position dragging. Position attacks use the ordinary tactic selector and shared ranged
  executor, including animation tags, hardpoint launch anchors, ammunition, projectiles, and area
  damage; actions authored only for air targets are rejected. Installed campaign/tutorial trigger
  catalogs contain no DBID 71 instance, so this contract is covered by named-source and synthetic
  scenario-trigger regressions rather than a fabricated shipped fixture.
- DBID 66 `Move` version 6 and DBID 117 `Work` versions 3/4 support the
  currently representable location-target path when `AttackMove`, `QueueOrder`,
  and `DoAbility` are false. Squad-list values are consumed first, the valid
  scalar squad is unique-added, stale target entities fall through according
  to retail target precedence, and every surviving current squad receives the
  location order.
- A live unit/squad target, attack-move, queued order, or AI-command ability is
  reported unsupported before any recipient changes. Retail resolves those
  generic work commands through action/tactic, army queue, and ability state
  that the sim does not yet model; treating them as ordinary movement or attack
  would create divergent game state.

Blood Gulch's DBID 967 links each source teleporter squad to its target through
the normal fixed simulation tick. The authoritative garrison lifecycle then
consumes that link; see [Garrison and teleporters](garrison-and-teleporters.md).

General events and UI completion use an explicit sim/UI boundary:

- DBIDs 811/1018 create checksummed retail subscribers, including the exact
  `(index << 16) + eventType` ID layout, optional player filtering, and counted
  firing. DBIDs 812/837 reset ordinary or counted state; DBIDs 838–840 append
  copied camera, entity, and entity-list filters; DBIDs 988/989 preserve the
  disabled-delete and clear-filter behavior.
- Condition 813 reads subscriber fired/count state. Conditions 905/906 read the
  dedicated chat/cinematic-completed subscribers; chat version 2 applies its
  optional wrapping game-time delay from the recorded fire time.
- DBIDs 729/480 enqueue typed chat or cinematic requests in authoritative
  `World` state. The renderer reads `World::presentation_requests()` as UI and
  submits `acknowledge_presentation` as synchronized input when playback ends.
  The sim then fires the matching completion event. Headless simulation never
  fabricates an immediate completion.

The installed executable effect dispatcher and every standalone handler
inspected for this slice were named, commented, and saved in the IDB:
`BTriggerScript_update` (`0x140490E40`), `BTriggerEffect_fire`
(`0x14044A0A0`), `BTriggerEffect_teKillV3` (`0x14044EE80`),
`BTriggerEffect_teKillV4` (`0x14044F0B0`), `BTriggerEffect_teDestroyV3`
(`0x14044F410`), `BTriggerEffect_teDestroyV4` (`0x14044F650`),
`BTriggerEffect_teCreateObjectV5` (`0x1404505B0`),
`BTriggerEffect_teCreateObjectV6` (`0x140450800`),
`BTriggerEffect_teCreateSquadV6` (`0x140450A70`),
`BTriggerEffect_teCreateSquadV7` (`0x1404511B0`),
`BTriggerEffect_tePayCost` (`0x140451B50`),
`BTriggerEffect_teRefundCost` (`0x140451BF0`),
`BTriggerEffect_teCreateUnitV1` (`0x140459FD0`),
`BTriggerEffect_teCreateUnitV2` (`0x14045A360`), and
`BTriggerEffect_teGetResourcesTotals` (`0x14045DF00`). The shipping compiler
inlined technology activation/deactivation and the other resource get/set cases
into `BTriggerEffect_fire`; they have no standalone functions to rename.

The spatial/health/ownership handlers were also recovered, named, commented,
and saved as `BTriggerEffect_teGetLocationV1` (`0x140458B50`),
`BTriggerEffect_teGetLocationV2` (`0x140458BF0`),
`BTriggerEffect_teGetOwner` (`0x140458CC0`),
`BTriggerEffect_teChangeOwnerV3` (`0x1404593A0`),
`BTriggerEffect_teGetHealthV2` (`0x14045B880`),
`BTriggerEffect_teGetHealthV3` (`0x14045BD00`),
`BTriggerEffect_teRepair` (`0x14045F700`),
`BTriggerEffect_teDamage` (`0x1404601C0`),
`BTriggerEffect_teCombatDamageV1` (`0x140460B40`),
`BTriggerEffect_teCombatDamageV2` (`0x140460FF0`),
`BTriggerEffect_teTeleportV2` (`0x1404618B0`),
`BTriggerEffect_teTeleportV3` (`0x1404620A0`),
`BTriggerEffect_teSetDirectionV1` (`0x140467F80`),
`BTriggerEffect_teSetDirectionV2` (`0x1404684B0`),
`BTriggerEffect_teGetDirectionV1` (`0x140468C40`), and
`BTriggerEffect_teGetDirectionV2` (`0x140468D30`). DBID 500
`GetDirectionFromLocations` is compiler-inlined in `BTriggerEffect_fire` and
has no standalone routine to rename.

The target-list and scripted-order routines were recovered and saved as
`BTriggerEffect_teUnloadV3` (`0x140453870`),
`BTriggerEffect_teUnloadV4` (`0x1404539E0`),
`BTriggerEffect_teMoveV6` (`0x140453BF0`),
`BTriggerEffect_teGetUnitsV3` (`0x140456FD0`),
`BTriggerEffect_teGetSquadsV4` (`0x140457520`),
`BTriggerEffect_teWorkV3` (`0x140457C70`),
`BTriggerEffect_teWorkV4` (`0x1404582F0`),
`BTriggerEffect_teUnitListAdd` (`0x140459BC0`),
`BTriggerEffect_teSquadListAdd` (`0x140459D40`), and the shared
`BTriggerEffect_teUnitOrSquadListRemove` (`0x140459EC0`). DBIDs 146/147 list
size are compiler-inlined as direct writes in `BTriggerEffect_fire`.

The DBID 71 signatures and `BSquadActionCarpetBomb`/`BUnitActionMoveAir` interaction were recovered
from the named 2008 source. No additional installed-executable function was inspected for that
slice.

The list-processing, iterator, and filter handlers were recovered as
`BTriggerVarIterator_attachIterator` (`0x140447C20`),
`BTriggerCondition_tcNextUnit` (`0x140426BA0`),
`BTriggerCondition_tcNextSquad` (`0x140426CC0`),
`BTriggerEffect_teSquadListPartition` (`0x14045E9C0`),
`BTriggerEffect_teUnitListPartition` (`0x14045EBD0`),
`BTriggerVarEntityIDList_shuffle` (`0x1404B2750`),
`BTriggerEffect_teUnitOrSquadListDiff` (`0x140460820`),
`BTriggerEffect_teUnitListFilter` (`0x140461650`), and
`BTriggerEffect_teSquadListFilter` (`0x140461780`). The shared filter-set
workers are `BEntityFilterSet_clearFilters` (`0x140216CA0`),
`BEntityFilterSet_addEntityFilterIsAlive` (`0x140216D20`),
`BEntityFilterSet_addEntityFilterInList` (`0x140216E60`),
`BTriggerEffect_teEntityFilterAddInList` (`0x1404614D0`),
`BEntityFilterSet_addEntityFilterPlayers` (`0x140216F10`),
`BEntityFilterSet_addEntityFilterTeams` (`0x140216FC0`),
`BEntityFilterSet_addEntityFilterProtoObjects` (`0x140217070`),
`BEntityFilterSet_addEntityFilterProtoSquads` (`0x140217120`),
`BEntityFilterSet_addEntityFilterObjectTypes` (`0x1402171D0`),
`BEntityFilterSet_addEntityFilterIsIdle` (`0x140216DC0`),
`BTriggerEffect_teEntityFilterAddDiplomacy` (`0x1404615B0`),
`BEntityFilterSet_addEntityFilterRelationType` (`0x140217340`),
`BEntityFilterSet_validateAndTestUnit` (`0x1402180E0`),
`BEntityFilterSet_validateAndTestSquad` (`0x140218190`),
`BEntityFilterSet_filterUnits` (`0x1402182F0`), and
`BEntityFilterSet_filterSquads` (`0x1402184D0`).

`PlaySound` and `DebugVar(String)` are recognized presentation/debug effects.
Chat and cinematic requests are also counted as presentation work after their
checksummed request state is created. Rendering/audio code consumes that state
without implementing or duplicating trigger logic.

Unknown DBIDs are retained in loaded definitions. Unsupported conditions do
not pass accidentally, unsupported effects do not mutate state, and both are
reported as sorted unique DBID lists by `TriggerUpdate`.

The `scenario-trigger-audit` sim binary prints authored DBID catalogs and the
first authoritative update report for installed scenarios. Its campaign-wide
regression loads all 15 `PHXscn` missions plus both tutorials against their own
layered database and verifies the aliased variables remain typed. Prototype
object/squad lists resolve names against that same database; squad-list
multiplicity is retained because retail uses repeated entries as a creation
count. The regression also requires every campaign's initial trigger update to
terminate. This exposes handler gaps without mistaking a trigger parse failure
for gameplay coverage.

## Remaining parity work

- Implement the rest of the retail condition/effect catalog, especially
  objectives, the remaining list element types and advanced filters, AI,
  and powers.
- Model death presentation duration, scoring, full squad plot-search, object
  obstruction fallback, generic entity-target work, alternate order queues,
  AI-command abilities, turret hardpoints, and the remaining optional effect
  paths needed to reproduce every retail side effect rather than reporting a
  bounded fidelity gap.
- Add retail async-condition consensus and UI-input delivery. Async conditions
  currently remain in `Waiting` state.
- Load external, power, and ability trigger-script lifecycles in addition to
  embedded scenario systems.
- Purge invalidated entity handles from trigger lists during updates as retail
  does; scalar generation checks are already safe.
- Render and play all supported presentation-request kinds while keeping those
  adapters read-only except for explicit synchronized completion input.
