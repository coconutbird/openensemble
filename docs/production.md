# Building production

OpenEnsemble treats production as authoritative simulation state. Commands,
payment, population reservations, queue progress, entity creation, and cleanup
all live in `sim`; the renderer only synchronizes the resulting world roster and
transforms.

## Recovered retail contract

The implementation was cross-checked against the read-only 2008
`BUnitActionBuilding` source and the installed Halo Wars DE executable:

- `BBuildingCommand_processUnit` (`0x1401987D0`) validates the authored command
  pair. Types 1/3 route to `BUnit_doTrain` (`0x1404D1BE0`); type 2 creates an
  unfinished target and routes its own worker through `BUnit_doBuild`
  (`0x1404D1E60`); type 13 routes socket work through `BUnit_doBuildOther`
  (`0x1404D2070`). Command targets are runtime table indices, not authored
  DBIDs.
- `BUnitActionBuilding_addTrain` (`0x1404F63E0`) accepts the affordable,
  population-valid prefix of a positive count. It pays full cost and reserves
  future population for every accepted item. A negative count removes matching
  queued items from the tail before canceling the current item, with a full
  refund.
- Training, research, BuildOther, and building construction share the worker in
  `BUnitActionBuilding_update` (`0x1404F4B70`). Promoting the next item consumes
  an update; work starts on the following update.
- `BUnitActionBuilding_completeTrain` (`0x1404FAE30`) releases future population,
  creates the entity at the trainer/parking-lot transform, links train limits,
  and starts its birth/rally sequence.
- `BUnitActionBuilding_doBuild` (`0x1404F81D0`) makes the unfinished target own
  its direct construction worker. It pays at creation, keeps the target at full
  authored hit points, and refunds only an explicit construction cancellation.
- `BUnitActionBuilding_doBuildOther` (`0x1404F8420`) permits only one queued or
  active socket build on a source. It pays and reserves future population at
  enqueue; `BUnitActionBuilding_startBuildOther` (`0x1404F8A40`) selects the
  first compatible free socket and creates the linked unfinished child when the
  item is promoted.
- `BUnitActionBuilding_completeBuild` (`0x1404FA320`) reaches `BUnit::onBuilt`.
  Population-cap additions, resource/rate effects, persistent actions, LOS,
  child objects, and auto-training activate only at this transition. Shipped
  construction damage uses `GameData/ConstructionDamageMultiplier`.
- Quick-build multiplies work by 30, while normal AI work can use a player
  scalar. Both were confirmed in `BUnitActionBuilding_updateBuildAnimRate`
  (`0x1404FDEF0`).
- `BUnitActionSpawnSquad::update` and `completeSpawn` in the named 2008 source
  keep persistent spawn progress on the owning unit, use either authored
  `WorkRate` or the child squad's build points, draw synchronized work-rate
  variance, enforce the owner's squad train limit, and place the result before
  applying rally and auto-join behavior. `BUnit::doJoin` enables the join
  action's multiple-follower path for these spawned squads.
- `BUnitActionAirTrafficControl` owns exactly eight landing spots. Its first
  update derives their world positions from the air base and civilization,
  `requestLandingSpot` grants the first free entry, and the paired `MoveAir`
  action retains that reservation until it disconnects. UNSC Air Pads use two
  authored rows; Covenant controllers use an eight-position radial layout.

All substantively inspected routines were assigned repeatable comments and
descriptive names in `xgameFinal.exe.i64` before it was saved.

## Implemented authoritative behavior

- `TrainUnit`, `TrainSquad`, `Research`, `Build`, and `BuildOther` use the same
  authoritative worker model. An unfinished target is non-operational and
  cannot research, train, move, or attack.
- Positive training counts are partially accepted based on resources,
  population capacity, and authored train limits.
- Costs and future population are reserved at enqueue time. Cancellation,
  malformed in-flight data, and trainer destruction refund both.
- Population slots come from `GameData/Pops`, and leader cap/max values come
  from the selected leader. Authored scenario leaders are applied during load;
  skirmish SCNs that defer the choice to the lobby use
  `configure_player_leader` before placing starting forces. Scenario/debug
  spawns contribute live population and release it when their owning unit or
  squad is removed. Late lobby binding preserves live counts and reapplies cap
  additions from already-created starting buildings.
- Squad population is the rounded aggregate of its authored member-object
  population. Missing stock `Pop/@type` data is inferred as `Unit`, with explicit
  handling for Spartan, Leader, Temple, and Rhino population tables.
- `CommandEnable` technology effects can enable or disable a specific authored
  building command for one player's prototype state.
- Completion creates a fully configured squad through the same database-backed
  path used by scenario/debug spawning. `TrainUnit` also receives retail's
  hidden one-member parent squad. The spawned squad records its trainer and
  optional shared train-limit bucket.
