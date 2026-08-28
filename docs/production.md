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
- Completion creates a fully configured squad or standalone object through the
  same database-backed path used by scenario/debug spawning. The spawned entity
  records its trainer and optional shared train-limit bucket.
- Direct `Build` creates an unfinished building at the command transform (or a
  supplied concrete socket), pays immediately, tracks its creator/socket, and
  refunds only on an explicit cancel. Destruction consumes the cost.
- `BuildOther` pays and reserves future population at enqueue, consumes one
  source-worker slot, resolves shipped socket object types and child offsets,
  creates the target on promotion, delegates displayed progress to that child,
  and releases the reservation when the child finishes or dies. Occupied
  sockets become available again after cancellation/destruction.
- Construction population is live from entity creation, matching retail's
  squad-wrapper creation path, while population-cap additions remain deferred
  until `built`. Removal subtracts cap only when it was activated.
- `ManualBuild` targets wait for authoritative `add_build_points` calls.
  Unfinished targets receive the scenario database's construction-damage
  multiplier and set the tactic `TARGET_UNBUILT` state.
- `ConstructionProgress`, `built`, creator/socket links, work points, costs, and
  population reservations are deterministic checksum state available to UI and
  renderer consumers.
- UI/renderer consumers can read `TrainingProgress` and the normal sim entity
  roster; no presentation-side production or spawn state is maintained.
- Authoritative roster queries keep queued `TrainUnit` and `TrainSquad` work in
  separate future-count domains, matching retail player counters. Trigger and
  UI consumers therefore cannot accidentally count the members of a queued
  squad as individually queued units.
- Scenario loading checksums a proto-squad identity catalog. Future squad
  queries use it to match the database IDs held by trigger/live state against
  runtime table indices held by production tasks without conflating the two.

## Validation

The synthetic `building-production` and `building-construction` integration
suites cover squad/unit commands, both construction modes, socket transforms,
costs, population, deferred cap additions, shared worker ordering, manual work,
cancellation versus destruction refunds, command gates, train limits, and
checksums. The opt-in installed-data tests load Blood Gulch's scenario ERA and
layered database, build shipped `unsc_bldg_command_01` through the authored
`game_base_Socket_01`/`PowerSocketBase` path, train the effective Marine squad,
and verify that the renderer discovers sim-created members and projectiles from
sim state.

## Deliberate parity boundaries

- Exact parking-lot, birth-animation, garrison, obstruction evacuation, terrain
  raycasts, arbitrary placement-rule validation, and rally-point placement are
  not modeled yet. Training completion currently uses a deterministic point
  just outside the trainer's obstruction footprint. Construction uses authored
  socket types, child offsets/rotations, and target build offsets/rotations, but
  visual attach-bone transforms remain a renderer/asset integration boundary.
- `InstantTrainWithRecharge`, linked gather-resource reservations, dynamic unit
  cost escalation, co-op player production lanes, quick-build, and AI work-rate
  scaling are rejected or deferred rather than approximated silently.
- `NoBuildUnderAttack`, construction-queue parking, auto-parking-lot creation,
  child-object instantiation at `onBuilt`, and co-op purchasing-player color
  propagation remain explicit follow-up work.
- Scenario initialization does not yet activate every civilization, leader, and
  shadow technology that retail installs. Consequently, the `CommandEnable`
  state is authoritative once a tech is active, but the complete initial tech
  bootstrap remains future work.