- Completed squads are forcibly contained by their trainer or its live
  auto-parking-lot controller and enter a checksummed birth queue. At most one
  squad is released per update, including retail's exact-zero timer edge, so
  instant batches do not collapse into one presentation event.
- Birth placement consumes the effective scenario-layered squad and object
  definitions: `ExitFromDirection`, `BirthOnTop`, member-count footprint
  expansion, terrain height, preferred obstruction checks, and deterministic
  perimeter fallback all affect the authoritative release transform. Direct
  rally destinations are formation-plotted before release and resolve through
  controller, producer, then player rally state.
- Squad `Birth` member animations and trainer animations are discovered from
  the scenario-layered asset source. The sim owns their asset, revision, and
  timing state; the renderer only plays the resulting `ScriptedAnimation`.
- `Birth=FlyIn` uses two source-backed paths. Air squads begin 100 world units
  above their landing point, descend, and rally after arrival. An enabled
  persistent `AirTrafficControl` action redirects trained aircraft to its first
  free source-exact pad and facing. The eight checksummed reservations remain
  controller-owned after landing, release with the aircraft, and expose
  authoritative state to renderer/UI consumers. Ground squads
  use their player's civilization transport, remain contained during the
  incoming flight, unload and rally at the destination, and remove the carrier
  after its outgoing flight. Missing transport data falls back to ordinary
  ground birth.
- `InstantTrainWithRecharge` completes accepted items immediately, bypasses the
  shared worker, and applies a checksummed per-building command recharge.
  Linear and exponential object-cost escalation use live plus future roster
  counts; each queued item stores its paid quote so cancellation refunds the
  exact amount.
- Direct `Build` creates an unfinished building at the command transform (or a
  supplied concrete socket), pays immediately, tracks its creator/socket, and
  refunds only on an explicit cancel. Destruction consumes the cost.
- `BuildOther` pays and reserves future population at enqueue, consumes one
  source-worker slot, resolves shipped socket object types and child offsets,
  creates the target on promotion, delegates displayed progress to that child,
  and releases the reservation when the child finishes or dies. Occupied
  sockets become available again after cancellation/destruction.
- A promoted `BuildOther` from a `UseAutoParkingLot` builder materializes the
  target's authored `AutoParkingLot` object at its local offset and rotation.
  The association controls training birth and lifetime cleanup. Direct `Build`
  does not create this socket-construction helper.
- Construction population is live from entity creation, matching retail's
  squad-wrapper creation path, while population-cap additions remain deferred
  until `built`. Removal subtracts cap only when it was activated.
- The `onBuilt` transition also activates the prototype's scenario-layered
  `AddResource` and player `Rate` contributions. Each unit stores the exact
  applied slots and amounts for deterministic removal, death replacement,
  prototype transformation, and ownership transfer without consulting stale
  presentation or database state.
- `AutoTrainOnBuilt` queues one no-cost copy of the authored proto squad through
  the normal trigger-training path. The queue retains normal population,
  train-limit, member creation, birth, parking-lot, and rally behavior instead
  of spawning a presentation-only shortcut.
- Persistent `SpawnSquad` actions are collected from the scenario-layered tactic
  catalog and executed as authoritative unit state. Runtime technology/action
  enablement pauses progress, Gaia owners are excluded, authored `WorkRate`,
  build points, synchronized variance, finite counts, train limits and buckets,
  transformed player prototypes, terrain-aware placement, and matching rally
  points all feed the normal squad-spawn path. Spawned squads retain trainer
  provenance for roster limits and deterministic cleanup.
- `AutoJoin` issues the same persistent Join contract with retail's
  allow-multiple flag. Join range is measured between squad obstruction
  surfaces, not centers, so physical followers can connect; finite spawn counts
  track both pending and connected followers and reopen when one dies. The
  action state, variance, completion count, join mode, and provenance are all
  checksummed.
- The same transition materializes every authored child-object category at its
  local offset/rotation and terrain height while applying civilization gates.
  `Socket`, typed `ParkingLot`, `Foundation`, `Unit`, and default `Object`
  entries become authoritative entities with their retail relationship kind;
  `Rally` installs unit state, and `OneTimeSpawnSquad` routes through normal
  no-cost training exactly once per player. Default objects retain the target
  prototype's class, so building children remain class-1 entities while true
  object prototypes remain class 0.
- Foundation entries replace a prior foundation at the same position when a
  built prototype transforms. Other existing children reconcile by retail
  relationship and position, and `KillChildObjectsOnDeath` cascades regular or
  immediate parent death only to children whose `BuiltByUnit` still names that
  parent. Parking lots retain their separate owned lifetime. Unfinished
  buildings expose none of these future children or their default rally.
- A completed base child carrying `ChildForDamageTakenScalar` contributes to
  the source-backed command-building protection formula. The protected
  building receives `1 / ((child_count + 1) * scalar)` incoming damage, and
  construction, transformation, base membership, child death, and removal all
  recompute that checksummed layer. Blood Gulch's real first supply pad changes
  its command center from `1.0` to `0.5` and removal restores `1.0`.
- `ManualBuild` targets wait for authoritative `add_build_points` calls.
  Unfinished targets receive the scenario database's construction-damage
  multiplier and set the tactic `TARGET_UNBUILT` state.
- `ConstructionProgress`, `built`, creator/socket links, work points, costs, and
  population reservations are deterministic checksum state available to UI and
  renderer consumers.
- UI/renderer consumers can read `TrainingProgress` and the normal sim entity
  roster, containment, transport, rally, and animation state; no
  presentation-side production or spawn state is maintained.
- Authoritative roster queries keep queued `TrainUnit` and `TrainSquad` work in
  separate future-count domains, matching retail player counters. Trigger and
  UI consumers therefore cannot accidentally count the members of a queued
  squad as individually queued units.
- Scenario loading checksums a proto-squad identity catalog. Future squad
  queries use it to match the database IDs held by trigger/live state against
  runtime table indices held by production tasks without conflating the two.
- Scenario archives are loaded before database parsing. Scenario-local tables
  therefore participate in last-loaded-wins resolution, and that exact layered
  database builds the sim world, production definitions, triggers, gameplay
  animation catalog, and presentation content.

## Validation

The synthetic `building-production` and `building-construction` integration
suites cover squad/unit commands, both construction modes, built-time socket
and rally activation, socket transforms,
auto-parking transforms and cleanup, birth containment and release ordering,
placement/rally behavior, both FlyIn paths, scripted animations, instant
recharge, escalated costs, population, deferred cap additions, shared worker
ordering, manual work, cancellation versus destruction refunds, command gates,
train limits, and checksums. The opt-in installed-data tests load Blood Gulch's
scenario ERA and layered database, build shipped `unsc_bldg_command_01` through
the authored `game_base_Socket_01`/`PowerSocketBase` path, verify its foundation
and sockets, materialize shipped rebel-base Unit and Covenant default-building
children, replace the command-center foundation during its unique upgrade,
train the effective Marine and one-time leader squads, and exercise scenario
archive, layered database, and simulation loading together. Renderer projection
tests verify that presentation discovers sim-created members and projectiles
from sim state. A separate installed Blood Gulch regression verifies the exact
layered Prophet and Flood `SpawnSentinel` profiles, enables the Prophet action,
observes two effective monitors connect through `FollowAttack`, removes one,
and observes exactly one authoritative replacement reconnect.
A separate installed Blood Gulch regression verifies the real Air Pad and
Heavy Factory persistent profiles plus both civilization-specific landing-pad
layouts from the scenario-layered database and tactics.

## Deliberate parity boundaries

- Birth obstruction currently uses the sim's circle/AABB representation and a
  deterministic perimeter search, not retail's complete obstruction manager,
  terrain raycasts, or every arbitrary placement-rule validator. Exact visual
  birth/end-bone transforms, keyframe-driven action completion, birth audio and
  alerts, and attack-move rally behavior remain presentation/gameplay gaps.
  Construction uses authored socket types, child offsets/rotations, and target
  build offsets/rotations, but visual attach-bone transforms remain an
  asset-integration boundary.
- Air-traffic-control state now owns pad allocation and trained-aircraft birth
  placement, but the remaining `MoveAir` return-to-base, ammunition reload,
  launch, and full flight-controller state machine is still future work. A
  ninth simultaneous request is rejected by the controller; the current birth
  path retains its already-computed generic placement as a safe fallback for
  malformed content that exceeds the shipped eight-aircraft contract.
- Linked gather-resource reservations, co-op player production lanes, the
  synchronized QuickBuild command, and retail AI selection of work-rate scalars
  are deferred. The authoritative worker already consumes each building's
  checksummed work-rate scalar. Persistent `SpawnSquad` likewise does not yet
  apply retail's QuickBuild multiplier or AI build-speed scalar.
- Animated `SpawnSquad` profiles retain their animation and
  `HideSpawnUntilRelease` data, but currently create the squad at the
  authoritative release threshold. Retail's earlier hidden attached entity,
  attack-tag detach/reveal, and animation-end restart phases remain to be
  modeled before claiming exact animated-spawn timing parity.
- `NoBuildUnderAttack`, construction-queue parking, remaining persistent
  `onBuilt` action families, and co-op purchasing-player
  color propagation remain explicit follow-up work. Bone-handle attachment
  projection and retail's shared settlement/base child-reference fan-out also
  remain outside the current child-object slice.
- Scenario and lobby initialization activate civilization and leader
  technologies in retail order before normal starting-force placement.
  Eligible root and technology-dependent Shadow techs cascade in database
  order. Rechecking shadows solely in response to later unit-count changes
  remains future work.
